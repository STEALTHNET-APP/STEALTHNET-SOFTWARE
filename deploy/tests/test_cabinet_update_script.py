"""Exercise the updater in a private fixture, without touching host services."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class CabinetUpdateScriptTests(unittest.TestCase):
    def run_update(self, failure):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = root / 'commands'
            commands.mkdir()
            installed = root / 'installed'
            installed.mkdir()
            binary = installed / 'sn-cabinet'
            binary.write_text('#!/bin/sh\necho old-cabinet\n')
            binary.chmod(0o755)
            original = binary.read_bytes()
            env_file = root / 'env'
            env_file.write_text('PANEL_API_URL=https://panel.example.test\nCABINET_SERVICE_KEY=test-only-retained-key\n')
            old_env = env_file.read_bytes()
            candidate = root / 'candidate'
            candidate.write_text('#!/bin/sh\necho sn-cabinet test-new\n')
            digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
            curl = '''#!/usr/bin/env python3
import os,sys,pathlib,shutil
r=pathlib.Path(os.environ['FIXTURE']);args=sys.argv[1:]
if any(a.endswith('/ready') for a in args):
 p=r/'attempts';n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n))
 if n<=2 or os.environ['FAILURE']=='ready':
  print('curl: (7) startup connection refused',file=sys.stderr);sys.exit(7)
 print('{"status":"ready"}')
elif any(a.endswith('.sha256') for a in args):print(os.environ['DIGEST'])
else:shutil.copyfile(r/'candidate',args[args.index('-o')+1])
'''
            systemctl = '''#!/usr/bin/env python3
import os,pathlib,sys
r=pathlib.Path(os.environ['FIXTURE']);p=r/'restarts';n=int(p.read_text())+1 if p.exists() else 1;p.write_text(str(n))
sys.exit(1 if os.environ['FAILURE']=='restart' and n==1 else 0)
'''
            sha = '''#!/usr/bin/env python3
import hashlib,sys,pathlib
digest,name=sys.stdin.read().strip().split(None,1)
sys.exit(0 if hashlib.sha256(pathlib.Path(name).read_bytes()).hexdigest()==digest else 1)
'''
            for name, content in {'curl': curl, 'systemctl': systemctl, 'sha256sum': sha, 'sleep': '#!/bin/sh\nexit 0\n'}.items():
                path = commands / name
                path.write_text(content)
                path.chmod(0o755)
            source = (Path(__file__).resolve().parents[2] / 'web/update-cabinet.sh').read_text()
            source = source.replace('[[ $EUID -eq 0 && -f /etc/sn-cabinet/env ]]', '[[ -f /etc/sn-cabinet/env ]]')
            source = source.replace('/etc/sn-cabinet/env', str(env_file)).replace('/usr/local/bin', str(installed))
            result = subprocess.run(['bash', '-c', source], capture_output=True, text=True, timeout=15,
                                    env={**os.environ, 'PATH': str(commands) + ':' + os.environ['PATH'],
                                         'FIXTURE': directory, 'DIGEST': digest, 'FAILURE': failure})
            self.assertEqual(env_file.read_bytes(), old_env)
            self.assertNotIn('test-only-retained-key', result.stdout + result.stderr)
            if failure:
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(binary.read_bytes(), original)
                self.assertIn('Предыдущая сборка восстановлена', result.stdout)
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(binary.read_bytes(), candidate.read_bytes())
                self.assertNotIn('curl: (7)', result.stderr)
                self.assertIn('Кабинет обновлён', result.stdout)
            return result

    def test_transient_connection_errors_are_quiet(self):
        self.run_update('')

    def test_readiness_failure_is_reported_and_rolled_back(self):
        result = self.run_update('ready')
        self.assertIn('curl: (7)', result.stderr)

    def test_restart_failure_rolls_back(self):
        self.run_update('restart')

"""Exercise the actual node installer gates with captured bootstrap responses."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = (Path(__file__).resolve().parents[2] / 'web/install-node.sh').read_text()

@unittest.skipUnless(shutil.which('jq'), 'jq is required by the node installer')
class NodeInstallerTests(unittest.TestCase):
    def run_shell(self, code, bootstrap):
        with tempfile.TemporaryDirectory() as root:
            Path(root, 'bootstrap.json').write_text(json.dumps(bootstrap))
            prelude = 'set -euo pipefail\nTMP=$1\ndie() { echo "$*" >&2; exit 1; }\nsay() { echo "$*"; }\n'
            return subprocess.run(['bash', '-c', prelude + code, 'test', root], capture_output=True, text=True)

    def test_setup_accepts_free_nodes_and_rejects_assigned_empty_profiles(self):
        gate = SCRIPT.split('if jq -e', 1)[1].split('XRAY_VERSION=', 1)[0]
        gate = 'if jq -e' + gate
        for assigned, inbounds, expected in [(False, 0, 0), (True, 2, 0), (True, 0, 1)]:
            with self.subTest(assigned=assigned, inbounds=inbounds):
                result = self.run_shell(gate, {'profile_assigned': assigned, 'inbound_count': inbounds})
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_completion_waits_for_acknowledged_idle_engine(self):
        polling = SCRIPT.split('for attempt in {1..18}; do', 1)[1]
        polling = 'for attempt in {1..18}; do' + polling
        code = 'AGENT_VERSION=test; XRAY_VERSION=v26.9.9\nsleep() { :; }\nbootstrap() { return 0; }\n' + polling
        for body, expected, message in [
            ({'ready': False, 'profile_assigned': False, 'idle_ready': True}, 0, 'нода без профиля'),
            ({'ready': True, 'profile_assigned': True, 'idle_ready': False}, 0, 'Xray работает'),
            ({'ready': False, 'profile_assigned': False, 'idle_ready': False}, 1, 'ещё не подтвердила'),
            ({'ready': False, 'profile_assigned': False}, 1, 'ещё не подтвердила'),
        ]:
            with self.subTest(body=body):
                result = self.run_shell(code, body)
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertIn(message, result.stdout + result.stderr)

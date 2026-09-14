import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('service_manager', Path(__file__).resolve().parents[2] / 'web/service-manager.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class ServiceManagerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        for key, name in [('ROOT', 'project'), ('ENV_ROOT', 'etc'), ('BIN_ROOT', 'bin')]:
            directory = base / name
            directory.mkdir()
            mock = patch.object(M, key, directory)
            mock.start()
            self.addCleanup(mock.stop)

    def edge(self, name, origin='https://panel.example.test'):
        folder = M.ENV_ROOT / ('sn-' + name)
        folder.mkdir()
        key = 'PANEL_URL' if name == 'sub' else 'PANEL_API_URL'
        (folder / 'env').write_text(key + '=' + origin + '\nSECRET=test-retained\n')
        (M.BIN_ROOT / ('sn-' + name)).write_text('test binary')

    def test_entrypoints_work_for_both_components_and_are_idempotent(self):
        self.edge('sub')
        self.edge('cabinet')
        with patch('builtins.print'):
            M.install_entrypoints()
            first = (M.ROOT / 'Makefile').read_text()
            M.install_entrypoints()
        self.assertEqual(first, (M.ROOT / 'Makefile').read_text())
        for action in ('update', 'start', 'stop', 'restart', 'status'):
            result = subprocess.run(['make', '-n', action], cwd=M.ROOT, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('python3 ./service-manager.py ' + action, result.stdout)

    def test_panel_makefile_is_preserved(self):
        (M.ROOT / '.env').write_text('PANEL_URL=https://panel.example.test\n')
        (M.ROOT / 'Makefile').write_text('panel original')
        M.install_entrypoints()
        self.assertEqual((M.ROOT / 'Makefile').read_text(), 'panel original')

    def test_custom_makefile_is_not_replaced(self):
        self.edge('cabinet')
        (M.ROOT / 'Makefile').write_text('operator original')
        with self.assertRaises(ValueError): M.install_entrypoints()
        self.assertEqual((M.ROOT / 'Makefile').read_text(), 'operator original')

    def test_management_includes_only_installed_project_services(self):
        self.edge('cabinet', 'broken-address')
        (M.ROOT / '.env').write_text('PANEL_URL=https://panel.example.test\nBOT_TOKEN=\n')
        def systemctl(args, **kwargs):
            name=args[2]
            if '--property=LoadState' in args:
                return subprocess.CompletedProcess(args, 0, 'loaded\n')
            path = M.BIN_ROOT / name if name == 'sn-cabinet' else M.ROOT / 'current/bin' / name
            return subprocess.CompletedProcess(args, 0, '{ path=' + str(path) + ' ; }')
        with patch.object(M.subprocess, 'run', side_effect=systemctl):
            self.assertEqual(M.services(), ['sn-api', 'sn-sub', 'sn-worker', 'sn-cabinet'])

    def test_update_uses_each_components_panel_and_preserves_secrets(self):
        self.edge('cabinet', 'https://panel-one.example.test')
        self.edge('sub', 'https://panel-two.example.test')
        before = {p: p.read_bytes() for p in M.ENV_ROOT.rglob('env')}
        with patch.object(M, 'run') as run, patch('builtins.print'):
            M.update()
        downloads = [c.args[0] for c in run.call_args_list if c.args[0][0] == 'curl']
        self.assertEqual(len(downloads), 2)
        self.assertIn('https://panel-one.example.test/update-cabinet.sh', downloads[1])
        self.assertIn('https://panel-two.example.test/update-sub.sh', downloads[0])
        self.assertNotIn('test-retained', str(run.call_args_list))
        self.assertEqual(before, {p: p.read_bytes() for p in before})

    def test_update_rejects_non_https_origin_before_download(self):
        self.edge('sub', 'http://panel.example.test')
        with patch.object(M, 'run') as run:
            with self.assertRaises(ValueError): M.update()
            run.assert_not_called()

    def test_lifecycle_commands_target_services_without_changing_enablement(self):
        for action in ('start', 'stop', 'restart'):
            with patch.object(M, 'services', return_value=['sn-sub', 'sn-cabinet']), patch.object(M, 'run') as run, patch('builtins.print'):
                M.control(action)
                run.assert_called_once_with(['systemctl', action, 'sn-sub', 'sn-cabinet'])

    def test_failed_update_stops_before_next_component(self):
        self.edge('sub')
        self.edge('cabinet')
        with patch.object(M, 'run', side_effect=RuntimeError('download failed')) as run:
            with self.assertRaises(RuntimeError): M.update()
        self.assertEqual(run.call_count, 1)

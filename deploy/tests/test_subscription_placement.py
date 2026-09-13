"""Subscription placement must survive setup, doctor, upgrades and rollback."""
import importlib.util
import io
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('placement_installer', Path(__file__).resolve().parents[1] / 'installer.py')
I = importlib.util.module_from_spec(spec)
spec.loader.exec_module(I)


class SubscriptionPlacementTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.c = dict(panel_domain='panel.example.com', sub_domain='sub.example.com',
                      brand='Test', currency='usd', admin_user='owner', admin_password='test-password-long',
                      bot_token='', proxy='caddy', subscription_placement='remote')

    def test_legacy_configs_keep_local_placement_and_origins_normalize_safely(self):
        config = I.validate_config({**self.c, 'panel_domain':'https://Panel.Example.com/'})
        self.assertEqual(config['panel_domain'], 'panel.example.com')
        self.assertEqual(config['currency'], 'USD')
        self.assertFalse(I.local_subscription(config))
        del config['subscription_placement']
        self.assertTrue(I.local_subscription(I.validate_config(config)))
        self.assertIn('sub', I.managed_services(config))
        with self.assertRaises(I.InstallError):
            I.validate_config({**self.c, 'subscription_placement':'typo'})

    def test_remote_preflight_needs_only_panel_dns_and_port(self):
        def dns(name, port):
            if name != self.c['panel_domain']: raise socket.gaierror('remote DNS is not configured yet')
            return [(socket.AF_INET, socket.SOCK_STREAM, 6, '', ('192.0.2.1', 0))]
        with patch.object(I, 'port_free', side_effect=lambda port: port != 8081) as ports, \
             patch.object(I.Path, 'exists', return_value=False), patch.object(I.socket, 'getaddrinfo', side_effect=dns) as resolve, patch.object(I, 'ui'):
            I.preflight(self.c)
            self.assertEqual({call.args[0] for call in ports.call_args_list}, {80, 443, 8080})
            resolve.assert_called_once_with('panel.example.com', None)
            with self.assertRaises(I.InstallError):
                I.preflight({**self.c, 'subscription_placement':'local'})

    def test_remote_units_restart_and_proxy_do_not_install_local_subscription(self):
        writes = {}
        def atomic(path, text, mode): writes[str(path)] = text
        with patch.object(I, 'atomic', side_effect=atomic), patch.object(I, 'run') as run:
            I.units(self.c)
            I.restart({'BOT_TOKEN':''}, self.c)
        self.assertNotIn('/etc/systemd/system/sn-sub.service', writes)
        self.assertIn('/etc/systemd/system/sn-api.service', writes)
        self.assertFalse(any('sn-sub' in part for call in run.call_args_list for part in call.args[0]))
        subprocess.run(['bash', '-n'], input=writes['/usr/local/bin/stealthnet'], text=True, check=True)
        self.assertNotIn('status sn-api sn-sub', writes['/usr/local/bin/stealthnet'])
        config = I.caddy_config(self.c)
        self.assertIn('panel.example.com {', config)
        self.assertNotIn('sub.example.com', config)
        self.assertNotIn('127.0.0.1:8081', config)
        self.assertEqual(I.initial_env(self.c)['SUB_PUBLIC_URL'], 'https://sub.example.com')

    def test_remote_health_checks_panel_assets_but_never_waits_for_subscription(self):
        with patch.object(I, 'run') as run, patch.object(I, 'json_request') as request, \
             patch.object(I.urllib.request, 'urlopen', return_value=io.BytesIO(b'<!doctype html><title>Panel</title>')), \
             patch.object(I, 'web_assets') as assets, patch.object(I, 'ui'):
            I.health(self.c, {})
        self.assertEqual([call.args[0] for call in request.call_args_list],
                         ['http://127.0.0.1:8080/api/health', 'https://panel.example.com/api/health'])
        self.assertFalse(any('sn-sub' in call.args[0] for call in run.call_args_list))
        assets.assert_called_once()

    def test_remote_upgrade_and_rollback_preserve_placement_and_do_not_start_sub(self):
        old = self.root/'releases/v0.1.7'; old.mkdir(parents=True)
        new = self.root/'releases/v0.1.8'; new.mkdir()
        (self.root/'current').symlink_to(old)
        config = {**self.c, 'version':'v0.1.7'}
        (self.root/'installation.json').write_text(json.dumps(config))
        values = {'DATABASE_URL':'postgres://owner:test@localhost/db', 'BOT_TOKEN':''}
        I.atomic(self.root/'.env', I.env_text(values))
        original_env = (self.root/'.env').read_bytes()
        args = type('Args', (), {'release_dir':new})()
        for fail in (True, False):
            with self.subTest(rollback=fail), patch.object(I, 'ROOT', self.root), \
                 patch.object(I, 'private_json', return_value=config.copy()), \
                 patch.object(I, 'stage_release', return_value=new), patch.object(I, 'backup') as backup, \
                 patch.object(I, 'repair_proxy'), patch.object(I, 'run') as run, patch.object(I, 'ui'), \
                 patch.object(I, 'health', side_effect=I.InstallError('not ready') if fail else None):
                if fail:
                    with self.assertRaises(I.InstallError): I.update(args, {'version':'v0.1.8'})
                    self.assertEqual((self.root/'current').resolve(), old.resolve())
                else:
                    I.update(args, {'version':'v0.1.8'})
                    self.assertEqual((self.root/'current').resolve(), new.resolve())
                self.assertFalse(any('sn-sub' in str(part) for call in run.call_args_list for part in call.args[0]))
                backup.assert_called_once()
            self.assertEqual((self.root/'.env').read_bytes(), original_env)
            self.assertEqual(json.loads((self.root/'installation.json').read_text())['subscription_placement'], 'remote')


if __name__ == '__main__': unittest.main()

import contextlib
import importlib.util
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch
import urllib.request

DEPLOY=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('installer_caddy', DEPLOY/'installer.py')
I=importlib.util.module_from_spec(spec)
spec.loader.exec_module(I)


class ProxyUpgradeTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.root=Path(self.temp.name)
        self.main=self.root/'Caddyfile'
        self.snippet=self.root/'panel.caddy'
        self.config={'panel_domain':'panel.example.com','sub_domain':'sub.example.com','proxy':'caddy'}
        self.old=I.caddy_config(self.config).replace('@sn_miniapp path /app /app/*\n    handle @sn_miniapp {','handle /app* {')+'\n# Operator note\n'
        self.snippet.write_text(self.old)
        self.snippet.chmod(0o640)
        self.main.write_text('import '+str(self.snippet)+'\n# Other sites are preserved\n')

    def tearDown(self):
        self.temp.cleanup()

    def repair(self):
        return I.repair_proxy(self.config,main=self.main,snippet=self.snippet)

    def test_upgrade_repairs_only_legacy_route_and_is_idempotent(self):
        before_main=self.main.read_bytes()
        with patch.object(I,'run') as run, patch.object(I,'ui'):
            self.assertTrue(self.repair())
            self.assertFalse(self.repair())
        self.assertEqual(run.call_count,2)
        self.assertEqual(self.main.read_bytes(),before_main)
        self.assertIn('# Operator note',self.snippet.read_text())
        self.assertEqual(self.snippet.stat().st_mode&0o777,0o640)
        self.assertEqual(self.snippet.read_text().replace('@sn_miniapp path /app /app/*\n    handle @sn_miniapp {','handle /app* {'),self.old)

    def test_validation_or_reload_failure_restores_previous_configuration(self):
        for failures in ((I.InstallError('invalid'),None),(None,I.InstallError('reload'),None)):
            with self.subTest(failures=len(failures)), patch.object(I,'run',side_effect=failures), self.assertRaises(I.InstallError):
                self.repair()
            self.assertEqual(self.snippet.read_text(),self.old)

    def test_external_proxy_and_unmanaged_file_are_not_modified(self):
        self.config['proxy']='external'
        with patch.object(I,'run') as run:
            self.assertFalse(self.repair())
            self.config['proxy']='caddy'
            self.snippet.write_text('# Custom configuration\n'+self.old)
            self.assertFalse(self.repair())
            run.assert_not_called()
        self.assertEqual(self.snippet.read_text(),'# Custom configuration\n'+self.old)


class CaddyRoutesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.caddy=shutil.which('caddy')
        if not cls.caddy:
            if os.environ.get('CI'): raise RuntimeError('Install Caddy to run release routing checks')
            raise unittest.SkipTest('Caddy is required for HTTP routing integration checks')

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.root=Path(self.temp.name)
        self.web=self.root/'current/web'
        self.web.mkdir(parents=True)
        for name,body in {'index.html':'<!doctype html><title>Panel</title>', 'app.css':'body { color: teal; }',
                          'core.js':'window.panelLoaded = true;', 'miniapp-unavailable.html':'<!doctype html><title>Mini App unavailable</title>'}.items():
            (self.web/name).write_text(body)
        (self.root/'shared/public').mkdir(parents=True)
        (self.root/'shared/public/logo.svg').write_text('<svg>operator branding</svg>')
        with contextlib.ExitStack() as stack:
            sockets=[stack.enter_context(socket.socket()) for _ in range(2)]
            for s in sockets: s.bind(('127.0.0.1',0))
            self.port,subport=[s.getsockname()[1] for s in sockets]
        self.url=f'http://127.0.0.1:{self.port}'
        self.config={'panel_domain':self.url,'sub_domain':f'http://127.0.0.1:{subport}','proxy':'caddy'}
        self.main=self.root/'Caddyfile'
        self.snippet=self.root/'panel.caddy'
        self.main.write_text('{\n admin off\n auto_https off\n}\nimport '+str(self.snippet)+'\n')

    def tearDown(self):
        self.temp.cleanup()

    @contextlib.contextmanager
    def server(self):
        with (self.root/'caddy.log').open('w+') as log:
            process=subprocess.Popen([self.caddy,'run','--config',str(self.main),'--adapter','caddyfile'],stdout=log,stderr=log,
                                     env={**os.environ,'XDG_DATA_HOME':str(self.root/'data'),'XDG_CONFIG_HOME':str(self.root/'config')})
            try:
                for _ in range(100):
                    try:
                        with urllib.request.urlopen(self.url+'/',timeout=.2): break
                    except OSError:
                        if process.poll() is not None:
                            log.seek(0); self.fail(log.read())
                        time.sleep(.05)
                else: self.fail('Isolated Caddy did not become ready')
                yield
            finally:
                process.terminate()
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: process.kill(); process.wait()

    def assert_routes(self):
        I.web_assets(self.url,self.web)
        for path in ('/app','/app/','/app/nested?from=telegram'):
            with urllib.request.urlopen(self.url+path) as response:
                self.assertEqual(response.read(),(self.web/'miniapp-unavailable.html').read_bytes())
        with urllib.request.urlopen(self.url+'/application') as response:
            self.assertEqual(response.read(),(self.web/'index.html').read_bytes())
        with urllib.request.urlopen(self.url+'/custom/logo.svg') as response:
            self.assertIn(b'operator branding',response.read())

    def test_fresh_install_and_legacy_upgrade_with_real_caddy(self):
        with patch.object(I,'ROOT',self.root): fresh=I.caddy_config(self.config)
        self.snippet.write_text(fresh)
        with self.server(): self.assert_routes()
        legacy=fresh.replace('@sn_miniapp path /app /app/*\n    handle @sn_miniapp {','handle /app* {')
        self.snippet.write_text(legacy)
        with self.server(), self.assertRaisesRegex(I.InstallError,'app.css'):
            I.web_assets(self.url,self.web)
        def run(args,**kwargs):
            if args[0]=='caddy': subprocess.run([self.caddy,*args[1:]],check=True,capture_output=True)
        with patch.object(I,'run',side_effect=run), patch.object(I,'ui'):
            self.assertTrue(I.repair_proxy(self.config,main=self.main,snippet=self.snippet))
        with self.server(): self.assert_routes()

    def test_container_template_keeps_panel_assets_outside_mini_app(self):
        template=(DEPLOY/'Caddyfile').read_text().replace('{$PANEL_DOMAIN}',self.config['panel_domain']).replace('{$SUB_DOMAIN}',self.config['sub_domain']).replace('/srv/web',str(self.web))
        self.snippet.write_text(template)
        with self.server():
            I.web_assets(self.url,self.web)
            with urllib.request.urlopen(self.url+'/app') as response:
                self.assertEqual(response.read(),(self.web/'miniapp-unavailable.html').read_bytes())

    def test_remote_subscription_has_no_local_proxy_site(self):
        self.config['subscription_placement']='remote'
        with patch.object(I,'ROOT',self.root):
            self.snippet.write_text(I.caddy_config(self.config))
        # A remote subscription must not appear among Caddy's managed hosts.
        adapted=subprocess.check_output([self.caddy,'adapt','--config',str(self.main),'--adapter','caddyfile'],text=True)
        self.assertNotIn('127.0.0.1:8081',adapted)
        with self.server(): self.assert_routes()


if __name__=='__main__': unittest.main()

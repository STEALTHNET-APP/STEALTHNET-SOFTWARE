import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('local_cabinet_installer',Path(__file__).resolve().parents[1]/'installer.py')
I=importlib.util.module_from_spec(spec);spec.loader.exec_module(I)

class LocalCabinetTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        self.binary=self.root/'sn-cabinet';self.binary.write_bytes(b'old cabinet');self.binary.chmod(0o755)
        self.env=self.root/'env';I.atomic(self.env,'PANEL_API_URL=https://panel.example.test\nCABINET_SERVICE_KEY=keep-this-test-key\nCABINET_BIND=127.0.0.1:8090\n')
        self.before=self.env.read_bytes();self.new=self.root/'releases/v0.2.5';(self.new/'bin').mkdir(parents=True);(self.new/'bin/sn-cabinet').write_bytes(b'new cabinet')
        self.backup=self.root/'backup';self.backup.mkdir()
        for key,value in [('ROOT',self.root),('CABINET_BIN',self.binary),('CABINET_ENV',self.env)]:
            p=patch.object(I,key,value);p.start();self.addCleanup(p.stop)
    def service(self,args,**kw):
        if '--property=LoadState' in args:return 'loaded'
        if '--property=ExecStart' in args:return '{ path='+str(self.binary)+' ; argv[]='+str(self.binary)+' ; }'
        if 'is-active' in args:return 'active'
        if 'is-enabled' in args:return 'enabled'
        return ''
    def test_only_cabinet_for_this_panel_is_selected(self):
        with patch.object(I,'run',side_effect=self.service):
            c=I.local_cabinet({'PANEL_URL':'https://panel.example.test/'})
            self.assertTrue(c['restart']);self.assertEqual(c['ready'],'http://127.0.0.1:8090/ready')
        with patch.object(I,'run') as run:
            self.assertIsNone(I.local_cabinet({'PANEL_URL':'https://different.example.test'}));run.assert_not_called()
    def test_success_preserves_env_and_keeps_previous_binary(self):
        with patch.object(I,'run',side_effect=self.service) as run,patch.object(I,'cabinet_ready') as ready,patch.object(I,'ui'):
            c=I.local_cabinet({'PANEL_URL':'https://panel.example.test'})
            I.update_local_cabinet(c,self.new,self.backup)
            self.assertEqual(self.binary.read_bytes(),b'new cabinet');self.assertEqual(self.env.read_bytes(),self.before)
            self.assertEqual(c['backup'].read_bytes(),b'old cabinet');self.assertEqual((self.backup/'cabinet/env').read_bytes(),self.before)
            self.assertEqual((self.backup/'cabinet/env').stat().st_mode&0o777,0o600)
            ready.assert_called_once_with('http://127.0.0.1:8090/ready',attempts=30)
    def test_disabled_service_is_not_started(self):
        c={'restart':False,'ready':'http://127.0.0.1:8090/ready','changed':False,'backup':None}
        with patch.object(I,'run') as run,patch.object(I,'cabinet_ready') as ready,patch.object(I,'ui'):
            I.update_local_cabinet(c,self.new,self.backup);self.assertEqual(self.binary.read_bytes(),b'new cabinet');run.assert_not_called();ready.assert_not_called()
    def fixture(self,version):
        old=self.root/'releases'/version;(old/'bin').mkdir(parents=True,exist_ok=True);(old/'bin/sn-cabinet').write_bytes(b'new cabinet' if version=='v0.2.5' else b'old cabinet')
        (self.root/'current').symlink_to(old);I.atomic(self.root/'installation.json',json.dumps({'version':version,'proxy':'external'}))
        values={'PANEL_URL':'https://panel.example.test','DATABASE_URL':'postgres://qa:qa@localhost/audit','CABINET_CODE_KEY':'keep-panel-test-key'}
        I.atomic(self.root/'.env',I.env_text(values));return old,values
    def test_cabinet_failure_rolls_back_panel_and_cabinet(self):
        old,values=self.fixture('v0.2.4');args=type('Args',(),{'release_dir':self.new})()
        with patch.object(I,'run',side_effect=self.service),patch.object(I,'private_json',return_value={'version':'v0.2.4','proxy':'external'}),patch.object(I,'stage_release',return_value=self.new),patch.object(I,'backup',return_value=self.backup),patch.object(I,'restart') as restart,patch.object(I,'repair_proxy'),patch.object(I,'health'),patch.object(I,'cabinet_ready',side_effect=I.InstallError('not ready')),patch.object(I,'ui'):
            with self.assertRaises(I.InstallError):I.update(args,{'version':'v0.2.5'})
            self.assertEqual((self.root/'current').resolve(),old.resolve());self.assertEqual(self.binary.read_bytes(),b'old cabinet');self.assertEqual(self.env.read_bytes(),self.before);self.assertEqual(restart.call_count,2)
            self.assertEqual(json.loads((self.root/'installation.json').read_text())['version'],'v0.2.4')
    def test_repeating_same_panel_version_updates_stale_cabinet(self):
        old,values=self.fixture('v0.2.5');args=type('Args',(),{'release_dir':old})()
        with patch.object(I,'run',side_effect=self.service),patch.object(I,'private_json',return_value={'version':'v0.2.5','proxy':'external'}),patch.object(I,'backup',return_value=self.backup),patch.object(I,'validate_release'),patch.object(I,'health'),patch.object(I,'cabinet_ready'),patch.object(I,'ui'):
            I.update(args,{'version':'v0.2.5'});self.assertEqual(self.binary.read_bytes(),b'new cabinet');self.assertEqual(self.env.read_bytes(),self.before)

    def test_ready_endpoint_uses_actual_cabinet_plain_text_contract(self):
        replies = [(503, b'upstream unavailable'), (200, b'ready')]
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def do_GET(self):
                status, body = replies.pop(0)
                self.send_response(status);self.end_headers();self.wfile.write(body)
        server=HTTPServer(('127.0.0.1',0),Handler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try:
            url=f'http://127.0.0.1:{server.server_port}/ready'
            with patch.object(I.time,'sleep'):
                I.cabinet_ready(url,attempts=2)
                replies.append((200,b'{"status":"ready"}'))
                with self.assertRaises(I.InstallError): I.cabinet_ready(url,attempts=1)
        finally:
            server.shutdown();server.server_close();thread.join()

import importlib.util
import io
import socket
import ssl
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import threading
import unittest
from unittest.mock import patch
import urllib.error

spec=importlib.util.spec_from_file_location('installer_health', Path(__file__).resolve().parents[1]/'installer.py')
I=importlib.util.module_from_spec(spec)
spec.loader.exec_module(I)


class HealthHandler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        bodies={'/ok': b'{"status":"ok","db":true}', '/html': b'<html>private-response</html>',
                '/array': b'[]', '/unready': b'{"status":"ok","db":false}'}
        self.send_response(200 if self.path in bodies else 502)
        self.end_headers()
        self.wfile.write(bodies.get(self.path, b'private-response'))


class InstallerHealthTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server=HTTPServer(('127.0.0.1', 0), HealthHandler)
        cls.thread=threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.url=f'http://127.0.0.1:{cls.server.server_port}'

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def test_real_http_health_and_unexpected_responses(self):
        I.json_request(self.url+'/ok', {'status':'ok','db':True}, attempts=1)
        for path, message in (('/html','вместо JSON'), ('/array','неожиданный статус'),
                              ('/unready','неожиданный статус'), ('/bad-gateway','HTTP 502')):
            with self.subTest(path=path), self.assertRaises(I.InstallError) as error:
                I.json_request(self.url+path, {'status':'ok','db':True}, attempts=1)
            self.assertIn(message, str(error.exception))
            self.assertIn(self.url+path, str(error.exception))
            self.assertNotIn('private-response', str(error.exception))

    def test_wrapped_network_errors_keep_the_specific_cause(self):
        for reason, message in ((ConnectionRefusedError(111, 'private-error'), 'соединение отклонено'),
                                (socket.gaierror(-2, 'private-error'), 'DNS-записи'),
                                (TimeoutError('private-error'), 'время ожидания'),
                                (ssl.SSLCertVerificationError('private-error'), 'сертификат HTTPS'),
                                (ssl.SSLError('private-error'), 'TLS-соединение'),
                                (ConnectionResetError('private-error'), 'сбросил соединение')):
            with self.subTest(reason=type(reason).__name__), patch.object(I.urllib.request, 'urlopen', side_effect=urllib.error.URLError(reason)), self.assertRaises(I.InstallError) as error:
                I.json_request('https://panel.example.com/api/health', {'status':'ok'}, attempts=1)
            self.assertIn(message, str(error.exception))
            self.assertNotIn('private-error', str(error.exception))

    def test_temporary_connection_failure_retries_and_recovers(self):
        responses=[urllib.error.URLError(ConnectionRefusedError()), io.BytesIO(b'{"status":"ok"}')]
        with patch.object(I.urllib.request,'urlopen',side_effect=responses) as request, patch.object(I.time,'sleep') as sleep:
            I.json_request('https://panel.example.com/api/health', {'status':'ok'}, attempts=2)
        self.assertEqual(request.call_count, 2)
        sleep.assert_called_once_with(1)

    def test_http_error_response_is_closed_without_reading_its_body(self):
        body=io.BytesIO(b'private-response')
        error=urllib.error.HTTPError('https://panel.example.com/api/health', 502, 'private-error', {}, body)
        self.assertIn('HTTP 502', I.health_error(error))
        self.assertTrue(body.closed)

    def test_exhausted_retries_report_cause_without_final_delay(self):
        with patch.object(I.urllib.request,'urlopen',side_effect=urllib.error.URLError(TimeoutError())) as request, patch.object(I.time,'sleep') as sleep, self.assertRaises(I.InstallError) as error:
            I.json_request('https://panel.example.com/api/health', {'status':'ok'}, attempts=3)
        self.assertEqual(request.call_count, 3)
        self.assertEqual(sleep.call_count, 2)
        self.assertIn('время ожидания', str(error.exception))

    def test_public_failure_explains_local_success_and_caddy_checks(self):
        c={'panel_domain':'panel.example.com','sub_domain':'sub.example.com','proxy':'caddy'}
        with patch.object(I,'run'), patch.object(I,'ui'), patch.object(I,'json_request', side_effect=[None,None,I.InstallError('соединение отклонено')]), self.assertRaises(I.InstallError) as error:
            I.health(c,{})
        message=str(error.exception)
        self.assertIn('соединение отклонено', message)
        self.assertIn('Локальные службы, API и база прошли проверку', message)
        self.assertIn('systemctl is-active caddy', message)
        self.assertIn('journalctl -u caddy', message)

    def test_local_failure_does_not_claim_services_passed(self):
        with patch.object(I,'run'), patch.object(I,'json_request', side_effect=I.InstallError('база недоступна')), self.assertRaises(I.InstallError) as error:
            I.health({'proxy':'caddy'},{})
        self.assertEqual(str(error.exception), 'база недоступна')

    def test_html_fetch_uses_same_network_diagnostics(self):
        c={'panel_domain':'panel.example.com','sub_domain':'sub.example.com','proxy':'caddy'}
        with patch.object(I,'run'), patch.object(I,'ui'), patch.object(I,'json_request'), patch.object(I.urllib.request,'urlopen',side_effect=urllib.error.URLError(TimeoutError())), self.assertRaises(I.InstallError) as error:
            I.health(c,{})
        self.assertIn('https://panel.example.com/', str(error.exception))
        self.assertIn('время ожидания', str(error.exception))
        self.assertIn('journalctl -u caddy', str(error.exception))


if __name__=='__main__':
    unittest.main()

"""Run the installer's real Bash/curl/jq preflight against a local HTTP panel."""
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import os
import subprocess
import tempfile
import threading
import unittest


class CabinetPreflightTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.status = 200
        cls.body = b'{}'
        cls.received_key = None

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                cls.received_key = self.headers.get('x-cabinet-service-key')
                self.send_response(cls.status)
                self.end_headers()
                self.wfile.write(cls.body)

        cls.server = HTTPServer(('127.0.0.1', 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        source = (Path(__file__).resolve().parents[2] / 'web/install-cabinet.sh').read_text()
        # Only the function definitions: never run apt, systemd or the installer on the test host.
        cls.functions = source.split('[[ $EUID -eq 0', 1)[0]

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def run_preflight(self, status, body):
        type(self).status = status
        type(self).body = body.encode()
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(['bash', '-c', self.functions + '\ncheck_cabinet_publication\nprintf "READY\\n"'],
                                    env={**os.environ, 'TMP': tmp, 'PANEL_API_URL': f'http://127.0.0.1:{self.server.server_port}',
                                         'CABINET_SERVICE_KEY': 'cabinet-test-key-do-not-print'},
                                    capture_output=True, text=True, timeout=10)
        self.assertEqual(type(self).received_key, 'cabinet-test-key-do-not-print')
        self.assertNotIn('cabinet-test-key-do-not-print', result.stdout + result.stderr)
        return result

    def test_unpublished_then_publish_and_retry_same_key(self):
        result = self.run_preflight(400, '{"error":"Сайт ещё не опубликован"}')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('HTTP 400', result.stderr)
        self.assertIn('Брендинг и содержимое', result.stderr)
        self.assertIn('Сохранить настройки', result.stderr)
        self.assertIn('ту же команду', result.stderr)
        self.assertNotIn('READY', result.stdout)
        result = self.run_preflight(200, '{"enabled":true,"brand":"Test portal"}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, 'READY\n')

    def test_wrong_key_has_different_recovery(self):
        for status in (401, 403):
            result = self.run_preflight(status, '{"error":"Unauthorized"}')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('актуальную команду', result.stderr)
            self.assertNotIn('ещё не опубликован', result.stderr)

    def test_proxy_and_unexpected_responses_do_not_expose_body(self):
        for status in (200, 400, 404, 502):
            for body in ('<html>private-response\u001b[31m</html>', '{"error":"cabinet-test-key-do-not-print"}'):
                result = self.run_preflight(status, body)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('private-response', result.stderr)
                self.assertNotIn('\u001b', result.stderr)
                self.assertNotIn('READY', result.stdout)

    def test_success_requires_published_brand(self):
        for body in ('{}', '[]', '{"enabled":false,"brand":"Test"}', '{"enabled":true,"brand":false}'):
            result = self.run_preflight(200, body)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('не подтвердила публикацию', result.stderr)


if __name__ == '__main__':
    unittest.main()

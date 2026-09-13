"""Opt-in end-to-end API regression against an isolated audit database.

SN_CABINET_QA_URL: a local API using that database.
SN_CABINET_QA_CREDENTIALS: private JSON with db_name, database_url, username, password.
Requires psycopg. Never point this test at an installation used by customers.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import secrets
import unittest
import urllib.error
import urllib.parse
import urllib.request
import uuid


@unittest.skipUnless(os.environ.get('SN_CABINET_QA_URL'), 'requires isolated cabinet API and audit database')
class CabinetRecoveryApiTests(unittest.TestCase):
    def test_revoked_selection_publication_recovery_and_concurrent_save(self):
        import psycopg
        credentials = json.loads(Path(os.environ['SN_CABINET_QA_CREDENTIALS']).read_text())
        base = os.environ['SN_CABINET_QA_URL'].rstrip('/')
        self.assertIn(urllib.parse.urlsplit(base).hostname, ('127.0.0.1', 'localhost', '::1'))
        self.assertIn('audit', credentials['db_name'])
        self.assertEqual(urllib.parse.urlsplit(credentials['database_url']).path.lstrip('/'), credentials['db_name'])
        admin = {}
        ids = []

        def request(path, method='GET', body=None, headers=None, expected=200):
            req = urllib.request.Request(base + path, method=method,
                data=None if body is None else json.dumps(body).encode(),
                headers={'Content-Type': 'application/json', **(admin if headers is None else headers)})
            try:
                response = urllib.request.urlopen(req, timeout=15)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                self.assertEqual(response.status, expected, path)
                return json.load(response)

        login = request('/api/auth/login', 'POST', {'username': credentials['username'], 'password': credentials['password']}, {})
        admin['Authorization'] = 'Bearer ' + login['token']
        with psycopg.connect(credentials['database_url']) as db:
            original = db.execute("SELECT value FROM settings WHERE key='cabinet.config'").fetchone()[0]

        def raw_config():
            with psycopg.connect(credentials['database_url']) as db:
                return db.execute("SELECT value FROM settings WHERE key='cabinet.config'").fetchone()[0]

        def issue(ident):
            return request(f'/api/cabinet-service/installations/{ident}/token', 'POST', {})['token']

        def revoke(ident):
            return request(f'/api/cabinet-service/installations/{ident}/revoke', 'POST', {})

        def save(config):
            return request('/api/cabinet-service', 'PATCH', config)

        try:
            save({'enabled': False, 'registration_enabled': False})
            config = request('/api/cabinet-service')['config']
            for key in ('logo', 'logo_dark', 'favicon'):
                config[key] = 'https://panel.example.test/customer-brand/starter.svg'
            for _ in range(2):
                ident = request('/api/cabinet-service/installations', 'POST', {
                    'name': 'Recovery ' + secrets.token_hex(5), 'public_url': f'https://{uuid.uuid4().hex}.example.test',
                    'server_ip': '192.0.2.10', 'placement': 'same'})['id']
                ids.append(ident)
            a, b = ids
            old_key = issue(a)
            issue(b)
            selected = {**config, 'enabled': True, 'miniapp_installation_id': a}
            save(selected)
            request('/api/cabinet/config', headers={'x-cabinet-service-key': old_key})
            revoke(a)
            self.assertTrue(raw_config()['enabled'])
            self.assertEqual(raw_config()['miniapp_installation_id'], '')
            request('/api/cabinet/config', headers={'x-cabinet-service-key': old_key}, expected=401)

            # A form opened before the revocation still saves publication and custom text.
            result = save({**selected, 'headline': 'Preserved custom headline'})
            self.assertTrue(result['miniapp_selection_cleared'])
            self.assertEqual(result['miniapp_installation_id'], '')
            self.assertEqual(raw_config()['headline'], 'Preserved custom headline')
            self.assertTrue(raw_config()['enabled'])

            # Reproduce a v0.2.2 installation whose revoked selection is already persisted.
            legacy = {**selected, 'enabled': False}
            with psycopg.connect(credentials['database_url']) as db:
                db.execute("UPDATE settings SET value=%s::jsonb WHERE key='cabinet.config'", (json.dumps(legacy),))
            form = request('/api/cabinet-service')
            self.assertTrue(form['miniapp_selection_cleared'])
            self.assertEqual(form['config']['miniapp_installation_id'], '')
            self.assertFalse(raw_config()['enabled'])

            # Recover a key with publication disabled; wait for a new report before saying online.
            new_key = issue(a)
            self.assertNotEqual(new_key, old_key)
            entry = next(x for x in request('/api/cabinet-service')['installations'] if x['id'] == a)
            self.assertIsNone(entry['revoked_at'])
            self.assertIsNone(entry['last_seen_at'])
            self.assertFalse(raw_config()['enabled'])
            request('/api/cabinet/config', headers={'x-cabinet-service-key': new_key}, expected=400)
            save({**form['config'], 'enabled': True})
            request('/api/cabinet/config', headers={'x-cabinet-service-key': new_key})
            request('/api/cabinet/config', headers={'x-cabinet-service-key': old_key}, expected=401)

            # Revoking another server leaves the current Mini App selection intact.
            save({**selected, 'miniapp_installation_id': b})
            revoke(a)
            self.assertEqual(raw_config()['miniapp_installation_id'], b)
            result = save({**selected, 'miniapp_installation_id': str(uuid.uuid4())})
            self.assertTrue(result['miniapp_selection_cleared'])
            self.assertTrue(raw_config()['enabled'])

            # Either operation may win the race, but a revoked ID must never remain selected.
            for _ in range(4):
                issue(a)
                save(selected)
                with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                    pending = [pool.submit(save, selected), pool.submit(revoke, a)]
                    for future in pending:
                        future.result()
                self.assertEqual(raw_config()['miniapp_installation_id'], '')
                self.assertTrue(raw_config()['enabled'])
        finally:
            with psycopg.connect(credentials['database_url']) as db:
                db.execute("UPDATE settings SET value=%s::jsonb WHERE key='cabinet.config'", (json.dumps(original),))
                for ident in ids:
                    db.execute('DELETE FROM cabinet_installations WHERE id=%s', (ident,))


if __name__ == '__main__':
    unittest.main()

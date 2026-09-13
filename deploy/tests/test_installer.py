import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

DEPLOY=Path(__file__).resolve().parents[1]
def module(name,file):
    spec=importlib.util.spec_from_file_location(name,DEPLOY/file)
    mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod);return mod
I=module('installer','installer.py')
PG=module('pg_env','pg-env.py')

class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.c=dict(panel_domain='panel.example.com',sub_domain='sub.example.com',brand='Test "Network"',currency='USD',admin_user='owner',admin_password='Long$pass"word\\123',bot_token='',proxy='external')
    def tearDown(self):self.temp.cleanup()
    def test_selected_release_survives_distribution_metadata(self):
        # Run the actual CLI parser, then source the same fields supplied by
        # /etc/os-release. OS VERSION must never replace the application tag.
        script=(DEPLOY.parent/'install.sh').read_text().split('[[ $EUID -eq 0 ]]',1)[0]
        for distro in ('13','24.04','26.04'):
            metadata=self.root/'os-release';metadata.write_text(f'ID=ubuntu\nVERSION="{distro} LTS"\nVERSION_ID="{distro}"\n')
            for args,expected in (([], 'latest'),(['--version','v0.1.1'],'v0.1.1')):
                with self.subTest(distro=distro,args=args):
                    command=script+f'\n. "{metadata}"\nprintf "%s" "$SN_RELEASE_VERSION"\n'
                    result=subprocess.run(['bash','-c',command,'installer',*args],text=True,capture_output=True,check=True)
                    self.assertEqual(result.stdout,expected)
    def test_domains_and_config_injection(self):
        for value in ('x\nroot * /','example.com:443','https://example.com','a..com','-a.example.com','a.example.com/','a.example.com"','127.0.0.1'):
            with self.subTest(value=value),self.assertRaises(I.InstallError):I.validate_config({**self.c,'panel_domain':value})
        self.assertEqual(I.validate_config(self.c)['brand'],self.c['brand'])
        with self.assertRaises(I.InstallError):I.validate_config({**self.c,'sub_domain':self.c['panel_domain']})
        with self.assertRaises(I.InstallError):I.validate_config({**self.c,'unknown':'secret'})
    def test_password_and_currency(self):
        for key,value in [('admin_password','short'),('admin_password','x'*129),('admin_user',"root';--"),('currency','XTR'),('bot_token','not-a-token'),('brand','a\nB=secret')]:
            with self.subTest(key=key),self.assertRaises(I.InstallError):I.validate_config({**self.c,key:value})
    def test_env_round_trip_preserves_special_secrets(self):
        env={'DATABASE_URL':'postgres://user:p%24ss@localhost/db','BOT_TOKEN':'$quote"\\value','BRAND_NAME':"O'Neil & Friends"}
        p=self.root/'.env';I.atomic(p,I.env_text(env))
        self.assertEqual(I.read_env(p),env)
        self.assertEqual(p.stat().st_mode&0o777,0o600)
    def test_pg_credentials_are_environment(self):
        env=PG.pg_env('postgresql://user:p%24ss%27%5C@127.0.0.1:5433/db?sslmode=require')
        self.assertEqual(env['PGPASSWORD'],"p$ss'\\")
        self.assertEqual(env['PGPORT'],'5433');self.assertEqual(env['PGSSLMODE'],'require')
        with self.assertRaises(ValueError):PG.pg_env('https://evil.example.com/db')
        with self.assertRaises(ValueError):PG.pg_env('postgres://x:y@localhost/db?options=bad')
    def test_secret_redaction(self):
        with patch.object(I,'SECRETS',['very-secret','secret']):self.assertEqual(I.redact('very-secret / secret'),'[скрыто] / [скрыто]')
    def test_database_helper_preserves_immutable_release(self):
        shutil.copy2(DEPLOY/'pg-env.py',self.root/'pg-env.py')
        before=set(self.root.iterdir())
        with patch.object(I,'__file__',str(self.root/'installer.py')):
            self.assertEqual(I.db_env({'DATABASE_URL':'postgres://owner:secret@localhost/db'})['PGPASSWORD'],'secret')
        self.assertEqual(set(self.root.iterdir()),before)
    def release(self):
        files=[f'bin/{n}' for n in I.BINS]+['web/index.html','web/miniapp-unavailable.html','install.sh','deploy/installer.py','deploy/migrate.sh','deploy/pg-env.py','db/migrations/001_init.sql']
        files += [f'web/{n}-linux-{a}' for a in ('amd64','arm64') for n in ('sn-node','sn-sub','sn-cabinet')]
        for n in files:
            p=self.root/n;p.parent.mkdir(parents=True,exist_ok=True);p.write_text('fixture '+n)
        arch={'x86_64':'amd64','aarch64':'arm64'}.get(I.platform.machine())
        m={'layout':1,'version':'v0.1.1','arch':arch,'files':{n:I.sha256(self.root/n) for n in files}}
        (self.root/'RELEASE.json').write_text(json.dumps(m));return m
    def test_valid_manifest_and_tampered_file(self):
        self.release();I.validate_release(self.root)
        (self.root/'bin/sn-api').write_text('tampered')
        with self.assertRaises(I.InstallError):I.validate_release(self.root)
    def test_missing_companion_architecture(self):
        m=self.release();del m['files']['web/sn-node-linux-arm64'];(self.root/'RELEASE.json').write_text(json.dumps(m))
        with self.assertRaises(I.InstallError):I.validate_release(self.root)
    def test_rejects_traversal_and_extra_files(self):
        m=self.release();m['files']['../escape']='a'*64;(self.root/'RELEASE.json').write_text(json.dumps(m))
        with self.assertRaises(I.InstallError):I.validate_release(self.root)
        self.release();(self.root/'unexpected').write_text('no')
        with self.assertRaises(I.InstallError):I.validate_release(self.root)
    def test_rejects_symlinks(self):
        self.release();p=self.root/'bin/sn-api';p.unlink();p.symlink_to('/bin/sh')
        with self.assertRaises(I.InstallError):I.validate_release(self.root)
    def test_private_config_required(self):
        p=self.root/'install.json';p.write_text(json.dumps(self.c));p.chmod(0o644)
        with patch.object(I.os,'geteuid',return_value=0),self.assertRaises(I.InstallError):I.private_json(p)
    def test_caddy_separates_apps_and_local_services(self):
        config=I.caddy_config(self.c)
        self.assertIn('handle /app*',config);self.assertIn('/miniapp-unavailable.html',config)
        self.assertIn('reverse_proxy 127.0.0.1:8080',config);self.assertIn('reverse_proxy 127.0.0.1:8081',config)
        self.assertNotIn('auto_https off',config)
        self.assertIn('handle_path /custom/*',config)
        self.assertIn('/shared/public',config)
    def test_update_preserves_operator_state(self):
        root=self.root/'installed';root.mkdir();old=root/'releases/v0.1.0';old.mkdir(parents=True);(root/'current').symlink_to(old)
        c={'version':'v0.1.0','proxy':'external','brand':'Owner brand','currency':'EUR'}
        values={'DATABASE_URL':'postgres://x:y@localhost/db','CABINET_CODE_KEY':'unchanged-key','BOT_TOKEN':'unchanged-token'}
        I.atomic(root/'.env',I.env_text(values));I.atomic(root/'installation.json',json.dumps(c))
        public=root/'shared/public';public.mkdir(parents=True);(public/'logo.svg').write_text('owner logo')
        new=root/'releases/v0.1.1';new.mkdir();(old/'operator-note').write_text('keep previous release')
        args=type('Args',(),{'release_dir':new})()
        before=(root/'.env').read_bytes();seen=[]
        def checked_backup(*args):
            self.assertEqual((root/'current').resolve(),old.resolve());seen.append('backup')
        def checked_run(*args,**kwargs):
            self.assertEqual(seen,['backup']);self.assertIn('migrate.sh',str(args[0]));seen.append('migrate')
        with patch.object(I,'ROOT',root),patch.object(I,'private_json',return_value=c),patch.object(I,'stage_release',return_value=new),patch.object(I,'backup',side_effect=checked_backup),patch.object(I,'run',side_effect=checked_run),patch.object(I,'restart'),patch.object(I,'health'),patch.object(I,'seed') as seed,patch.object(I,'initial_env') as initial,patch.object(I,'configure_proxy') as proxy,patch.object(I,'ui'):
            I.update(args,{'version':'v0.1.1'})
            seed.assert_not_called();initial.assert_not_called();proxy.assert_not_called()
        self.assertEqual((root/'.env').read_bytes(),before)
        self.assertEqual((public/'logo.svg').read_text(),'owner logo')
        self.assertEqual((old/'operator-note').read_text(),'keep previous release')
        state=json.loads((root/'installation.json').read_text());self.assertEqual(state['brand'],'Owner brand');self.assertEqual(state['currency'],'EUR')
        self.assertEqual(state['version'],'v0.1.1');self.assertEqual((root/'current').resolve(),new.resolve())
    def test_public_storage_does_not_replace_owner_assets(self):
        with patch.object(I,'ROOT',self.root):
            I.public_storage();logo=self.root/'shared/public/logo.svg';logo.write_text('custom')
            I.public_storage();self.assertEqual(logo.read_text(),'custom')
    def test_same_tag_cannot_replace_binaries(self):
        m=self.release()
        destination=self.root/'installation';(destination/'releases/v0.1.1').mkdir(parents=True)
        (destination/'releases/v0.1.1/RELEASE.json').write_text(json.dumps({**m,'files':{}}))
        with patch.object(I,'ROOT',destination),self.assertRaises(I.InstallError):I.stage_release(self.root,m)
    def test_update_rolls_back_pointer_on_failed_readiness(self):
        root=self.root/'installed';root.mkdir();old=root/'releases/v0.1.0';old.mkdir(parents=True);(root/'current').symlink_to(old)
        (root/'installation.json').write_text('{}')
        new=root/'releases/v0.1.1';new.mkdir()
        c={'version':'v0.1.0','proxy':'external'};values={'DATABASE_URL':'postgres://x:y@localhost/db'}
        args=type('Args',(),{'release_dir':new})()
        with patch.object(I,'ROOT',root),patch.object(I,'private_json',return_value=c),patch.object(I,'read_env',return_value=values),patch.object(I,'stage_release',return_value=new),patch.object(I,'backup'),patch.object(I,'run'),patch.object(I,'restart') as restart,patch.object(I,'health',side_effect=I.InstallError('failed')),patch.object(I,'ui'):
            with self.assertRaises(I.InstallError):I.update(args,{'version':'v0.1.1'})
            self.assertEqual((root/'current').resolve(),old.resolve())
            self.assertEqual(restart.call_count,2)

if __name__=='__main__':unittest.main()

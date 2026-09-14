#!/usr/bin/env python3
"""STEALTHNET release installer. Python 3.10+, standard library only."""
import argparse
import fcntl
import getpass
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import re
import secrets
import shutil
import socket
import ssl
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

# Immutable release directories must not acquire Python bytecode on import.
sys.dont_write_bytecode = True

ROOT = Path('/opt/stealthnet-software')
CABINET_ENV = Path('/etc/sn-cabinet/env')
CABINET_BIN = Path('/usr/local/bin/sn-cabinet')
SERVICES = ('api', 'sub', 'worker', 'bot')
BINS = ('sn-api', 'sn-sub', 'sn-worker', 'sn-bot', 'sn-admin', 'sn-node', 'sn-cabinet')
DOMAIN = re.compile(r'(?=.{1,253}\Z)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}\Z')
TAG = re.compile(r'v\d+\.\d+\.\d+(?:-[a-zA-Z0-9][a-zA-Z0-9.-]*)?\Z')
SECRETS = []
LOG = None
COLOR = sys.stdout.isatty() and 'NO_COLOR' not in os.environ

class InstallError(Exception):
    pass

def ui(text, color='36'):
    print((f'\033[{color}m{text}\033[0m' if COLOR else text), flush=True)

def redact(text):
    for value in sorted(set(SECRETS), key=len, reverse=True):
        if value: text = text.replace(value, '[скрыто]')
    return text

def write_log(text):
    if LOG:
        LOG.write(redact(text) + '\n'); LOG.flush()

def run(args, *, env=None, data=None, check=True, timeout=1200, output=None):
    # Never log input/env: these can contain passwords and SQL credentials.
    write_log('> ' + ' '.join(str(a) for a in args))
    proc = subprocess.run([str(a) for a in args], input=data, text=output is None,
                          stdout=output or subprocess.PIPE, stderr=subprocess.PIPE,
                          env=env, timeout=timeout)
    stdout = proc.stdout or ''
    stderr = proc.stderr or ''
    if isinstance(stderr, bytes): stderr = stderr.decode(errors='replace')
    write_log(str(stdout) + stderr)
    if check and proc.returncode:
        raise InstallError(f'Шаг завершился с ошибкой: {Path(str(args[0])).name}. Подробности в журнале {LOG.name if LOG else "установки"}.')
    return stdout.strip() if isinstance(stdout, str) else stdout

def atomic(path, text, mode=0o600):
    path = Path(path); path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.'+path.name+'.', dir=path.parent)
    try:
        os.fchmod(fd, mode)
        with os.fdopen(fd, 'w') as stream:
            stream.write(text); stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary): os.unlink(temporary)

def sha256(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024*1024), b''): h.update(chunk)
    return h.hexdigest()

def validate_release(directory):
    directory = Path(directory)
    manifest = json.loads((directory/'RELEASE.json').read_text())
    arch = {'x86_64':'amd64','aarch64':'arm64'}.get(platform.machine())
    if not TAG.fullmatch(str(manifest.get('version', ''))) or manifest.get('layout') != 1 or manifest.get('arch') != arch:
        raise InstallError('Релиз не подходит для этого сервера.')
    files = manifest.get('files', {})
    required = [f'bin/{name}' for name in BINS] + ['web/index.html','web/app.css','web/core.js','web/miniapp-unavailable.html','install.sh','deploy/installer.py','deploy/migrate.sh','deploy/pg-env.py','db/migrations/001_init.sql']
    required += [f'web/{name}-linux-{a}' for a in ('amd64','arm64') for name in ('sn-node','sn-sub','sn-cabinet')]
    if not isinstance(files, dict) or not all(name in files for name in required):
        raise InstallError('Релиз неполный: отсутствуют обязательные файлы.')
    for name, expected in files.items():
        p = Path(name)
        if p.is_absolute() or '..' in p.parts or not re.fullmatch('[a-f0-9]{64}',str(expected)):
            raise InstallError('Некорректный манифест релиза.')
        target = directory/p
        if target.is_symlink() or not target.is_file() or sha256(target) != expected:
            raise InstallError('Повреждён файл релиза: ' + name)
    for p in directory.rglob('*'):
        if p.is_symlink() or (p.is_file() and p.name != 'RELEASE.json' and str(p.relative_to(directory)) not in files):
            raise InstallError('В релизе найден незаявленный файл.')
    return manifest

def private_json(path):
    path = Path(path)
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o077:
        raise InstallError(f'{path}: файл с паролем должен принадлежать root и иметь права 600.')
    value = json.loads(path.read_text())
    if not isinstance(value, dict): raise InstallError('Ожидается объект JSON.')
    return value

def domain_name(value):
    value = re.sub(r'^https?://', '', value.strip().lower()).removesuffix('/')
    if not DOMAIN.fullmatch(value):
        raise InstallError('Нужен домен, например panel.example.com: без порта, /s/ID, других путей и параметров.')
    return value

def project_currency(value):
    value = value.strip().upper()
    if not re.fullmatch('[A-Z]{3}', value) or value == 'XTR':
        raise InstallError('Введите трёхбуквенный код валюты: USD, EUR, RUB или UAH. Не знак валюты и не сумму. Stars включаются отдельно в панели.')
    return value

def local_subscription(c):
    # Configurations written before this choice existed always installed sn-sub.
    placement = c.get('subscription_placement', 'local')
    if placement not in ('local', 'remote'):
        raise InstallError('subscription_placement: local (сервер панели) или remote (отдельный сервер).')
    return placement == 'local'

def managed_services(c):
    return tuple(s for s in SERVICES if s != 'sub' or local_subscription(c))

def validate_config(c):
    allowed = {'panel_domain','sub_domain','brand','currency','admin_user','admin_password','bot_token','proxy','subscription_placement'}
    if set(c)-allowed: raise InstallError('Неизвестные параметры: '+', '.join(sorted(set(c)-allowed)))
    c = dict(c)
    c.setdefault('proxy','caddy'); c.setdefault('bot_token',''); c.setdefault('admin_user','admin')
    c.setdefault('subscription_placement', 'local')
    for key in allowed:
        if key not in c or not isinstance(c[key],str): raise InstallError('Не задан параметр: '+key)
        if any(ord(ch)<32 or ord(ch)==127 for ch in c[key]): raise InstallError('Недопустимые символы: '+key)
    for key in ('panel_domain','sub_domain'):
        c[key] = domain_name(c[key])
    local_subscription(c)
    if c['panel_domain']==c['sub_domain']: raise InstallError('Для панели и подписок нужны разные домены.')
    if not 1<=len(c['brand'])<=80: raise InstallError('Название сервиса: от 1 до 80 символов.')
    if not re.fullmatch('[a-zA-Z0-9_.@-]{3,64}',c['admin_user']): raise InstallError('Логин: 3–64 латинских символа, цифры, . _ @ -')
    if not 12<=len(c['admin_password'])<=128: raise InstallError('Пароль владельца: 12–128 символов.')
    c['currency'] = project_currency(c['currency'])
    if c['bot_token'] and not re.fullmatch(r'[0-9]{5,15}:[A-Za-z0-9_-]{20,100}',c['bot_token']): raise InstallError('Некорректный токен Telegram-бота.')
    if c['proxy'] not in ('caddy','external'): raise InstallError('proxy: caddy или external')
    return c

def terminal():
    # Buffered r+ requires seeking, which a real SSH terminal cannot do.
    # A raw bidirectional descriptor supports prompts and input without seeking.
    try:
        return io.TextIOWrapper(open('/dev/tty', 'r+b', buffering=0),
                                encoding='utf-8', line_buffering=True)
    except OSError as exc:
        raise InstallError('Не удалось открыть терминал для вопросов. Запустите установщик в интерактивном SSH-сеансе (ssh -t) или передайте --config /root/install.json (права 600).') from exc

def wizard():
    with terminal() as tty:
        def ask(label, default='', *, hint='', validate=None):
            if hint: tty.write('  '+hint+'\n')
            while True:
                tty.write(f'  {label}'+(f' [{default}]' if default else '')+': '); tty.flush()
                line = tty.readline()
                if not line: raise InstallError('Ввод прерван.')
                value = line.strip() or default
                try: return validate(value) if validate else value
                except InstallError as error:
                    tty.write('  '+str(error)+' Попробуйте ещё раз.\n')
        def placement(value):
            if value not in ('1', '2'): raise InstallError('Выберите 1 или 2.')
            return {'1':'local', '2':'remote'}[value]
        ui('\n  STEALTHNET  /  Новый сервер', '1;36')
        mode = ask('Где разместить подписку', '1', validate=placement,
                   hint='1 — на этом сервере вместе с панелью; 2 — на отдельном сервере.\n'
                        '  Подписка — страница подключения и ссылка, которую клиент добавляет в VPN-приложение.')
        panel = ask('Домен панели', validate=domain_name,
                    hint='Например panel.example.com. Это адрес входа администратора.\n'
                         '  DNS A/AAAA направьте на IP ЭТОГО сервера. Можно вставить https://panel.example.com.')
        def subscription_domain(value):
            value = domain_name(value)
            if value == panel: raise InstallError('У подписки должен быть свой домен, например sub.example.com.')
            return value
        sub_hint = ('DNS A/AAAA направьте на IP ЭТОГО сервера.' if mode == 'local' else
                    'DNS A/AAAA должны вести на IP ОТДЕЛЬНОГО сервера подписки.\n'
                    '  Сейчас достаточно выбрать будущий домен: его DNS и HTTPS настроите на втором сервере позже.')
        sub = ask('Домен подписки', validate=subscription_domain,
                  hint='Например sub.example.com. Нужен домен, а не ссылка клиента /s/ID.\n  '+sub_hint)
        c = {'subscription_placement':mode, 'panel_domain':panel, 'sub_domain':sub,
             'brand':ask('Название вашего сервиса', hint='Например My VPN — это название увидят клиенты.'),
             'currency':ask('Валюта проекта','USD', validate=project_currency,
                            hint='Код валюты для цен и баланса: USD — доллар, EUR — евро, RUB — рубль, UAH — гривна.\n'
                                 '  Введите код из трёх латинских букв, без суммы. Telegram Stars настраиваются отдельно.'),
             'admin_user':ask('Логин владельца','admin', hint='Первый аккаунт с полными правами. Например admin; это не Telegram-имя.')}
        c['admin_password'] = getpass.getpass('  Пароль владельца (Enter — создать надёжный): ',stream=tty)
        if c['admin_password']:
            again = getpass.getpass('  Повторите пароль: ',stream=tty)
            if again != c['admin_password']: raise InstallError('Пароли не совпадают. Запустите установщик повторно.')
        else: c['admin_password'] = secrets.token_urlsafe(21)
        c['bot_token'] = getpass.getpass('  Токен Telegram-бота (Enter — настроить позже): ',stream=tty)
        return validate_config(c)

def env_text(values):
    # systemd EnvironmentFile, not shell. Escape backslashes and double quotes.
    return ''.join(k+'="'+str(v).replace('\\','\\\\').replace('"','\\"')+'"\n' for k,v in values.items())

def read_env(path):
    values = {}
    for line in Path(path).read_text().splitlines():
        if not line or line.startswith('#'): continue
        key, sep, value = line.partition('=')
        if not sep or not re.fullmatch('[A-Z][A-Z0-9_]*',key): raise InstallError('Некорректный .env')
        if value.startswith('"') and value.endswith('"'):
            value = re.sub(r'\\([\\"])',r'\1',value[1:-1])
        values[key] = value
    return values

def db_env(values):
    spec = importlib.util.spec_from_file_location('pg_environment', Path(__file__).with_name('pg-env.py'))
    mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
    env = os.environ.copy(); env.update(mod.pg_env(values['DATABASE_URL'])); env.update(values)
    return env

def sql(statement, values=None):
    cmd = ['psql','-X','-q','-t','-A','-v','ON_ERROR_STOP=1']
    if values is None: cmd = ['runuser','-u','postgres','--']+cmd
    return run(cmd, env=db_env(values) if values else None, data=statement)

def os_check():
    if os.geteuid()!=0: raise InstallError('Запустите от root.')
    if not Path('/run/systemd/system').is_dir(): raise InstallError('Нужен systemd.')
    os_release={}
    for line in Path('/etc/os-release').read_text().splitlines():
        key,_,value=line.partition('='); os_release[key]=value.strip('"')
    supported={'ubuntu':('22.04','24.04','26.04'),'debian':('12','13')}
    if os_release.get('VERSION_ID') not in supported.get(os_release.get('ID'),()):
        raise InstallError('Поддерживаются Debian 12/13, Ubuntu 22.04/24.04/26.04 LTS.')
    if shutil.disk_usage('/opt').free < 3*1024**3: raise InstallError('Нужно не менее 3 ГБ свободного места в /opt.')
    memory = int(re.search(r'MemTotal:\s+(\d+)',Path('/proc/meminfo').read_text()).group(1))
    if memory < 900*1024: raise InstallError('Нужно хотя бы 1 ГБ памяти; рекомендуется 2 ГБ.')
    return os_release

def port_free(port):
    try:
        with socket.socket() as s: s.bind(('0.0.0.0',port))
        return True
    except OSError: return False

def preflight(c, resume=False):
    if not resume:
        for port in ((8080,8081) if local_subscription(c) else (8080,)):
            if not port_free(port): raise InstallError(f'Порт {port} занят. Освободите его или выберите другой сервер для панели.')
        for svc in SERVICES:
            if Path(f'/etc/systemd/system/sn-{svc}.service').exists(): raise InstallError('Найдена существующая установка. Используйте её update.sh, не устанавливайте поверх.')
    if c['proxy']=='caddy':
        for port in (80,443):
            if not port_free(port) and subprocess.run(['systemctl','is-active','--quiet','caddy']).returncode:
                raise InstallError(f'Порт {port} занят другим веб-сервером. Используйте документированный режим external или отдельный сервер.')
        for key in (('panel_domain','sub_domain') if local_subscription(c) else ('panel_domain',)):
            try: addresses=sorted({v[4][0] for v in socket.getaddrinfo(c[key],None)})
            except OSError: raise InstallError(f'Нет DNS-записи для {c[key]}. Настройте A/AAAA и повторите запуск.')
            ui('  DNS '+c[key]+' → '+', '.join(addresses), '90')

def dependencies(c):
    env=os.environ.copy(); env.update(DEBIAN_FRONTEND='noninteractive',NEEDRESTART_MODE='l')
    run(['apt-get','-o','DPkg::Lock::Timeout=180','update','-qq'],env=env)
    packages=['ca-certificates','curl','python3','make','postgresql','postgresql-contrib']
    install_caddy = c['proxy']=='caddy' and not shutil.which('caddy')
    if install_caddy: packages += ['gnupg']
    run(['apt-get','-o','DPkg::Lock::Timeout=180','install','-y','--no-install-recommends']+packages,env=env)
    if install_caddy:
        # Ubuntu 22.04 does not ship Caddy in its standard package indexes.
        # Use the signed stable repository documented by the Caddy project.
        with tempfile.TemporaryDirectory(prefix='sn-caddy-') as temporary:
            key=Path(temporary)/'caddy.asc'; ring=Path(temporary)/'caddy.gpg'
            run(['curl','--proto','=https','--tlsv1.2','-fsSL','--max-time','45',
                 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key','-o',str(key)])
            run(['gpg','--batch','--dearmor','--output',str(ring),str(key)])
            destination=Path('/usr/share/keyrings/caddy-stable-archive-keyring.gpg')
            shutil.copy2(ring,destination); destination.chmod(0o644)
        atomic('/etc/apt/sources.list.d/caddy-stable.list',
               'deb [signed-by=/usr/share/keyrings/caddy-stable-archive-keyring.gpg] https://dl.cloudsmith.io/public/caddy/stable/deb/debian any-version main\n',0o644)
        run(['apt-get','-o','DPkg::Lock::Timeout=180','update','-qq'],env=env)
        run(['apt-get','-o','DPkg::Lock::Timeout=180','install','-y','--no-install-recommends','caddy'],env=env)
    run(['systemctl','enable','--now','postgresql'])
    if subprocess.run(['id','-u','stealthnet'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode:
        run(['useradd','--system','--user-group','--home-dir','/var/lib/stealthnet','--create-home','--shell','/usr/sbin/nologin','stealthnet'])
    Path('/var/lib/stealthnet').mkdir(exist_ok=True)
    run(['chown','stealthnet:stealthnet','/var/lib/stealthnet'])

def initial_env(c):
    password=secrets.token_hex(24)
    return {'DATABASE_URL':f'postgres://stealthnet:{password}@127.0.0.1:5432/stealthnet',
            'API_BIND':'127.0.0.1:8080','SUB_BIND':'127.0.0.1:8081',
            'SUB_PUBLIC_URL':'https://'+c['sub_domain'],'PANEL_URL':'https://'+c['panel_domain'],
            'BRAND_NAME':c['brand'],'WEB_ROOT':str(ROOT/'current/web'),'SUB_MODE':'db',
            'SUB_SERVICE_TOKEN':secrets.token_hex(24),'CABINET_CODE_KEY':secrets.token_hex(32),
            'BOT_TOKEN':c['bot_token'],'RUST_LOG':'info'}

def create_database(values, resume):
    role=sql("SELECT 1 FROM pg_roles WHERE rolname='stealthnet';")=='1'
    database=sql("SELECT 1 FROM pg_database WHERE datname='stealthnet';")=='1'
    if (role or database) and not resume: raise InstallError('База или роль stealthnet уже существует. Автоматически менять её пароль нельзя.')
    if not role:
        password=db_env(values)['PGPASSWORD']
        sql("CREATE ROLE stealthnet LOGIN PASSWORD '"+password+"';")
    if not database: run(['runuser','-u','postgres','--','createdb','--owner=stealthnet','stealthnet'])
    if sql('SELECT 1;',values)!='1': raise InstallError('Не удалось подключиться к базе данных.')

def stage_release(source, manifest):
    dest=ROOT/'releases'/manifest['version']
    if dest.exists():
        old=json.loads((dest/'RELEASE.json').read_text())
        if old != manifest: raise InstallError('Под этим тегом уже установлена другая сборка. Выпустите новый тег версии.')
        validate_release(dest)
        return dest
    temporary=ROOT/'releases'/('.'+manifest['version']+'.staging')
    if temporary.exists(): shutil.rmtree(temporary)
    shutil.copytree(source,temporary)
    for p in temporary.rglob('*'):
        p.chmod(0o755 if p.is_dir() or p.parent.name=='bin' or p.suffix=='.sh' else 0o644)
    temporary.chmod(0o755)
    os.replace(temporary,dest)
    return dest

def switch(dest):
    temp=ROOT/'.current-next'
    temp.unlink(missing_ok=True); temp.symlink_to(dest)
    os.replace(temp,ROOT/'current')
    for name in ('Makefile','update.sh'):
        link=ROOT/name
        if not link.exists() and not link.is_symlink(): link.symlink_to('current/'+name)

def public_storage():
    # Operator assets live outside immutable releases and are never seeded over.
    for path in (ROOT/'shared', ROOT/'shared/public'):
        path.mkdir(exist_ok=True, mode=0o755)
        path.chmod(0o755)

def seed(c, values):
    settings={'brand.name':c['brand'],'billing.currency':c['currency'],
              'subscription.public_url':values['SUB_PUBLIC_URL'],'panel.public_url':values['PANEL_URL']}
    def lit(v): return "'"+v.replace("'","''")+"'"
    statement='\n'.join('INSERT INTO settings(key,value) VALUES ('+lit(k)+','+lit(json.dumps(v,ensure_ascii=False))+'::jsonb) ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value;' for k,v in settings.items())
    sql(statement,values)

def units(c):
    services = managed_services(c)
    for svc in services:
        atomic(f'/etc/systemd/system/sn-{svc}.service',f'''[Unit]
Description=STEALTHNET — {svc}
After=network-online.target postgresql.service
Wants=network-online.target
Requires=postgresql.service
StartLimitIntervalSec=120
StartLimitBurst=10

[Service]
Type=simple
User=stealthnet
Group=stealthnet
EnvironmentFile={ROOT}/.env
WorkingDirectory=/var/lib/stealthnet
ExecStart={ROOT}/current/bin/sn-{svc}
Restart=on-failure
RestartSec=5
TimeoutStopSec=30
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/stealthnet
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
CapabilityBoundingSet=

[Install]
WantedBy=multi-user.target
''',0o644)
    atomic('/usr/local/bin/stealthnet',f'''#!/usr/bin/env bash
set -euo pipefail
case "${{1:-help}}" in
  status) systemctl --no-pager --full status {' '.join('sn-'+s for s in services)};;
  logs) case "${{2:-api}}" in {'|'.join(services)}) exec journalctl -u "sn-${{2:-api}}" -n 100 -f;; *) echo '{' | '.join(services)}'; exit 2;; esac;;
  update) shift; exec bash {ROOT}/current/install.sh --update "$@";;
  doctor) exec python3 {ROOT}/current/deploy/installer.py doctor;;
  admin-password) exec python3 {ROOT}/current/deploy/installer.py admin-password;;
  bot-token) exec python3 {ROOT}/current/deploy/installer.py bot-token;;
  *) printf 'STEALTHNET\\n  stealthnet status\\n  stealthnet doctor\\n  stealthnet logs [api|sub|worker|bot]\\n  stealthnet update [--version vX.Y.Z]\\n  stealthnet admin-password\\n  stealthnet bot-token\\n';;
esac
''',0o755)
    run(['systemctl','daemon-reload'])

def caddy_config(c):
    config = f'''# Managed by STEALTHNET; other Caddy sites are left intact.
{c['panel_domain']} {{
    encode zstd gzip
    handle /api/* {{
        reverse_proxy 127.0.0.1:8080
    }}
    handle_path /custom/* {{
        root * {ROOT}/shared/public
        file_server
    }}
    @sn_miniapp path /app /app/*
    handle @sn_miniapp {{
        root * {ROOT}/current/web
        rewrite * /miniapp-unavailable.html
        file_server
    }}
    handle {{
        root * {ROOT}/current/web
        try_files {{path}} {{path}}/index.html /index.html
        file_server
        header Cache-Control "no-cache"
    }}
    header {{
        -Server
        Strict-Transport-Security "max-age=31536000"
        X-Content-Type-Options nosniff
        Referrer-Policy no-referrer
        X-Frame-Options DENY
        Content-Security-Policy "frame-ancestors 'none'; base-uri 'self'; object-src 'none'"
    }}
}}
'''
    if local_subscription(c):
        config += f'''
{c['sub_domain']} {{
    encode zstd gzip
    reverse_proxy 127.0.0.1:8081
    header {{
        -Server
        Strict-Transport-Security "max-age=31536000"
        X-Content-Type-Options nosniff
        Referrer-Policy no-referrer
    }}
}}
'''
    return config

def configure_proxy(c):
    config=caddy_config(c)
    atomic(ROOT/'Caddyfile.example',config,0o644)
    if c['proxy']=='external':
        ui('  Прокси настраивается отдельно: '+str(ROOT/'Caddyfile.example'),'33'); return
    main=Path('/etc/caddy/Caddyfile'); snippet=Path('/etc/caddy/stealthnet/panel.caddy')
    before=main.read_text() if main.exists() else ''
    old_snippet=snippet.read_text() if snippet.exists() else None
    import_line='import /etc/caddy/stealthnet/*.caddy'
    # Preserve unrelated sites and the packaged welcome page.
    candidate=before if import_line in before.splitlines() else before+'\n'+import_line+'\n'
    if not (ROOT/'Caddyfile.before-install').exists(): atomic(ROOT/'Caddyfile.before-install',before)
    snippet.parent.mkdir(parents=True,exist_ok=True); snippet.parent.chmod(0o755)
    atomic(snippet,config,0o644); atomic(main,candidate,0o644)
    try:
        run(['caddy','validate','--config',str(main),'--adapter','caddyfile'])
        if shutil.which('ufw') and 'Status: active' in run(['ufw','status'],check=False):
            run(['ufw','allow','80/tcp']); run(['ufw','allow','443/tcp'])
        run(['systemctl','enable','--now','caddy'])
        run(['systemctl','reload','caddy'])
    except Exception:
        atomic(main,before,0o644)
        if old_snippet is None: snippet.unlink(missing_ok=True)
        else: atomic(snippet,old_snippet,0o644)
        raise

def repair_proxy(c, *, main=Path('/etc/caddy/Caddyfile'), snippet=Path('/etc/caddy/stealthnet/panel.caddy')):
    # Upgrade only the installer-owned legacy matcher. Retain operator edits,
    # unrelated sites and external proxies; update() already backs up Caddy.
    if c['proxy']!='caddy' or not snippet.is_file(): return False
    before=snippet.read_text()
    if not before.startswith('# Managed by STEALTHNET; other Caddy sites are left intact.\n'): return False
    legacy=re.compile(r'(?m)^([ \t]*)handle /app\* \{[ \t]*$')
    if not legacy.search(before): return False
    candidate=legacy.sub(lambda m: m[1]+'@sn_miniapp path /app /app/*\n'+m[1]+'handle @sn_miniapp {',before)
    mode=stat.S_IMODE(snippet.stat().st_mode)
    atomic(snippet,candidate,mode)
    try:
        run(['caddy','validate','--config',str(main),'--adapter','caddyfile'])
        run(['systemctl','reload','caddy'])
    except Exception:
        atomic(snippet,before,mode)
        run(['systemctl','reload','caddy'],check=False)
        raise
    ui('  ✓ Исправлена выдача стилей панели; настройки Caddy сохранены.','32')
    return True

def health_error(error):
    # URL errors wrap the useful socket/TLS error. Do not print arbitrary
    # server responses or exception text: either can contain credentials.
    if isinstance(error, urllib.error.HTTPError):
        error.close()
        return f'сервер вернул HTTP {error.code}; проверьте маршрут reverse proxy и журнал службы'
    reason=error.reason if isinstance(error, urllib.error.URLError) else error
    if isinstance(reason, socket.gaierror):
        return 'домен не разрешается в IP; проверьте DNS-записи A/AAAA и DNS на сервере'
    if isinstance(reason, ConnectionRefusedError):
        return 'соединение отклонено; проверьте IP в DNS, запущен ли веб-сервер и открыт ли порт'
    if isinstance(reason, TimeoutError):
        return 'истекло время ожидания; проверьте доступность адреса и firewall сервера и провайдера'
    if isinstance(reason, ssl.SSLCertVerificationError):
        return 'сертификат HTTPS не прошёл проверку; проверьте домен, срок сертификата и время на сервере'
    if isinstance(reason, ssl.SSLError):
        return 'не удалось установить TLS-соединение; проверьте HTTPS и журнал веб-сервера'
    if isinstance(reason, ConnectionResetError):
        return 'сервер сбросил соединение; проверьте reverse proxy и его журнал'
    if isinstance(reason, OSError):
        return 'сетевая ошибка; проверьте адрес, маршрутизацию и firewall'
    if isinstance(error, (json.JSONDecodeError, UnicodeDecodeError)):
        return 'получен ответ вместо JSON состояния; проверьте маршрут reverse proxy'
    return 'не удалось прочитать ответ проверки состояния'

def json_request(url, expected, *, attempts=30):
    last=''
    for _ in range(attempts):
        try:
            with urllib.request.urlopen(url,timeout=3) as response:
                obj=json.loads(response.read(65536))
            if isinstance(obj, dict) and all(obj.get(k)==v for k,v in expected.items()): return
            last='служба вернула неожиданный статус; проверьте её журнал и подключение к базе'
        except Exception as error: last=health_error(error)
        if _+1<attempts: time.sleep(1)
    raise InstallError(f'Не прошла проверка {url}: {last}.')

def web_assets(base_url, directory):
    for name, types in (('app.css',('text/css',)),('core.js',('text/javascript','application/javascript'))):
        expected=(directory/name).read_bytes()
        url=base_url+'/'+name+'?sn_check='+hashlib.sha256(expected).hexdigest()[:16]
        try:
            with urllib.request.urlopen(url,timeout=10) as response:
                content_type=response.headers.get_content_type()
                body=response.read(len(expected)+1)
        except Exception as error:
            raise InstallError(f'Не прошла проверка {url}: {health_error(error)}.') from None
        if content_type not in types or body!=expected:
            raise InstallError(f'Панель неверно отдаёт {name}. Проверьте маршруты Caddy: '
                'правило Mini App должно включать только /app и /app/*; /app.css должен отдаваться как text/css. '
                'Также проверьте каталог статики и кэш reverse proxy.')

def health(c, values, public=True):
    for svc in managed_services(c):
        if svc=='bot' and not values.get('BOT_TOKEN'): continue
        run(['systemctl','is-active','--quiet','sn-'+svc])
    json_request('http://127.0.0.1:8080/api/health',{'status':'ok','db':True})
    if local_subscription(c): json_request('http://127.0.0.1:8081/ready',{'status':'ready'})
    if public and c['proxy']=='caddy':
        ui('  → Проверяем HTTPS и сертификаты (первый выпуск может занять минуту)…')
        try:
            json_request('https://'+c['panel_domain']+'/api/health',{'status':'ok','db':True},attempts=45)
            if local_subscription(c): json_request('https://'+c['sub_domain']+'/ready',{'status':'ready'},attempts=45)
            url='https://'+c['panel_domain']+'/'
            try:
                with urllib.request.urlopen(url,timeout=10) as response:
                    html=response.read(256)
            except Exception as error:
                raise InstallError(f'Не прошла проверка {url}: {health_error(error)}.') from None
            if b'<!doctype html' not in html.lower(): raise InstallError('Панель не отдаёт HTML.')
            web_assets('https://'+c['panel_domain'],ROOT/'current/web')
        except InstallError as error:
            raise InstallError(str(error)+'\n'
                'Локальные службы, API и база прошли проверку. Ошибка относится к публичному адресу.\n'+
                ('Сверьте A/AAAA доменов панели и подписки с IP этого сервера. ' if local_subscription(c) else
                 'Сверьте A/AAAA домена панели с IP этого сервера. Домен подписки относится к отдельному серверу. ')+
                'Для стандартной установки нужны TCP 80 и 443, '
                'включая firewall в кабинете хостинга.\n'
                'Проверка Caddy: systemctl is-active caddy\n'
                'Журнал Caddy: journalctl -u caddy -n 50 --no-pager') from None

def restart(values, c):
    enabled=['sn-'+s for s in managed_services(c) if s!='bot' or values.get('BOT_TOKEN')]
    run(['systemctl','enable']+enabled)
    run(['systemctl','restart']+enabled)

def backup(values, c):
    dest=ROOT/'backups'/time.strftime('%Y%m%dT%H%M%SZ',time.gmtime())
    dest.mkdir(parents=True,mode=0o700,exist_ok=False)
    for name in ('.env','installation.json'):
        shutil.copy2(ROOT/name,dest/name)
    atomic(dest/'previous-release.txt',str((ROOT/'current').resolve())+'\n')
    if Path('/etc/caddy/Caddyfile').exists(): shutil.copy2('/etc/caddy/Caddyfile',dest/'Caddyfile')
    if Path('/etc/caddy').exists(): shutil.copytree('/etc/caddy',dest/'caddy',symlinks=True)
    if (ROOT/'shared').exists(): shutil.copytree(ROOT/'shared',dest/'shared',symlinks=True)
    with (dest/'database.dump').open('wb') as out:
        os.chmod(out.name,0o600)
        run(['pg_dump','-Fc'],env=db_env(values),output=out)
    run(['pg_restore','--list',str(dest/'database.dump')])
    ui('  Резервная копия: '+str(dest),'90')
    return dest

def step(number, title, fn):
    ui(f'\n  [{number}] {title}', '1;36')
    start=time.monotonic(); result=fn()
    ui(f'  ✓ Готово · {int(time.monotonic()-start)} с','32')
    return result

def install(args, manifest):
    pending=ROOT/'.install-pending.json'
    resume=pending.exists()
    if (ROOT/'installation.json').exists():
        raise InstallError('Панель уже установлена. Для обновления: stealthnet update')
    if (ROOT/'.env').exists() and not resume:
        raise InstallError('Обнаружена существующая .env. Старая установка не будет перезаписана; используйте её update.sh.')
    c=validate_config(private_json(pending)) if resume else (validate_config(private_json(args.config)) if args.config else wizard())
    SECRETS.extend([c['admin_password'],c['bot_token']])
    step('1/7','Проверяем сервер и домены',lambda:preflight(c,resume))
    ROOT.mkdir(parents=True,exist_ok=True); ROOT.chmod(0o755)
    (ROOT/'releases').mkdir(exist_ok=True,mode=0o755); (ROOT/'releases').chmod(0o755)
    if not resume:
        atomic(pending,json.dumps(c,ensure_ascii=False))
        values=initial_env(c); atomic(ROOT/'.env',env_text(values))
    else: values=read_env(ROOT/'.env')
    SECRETS.extend([values['DATABASE_URL'],db_env(values)['PGPASSWORD'],values['SUB_SERVICE_TOKEN'],values['CABINET_CODE_KEY']])
    atomic('/root/stealthnet-access.txt',f"Панель: https://{c['panel_domain']}\nЛогин: {c['admin_user']}\nПароль: {c['admin_password']}\n\nСохраните данные в менеджере паролей и удалите этот файл.\n")
    step('2/7','Устанавливаем PostgreSQL и системные пакеты',lambda:dependencies(c))
    def prepare():
        create_database(values,resume)
        dest=stage_release(args.release_dir,manifest)
        run(['bash',dest/'deploy/migrate.sh'],env=db_env(values)); seed(c,values)
        run([dest/'bin/sn-admin','--password-stdin',c['admin_user'],'owner'],data=c['admin_password']+'\n',env=db_env(values))
        return dest
    dest=step('3/7','Готовим базу, настройки и аккаунт владельца',prepare)
    step('4/7','Подключаем готовый релиз',lambda:(public_storage(),switch(dest)))
    step('5/7','Настраиваем и запускаем службы',lambda:(units(c),restart(values,c)))
    step('6/7','Настраиваем HTTPS',lambda:configure_proxy(c))
    step('7/7','Проверяем работоспособность',lambda:health(c,values))
    safe={k:v for k,v in c.items() if k not in ('admin_password','bot_token')}
    safe.update(version=manifest['version'],layout=1,installed_at=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()))
    atomic(ROOT/'installation.json',json.dumps(safe,ensure_ascii=False,indent=2)+'\n'); pending.unlink()
    ui('\n  ✓ STEALTHNET установлен · '+manifest['version'],'1;32')
    ui('  Панель: https://'+c['panel_domain'])
    if local_subscription(c):
        ui('  Подписки: https://'+c['sub_domain'])
    else:
        ui('  Подписка на отдельном сервере ещё не установлена: https://'+c['sub_domain'],'33')
        ui('  В панели: Настройки → Сервис подписки → выпустите ключ → Установка → На отдельном сервере.')
        ui('  Выполните команду на втором сервере, затем настройте там DNS и HTTPS.')
        ui('  Инструкция: https://'+c['panel_domain']+'/subscription-installation.html')
    ui('  Доступ владельца: /root/stealthnet-access.txt (только root)')
    if c['proxy']=='external': ui('  Внешний HTTPS ещё нужно настроить по Caddyfile.example. Проверены локальные службы.','33')
    if not values.get('BOT_TOKEN'): ui('  Telegram: добавьте токен командой stealthnet bot-token.','90')
    ui('  Дальше в админке: профиль → нода → хост → сквад → тариф.')
    ui('  Кабинет и Mini App: Настройки → Клиентский кабинет → Установить.')
    ui('  Проверка: stealthnet doctor   Обновление: stealthnet update\n')

def local_cabinet(values):
    """Only manage the standard cabinet installed here for this exact panel."""
    if not CABINET_ENV.is_file() or not CABINET_BIN.is_file(): return None
    cabinet_values=read_env(CABINET_ENV)
    if cabinet_values.get('PANEL_API_URL','').rstrip('/')!=values.get('PANEL_URL','').rstrip('/') or not values.get('PANEL_URL'):
        return None
    if CABINET_BIN.is_symlink():
        raise InstallError('Нестандартный путь кабинета: проверьте /usr/local/bin/sn-cabinet перед обновлением.')
    if run(['systemctl','show','sn-cabinet','--property=LoadState','--value'],check=False)!='loaded': return None
    command=run(['systemctl','show','sn-cabinet','--property=ExecStart','--value'],check=False)
    if 'path='+str(CABINET_BIN)+' ' not in command:
        raise InstallError('Служба sn-cabinet использует нестандартный бинарник. Обновите её отдельно.')
    active=run(['systemctl','is-active','sn-cabinet'],check=False)=='active'
    enabled=run(['systemctl','is-enabled','sn-cabinet'],check=False) in ('enabled','enabled-runtime')
    bind=cabinet_values.get('CABINET_BIND','127.0.0.1:8090')
    url=urllib.parse.urlsplit('http://'+bind)
    try: port=url.port
    except ValueError: port=None
    if not port or not url.hostname or url.username or url.password or url.path or url.query or url.fragment:
        raise InstallError('Проверьте CABINET_BIND в /etc/sn-cabinet/env.')
    host={'0.0.0.0':'127.0.0.1','::':'::1'}.get(url.hostname,url.hostname)
    host='['+host+']' if ':' in host else host
    return {'restart':active or enabled,'ready':f'http://{host}:{port}/ready','changed':False,'backup':None}

def replace_binary(source, target):
    fd,temporary=tempfile.mkstemp(prefix='.'+target.name+'.',dir=target.parent)
    try:
        with os.fdopen(fd,'wb') as out, Path(source).open('rb') as inp:
            shutil.copyfileobj(inp,out);out.flush();os.fsync(out.fileno());os.fchmod(out.fileno(),0o755)
        os.replace(temporary,target)
    finally:
        if os.path.exists(temporary): os.unlink(temporary)

def cabinet_ready(url, *, attempts=30):
    last='служба не подтвердила готовность'
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(url,timeout=3) as response:
                if response.read(64).strip()==b'ready': return
        except Exception as error: last=health_error(error)
        if attempt+1<attempts: time.sleep(1)
    raise InstallError(f'Не прошла проверка кабинета {url}: {last}.')

def update_local_cabinet(cabinet, directory, backup_dir):
    if not cabinet: return
    source=directory/'bin/sn-cabinet'
    if sha256(source)==sha256(CABINET_BIN):
        if cabinet['restart']: cabinet_ready(cabinet['ready'],attempts=15)
        return
    saved=backup_dir/'cabinet';saved.mkdir(mode=0o700)
    cabinet['backup']=saved/'sn-cabinet';shutil.copy2(CABINET_BIN,cabinet['backup'])
    shutil.copy2(CABINET_ENV,saved/'env');(saved/'env').chmod(0o600)
    replace_binary(source,CABINET_BIN);cabinet['changed']=True
    if cabinet['restart']:
        run(['systemctl','restart','sn-cabinet'])
        cabinet_ready(cabinet['ready'],attempts=30)
    ui('  ✓ Кабинет на сервере панели обновлён. Ключ и настройки сохранены.','32')

def restore_local_cabinet(cabinet):
    if not cabinet or not cabinet['changed']: return
    replace_binary(cabinet['backup'],CABINET_BIN)
    if cabinet['restart']: run(['systemctl','restart','sn-cabinet'])
    cabinet['changed']=False

def update(args, manifest):
    if not (ROOT/'installation.json').exists(): raise InstallError('Готовая установка не найдена. Для исходников используйте существующий update.sh.')
    c=private_json(ROOT/'installation.json'); values=read_env(ROOT/'.env')
    SECRETS.extend(v for k,v in values.items() if any(s in k for s in ('TOKEN','KEY','DATABASE')))
    SECRETS.append(db_env(values)['PGPASSWORD'])
    old=(ROOT/'current').resolve()
    cabinet=local_cabinet(values)
    if c['version']==manifest['version']:
        validate_release(old); health(c,values)
        if cabinet and sha256(old/'bin/sn-cabinet')!=sha256(CABINET_BIN):
            saved=backup(values,c)
            try: update_local_cabinet(cabinet,old,saved)
            except Exception:
                restore_local_cabinet(cabinet)
                raise
        elif cabinet and cabinet['restart']: cabinet_ready(cabinet['ready'],attempts=15)
        ui('  ✓ Эта версия уже установлена; службы проверены.','32'); return
    def version_tuple(v): return tuple(map(int,re.match(r'v(\d+)\.(\d+)\.(\d+)',v).groups()))
    if version_tuple(manifest['version']) < version_tuple(c['version']): raise InstallError('Понижение версии с миграциями запрещено. Используйте резервную копию.')
    dest=step('1/4','Проверяем и сохраняем новый релиз',lambda:stage_release(args.release_dir,manifest))
    saved=step('2/4','Сохраняем базу данных и настройки',lambda:backup(values,c))
    try:
        step('3/4','Применяем миграции и переключаем службы',lambda:(run(['bash',dest/'deploy/migrate.sh'],env=db_env(values)),switch(dest),restart(values,c)))
        repair_proxy(c)
        step('4/4','Проверяем новую версию',lambda:health(c,values))
        update_local_cabinet(cabinet,dest,saved)
    except Exception:
        try:
            switch(old); restart(values,c)
        finally:
            restore_local_cabinet(cabinet)
        ui('  Выполнен возврат к прежним бинарникам. Применённые миграции остаются; резервная копия базы сохранена.','33')
        raise
    c['version']=manifest['version']; atomic(ROOT/'installation.json',json.dumps(c,ensure_ascii=False,indent=2)+'\n')
    ui('\n  ✓ Обновлено до '+manifest['version']+'. Настройки и ключи сохранены.','1;32')

def maintenance(action):
    if not (ROOT/'installation.json').exists(): raise InstallError('Установка ещё не завершена.')
    c=private_json(ROOT/'installation.json'); values=read_env(ROOT/'.env')
    if action=='doctor':
        validate_release(ROOT/'current'); health(c,values)
        ui('  ✓ Релиз, PostgreSQL и API панели проверены.','32')
        if local_subscription(c): ui('  ✓ Локальный сервис подписки проверен.','32')
        else: ui('  Подписка размещается отдельно: её готовность проверьте на сервере подписки по инструкции в панели.','90')
        if c['proxy']=='external': ui('  Проверены локальные службы. Внешний HTTPS управляется отдельно.','33')
        return
    if action=='admin-password':
        with terminal() as tty:
            tty.write('  Логин администратора: '); tty.flush(); username=tty.readline().strip()
            password=getpass.getpass('  Новый пароль (12–128 символов): ',stream=tty)
            again=getpass.getpass('  Повторите пароль: ',stream=tty)
        if not re.fullmatch('[a-zA-Z0-9_.@-]{3,64}',username) or not 12<=len(password)<=128 or password!=again: raise InstallError('Проверьте логин и совпадение паролей.')
        SECRETS.append(password)
        exists=sql("SELECT 1 FROM admins WHERE lower(username)=lower('"+username+"');",values)
        if exists!='1': raise InstallError('Администратор не найден. Эта команда не создаёт новые аккаунты.')
        run([ROOT/'current/bin/sn-admin','--password-stdin',username],data=password+'\n',env=db_env(values))
        Path('/root/stealthnet-access.txt').unlink(missing_ok=True)
        ui('  ✓ Пароль изменён.','32')
    elif action=='bot-token':
        token=getpass.getpass('  Токен Telegram-бота: ')
        if not re.fullmatch(r'[0-9]{5,15}:[A-Za-z0-9_-]{20,100}',token): raise InstallError('Некорректный токен.')
        SECRETS.append(token)
        # Validate before changing a working bot. No messages are sent.
        with urllib.request.urlopen('https://api.telegram.org/bot'+token+'/getMe',timeout=10) as response:
            result=json.loads(response.read(65536))
        if not result.get('ok'): raise InstallError('Telegram не принял токен.')
        old=values.copy(); values['BOT_TOKEN']=token
        try:
            atomic(ROOT/'.env',env_text(values)); run(['systemctl','enable','sn-bot']); run(['systemctl','restart','sn-bot'])
            time.sleep(2); run(['systemctl','is-active','--quiet','sn-bot'])
        except Exception:
            atomic(ROOT/'.env',env_text(old))
            if old.get('BOT_TOKEN'): run(['systemctl','restart','sn-bot'])
            else: run(['systemctl','disable','--now','sn-bot'])
            raise
        ui('  ✓ Бот запущен. Оформление и продажи настраиваются в админке.','32')

def main():
    global LOG
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action',choices=('install','update','doctor','admin-password','bot-token'))
    p.add_argument('--release-dir',type=Path); p.add_argument('--config',type=Path)
    args=p.parse_args()
    try:
        os.umask(0o077); os_check()
        lock=open('/run/lock/stealthnet-install.lock','w')
        try: fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError: raise InstallError('Другой установщик уже работает.')
        logdir=Path('/var/log/stealthnet'); logdir.mkdir(mode=0o700,exist_ok=True)
        LOG=(logdir/(args.action+'-'+time.strftime('%Y%m%dT%H%M%S')+'.log')).open('a')
        ui('  Журнал: '+LOG.name,'90')
        if args.action in ('install','update'):
            if not args.release_dir: raise InstallError('Сначала загрузите проверенный релиз через install.sh.')
            manifest=validate_release(args.release_dir)
            (install if args.action=='install' else update)(args,manifest)
        else: maintenance(args.action)
    except (Exception,KeyboardInterrupt) as error:
        message='Установка прервана.' if isinstance(error,KeyboardInterrupt) else str(error)
        write_log(message); ui('\n  × '+redact(message),'31')
        if LOG: ui('  Журнал: '+LOG.name,'90')
        if (ROOT/'.install-pending.json').exists(): ui('  Исправьте причину и повторите ту же команду: сохранённые настройки будут использованы.','33')
        return 1
    return 0

if __name__=='__main__': sys.exit(main())

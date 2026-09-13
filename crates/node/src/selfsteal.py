#!/usr/bin/env python3
"""Private node helper, bundled into sn-node. Input is data, never commands.

Owns only sn-selfsteal.service and its directories. Caddy keeps ACME keys locally.
Preparing a candidate keeps the active domain served until Xray acknowledges it.
"""
import hashlib
import http.client
import io
import json
import os
from pathlib import Path
import pwd
import re
import socket
import ssl
import stat
import subprocess
import sys
import tarfile
import time
import urllib.request

ETC = Path('/etc/sn-selfsteal')
DATA = Path('/var/lib/sn-selfsteal')
BIN = Path('/usr/local/lib/sn-selfsteal/caddy-2.11.4')
UNIT = Path('/etc/systemd/system/sn-selfsteal.service')
SOCKET = 'unix//run/sn-selfsteal/admin.sock'
VERSION = '2.11.4'
HASHES = {
    'x86_64': ('amd64', '527fbf917c39189a1e3b31d34fa955601680b2d5c8055d2a87b8b9588dec7bb9'),
    'aarch64': ('arm64', '52d42ae12b3462097e9868da6dfed3c9648ae12edd3b3638102312af84cb6904'),
}


def run(args, timeout=25, check=True):
    p = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=timeout, env={**os.environ, 'LC_ALL': 'C'})
    if check and p.returncode:
        # No credentials or configuration content in diagnostics.
        raise RuntimeError('Selfsteal: command failed: ' + Path(args[0]).name + ' ' + (args[1] if len(args) > 1 else ''))
    return p


def directory(path, mode=0o755):
    path.mkdir(parents=True, exist_ok=True)
    s = path.lstat()
    if not stat.S_ISDIR(s.st_mode) or s.st_uid != 0:
        raise RuntimeError('Selfsteal: unsafe installation directory')
    path.chmod(mode)


def atomic(path, data, mode=0o644):
    if path.is_symlink():
        raise RuntimeError('Selfsteal: symbolic link in managed files')
    temp = path.with_name(path.name + '.new')
    fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, mode)
    with os.fdopen(fd, 'wb') as f:
        os.fchmod(f.fileno(), mode)
        f.write(data if isinstance(data, bytes) else data.encode())
        f.flush()
        os.fsync(f.fileno())
    os.replace(temp, path)


def read_json(path, default=None):
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        return default


def state(phase, domain=None, error=None, expires=None):
    value = dict(phase=phase, domain=domain, error=error, certificate_expires_at=expires,
                 checked_at=int(time.time()))
    atomic(ETC / 'status.json', json.dumps(value), 0o600)
    return value


def platform():
    if os.geteuid() != 0 or not Path('/run/systemd/system').is_dir():
        raise RuntimeError('Selfsteal needs a native node with systemd / Для Selfsteal нужна нода с обычной установкой и systemd')
    os_info = dict(re.findall(r'^(\w+)=["\']?([^"\'\n]+)', Path('/etc/os-release').read_text(), re.M))
    if (os_info.get('ID'), os_info.get('VERSION_ID')) not in {
        ('debian', '12'), ('debian', '13'), ('ubuntu', '22.04'), ('ubuntu', '24.04'), ('ubuntu', '26.04')
    }:
        raise RuntimeError('Selfsteal supports Debian 12/13 and Ubuntu 22.04/24.04/26.04')


def site_input(value):
    if value is None:
        return None
    if not isinstance(value, dict) or set(value) - {'domain', 'inbound_tag', 'template', 'language', 'title', 'description', 'html'}:
        raise ValueError('Selfsteal: invalid website settings')
    domain = value.get('domain', '')
    if len(domain) > 253 or not re.fullmatch(r'(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z][a-z0-9-]*', domain):
        raise ValueError('Selfsteal: invalid domain')
    if value.get('template') not in ('studio', 'journal', 'travel', 'recipes', 'tools', 'library', 'gallery', 'garden', 'clock'):
        raise ValueError('Selfsteal: unknown website template')
    if value.get('language') not in ('ru', 'en') or not isinstance(value.get('html', ''), str) or len(value.get('html', '').encode()) > 256 * 1024:
        raise ValueError('Selfsteal: invalid website content')
    return value


def install_caddy():
    directory(BIN.parent)
    if BIN.exists():
        if BIN.is_symlink() or BIN.stat().st_uid != 0:
            raise RuntimeError('Selfsteal: unsafe Caddy binary')
        return
    state('downloading')
    arch, digest = HASHES[os.uname().machine]
    url = f'https://github.com/caddyserver/caddy/releases/download/v{VERSION}/caddy_{VERSION}_linux_{arch}.tar.gz'

    class HTTPSOnly(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            if not newurl.startswith('https://'):
                raise RuntimeError('Selfsteal: insecure download redirect')
            return super().redirect_request(req, fp, code, msg, headers, newurl)

    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), HTTPSOnly())
    with opener.open(url, timeout=40) as response:
        raw = response.read(64 * 1024 * 1024 + 1)
    if len(raw) > 64 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != digest:
        raise RuntimeError('Caddy SHA256 verification failed / Не совпала контрольная сумма Caddy')
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:gz') as archive:
        member = archive.getmember('caddy')
        if not member.isfile() or member.size > 128 * 1024 * 1024:
            raise RuntimeError('Selfsteal: invalid Caddy archive')
        binary = archive.extractfile(member).read()
    atomic(BIN, binary, 0o755)
    if not run([str(BIN), 'version']).stdout.startswith(('v' + VERSION).encode()):
        BIN.unlink()
        raise RuntimeError('Selfsteal: Caddy version mismatch')


def setup():
    platform()
    directory(ETC)
    directory(DATA)
    directory(DATA / 'sites')
    try:
        account = pwd.getpwnam('sn-selfsteal')
    except KeyError:
        run(['useradd', '--system', '--user-group', '--home-dir', str(DATA), '--shell', '/usr/sbin/nologin', 'sn-selfsteal'])
        account = pwd.getpwnam('sn-selfsteal')
    if account.pw_uid == 0:
        raise RuntimeError('Selfsteal: invalid service account')
    private = DATA / 'data'
    private.mkdir(exist_ok=True)
    if private.is_symlink() or not private.is_dir():
        raise RuntimeError('Selfsteal: unsafe certificate storage')
    os.chown(private, account.pw_uid, account.pw_gid)
    private.chmod(0o700)
    install_caddy()
    service = f'''[Unit]
Description=Node website for REALITY
After=network-online.target
Wants=network-online.target
[Service]
User=sn-selfsteal
Group=sn-selfsteal
ExecStart={BIN} run --config {ETC}/caddy.json
Restart=on-failure
RestartSec=5
RuntimeDirectory=sn-selfsteal
RuntimeDirectoryMode=0750
Environment=HOME={DATA}/data
Environment=XDG_DATA_HOME={DATA}/data
Environment=XDG_CONFIG_HOME={DATA}/data
UMask=0077
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectControlGroups=true
ReadWritePaths={DATA}/data /run/sn-selfsteal
[Install]
WantedBy=multi-user.target
'''
    if not UNIT.exists() or UNIT.read_text() != service:
        # A foreign service at this exact name is not ours to replace.
        if UNIT.exists() and 'Description=Node website for REALITY' not in UNIT.read_text():
            raise RuntimeError('Selfsteal: service name already belongs to another installation')
        atomic(UNIT, service)
        run(['systemctl', 'daemon-reload'])


def caddy_config(sites):
    domains = [s['domain'] for s in sites]
    routes = []
    for site in sites:
        routes.append({'match': [{'host': [site['domain']]}], 'handle': [
            {'handler': 'headers', 'response': {'set': {'X-Content-Type-Options': ['nosniff'], 'Referrer-Policy': ['no-referrer']}}},
            {'handler': 'file_server', 'root': str(DATA / 'sites' / site['domain']), 'index_names': ['index.html']}
        ], 'terminal': True})
    return {
        'admin': {'listen': SOCKET, 'config': {'persist': False}},
        'storage': {'module': 'file_system', 'root': str(DATA / 'data' / 'certificates')},
        'apps': {
            'tls': {'automation': {'policies': [{'subjects': domains, 'issuers': [{'module': 'acme', 'challenges': {'tls-alpn': {'disabled': True}}}]}]}},
            'http': {'http_port': 80, 'https_port': 9443, 'servers': {
                'website': {'listen': ['127.0.0.1:9443'], 'protocols': ['h1', 'h2'],
                            'automatic_https': {'disable_redirects': True}, 'routes': routes,
                            'tls_connection_policies': [{'match': {'sni': domains}, 'protocol_min': 'tls1.3'}]},
                'http': {'listen': [':80'], 'routes': [
                    {'match': [{'host': domains}], 'handle': [{'handler': 'static_response', 'status_code': 308,
                     'headers': {'Location': ['https://{http.request.host}{http.request.uri}']}}]},
                    {'handle': [{'handler': 'static_response', 'status_code': 404}]}
                ]}
            }}
        }
    }


def configure(sites):
    new = json.dumps(caddy_config(sites), ensure_ascii=False).encode()
    path = ETC / 'caddy.json'
    old = path.read_bytes() if path.exists() else None
    running = run(['systemctl', 'is-active', '--quiet', 'sn-selfsteal.service'], check=False).returncode == 0
    if old == new and running:
        return
    atomic(ETC / 'candidate.json', new)
    run([str(BIN), 'validate', '--config', str(ETC / 'candidate.json')])
    atomic(path, new)
    try:
        if running:
            run([str(BIN), 'reload', '--config', str(path), '--address', SOCKET])
        else:
            # Port ownership is checked before starting; no foreign web server is stopped.
            for host, port in [('0.0.0.0', 80), ('127.0.0.1', 9443)]:
                with socket.socket() as sock:
                    try:
                        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                        sock.bind((host, port))
                    except OSError as e:
                        raise RuntimeError(f'Port {port} is occupied; move the existing website first / Порт {port} занят; сначала перенесите существующий сайт') from e
            run(['systemctl', 'enable', '--now', 'sn-selfsteal.service'])
    except Exception:
        if old is not None:
            atomic(path, old)
            if running:
                run([str(BIN), 'reload', '--config', str(path), '--address', SOCKET], check=False)
        raise


def tls_check(domain):
    # Use the normal trusted CA store. Never accept a self-signed fallback.
    context = ssl.create_default_context()
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.set_alpn_protocols(['h2'])
    with socket.create_connection(('127.0.0.1', 9443), timeout=5) as raw:
        with context.wrap_socket(raw, server_hostname=domain) as tls:
            if tls.selected_alpn_protocol() != 'h2':
                raise RuntimeError('Selfsteal: HTTP/2 is unavailable')
            expires = int(ssl.cert_time_to_seconds(tls.getpeercert()['notAfter']))
    context.set_alpn_protocols(['http/1.1'])
    with socket.create_connection(('127.0.0.1', 9443), timeout=5) as raw:
        with context.wrap_socket(raw, server_hostname=domain) as tls:
            conn = http.client.HTTPConnection(domain, timeout=5)
            conn.sock = tls
            conn.request('GET', '/', headers={'Host': domain, 'Connection': 'close'})
            response = conn.getresponse()
            if response.status != 200 or not response.getheader('Content-Type', '').startswith('text/html'):
                raise RuntimeError('Selfsteal: website is not returning HTML')
    return expires


def prepare(site):
    setup()
    state('preparing', site['domain'])
    root = DATA / 'sites' / site['domain']
    directory(root)
    atomic(root / 'index.html', site['html'])
    # Old and candidate sites share a listener and keep separate certificates.
    active = read_json(ETC / 'active.json')
    sites = [active, site] if active and active['domain'] != site['domain'] else [site]
    configure(sites)
    state('certificate', site['domain'])
    deadline = time.monotonic() + 150
    while True:
        try:
            expires = tls_check(site['domain'])
            return state('ready', site['domain'], expires=expires)
        except (OSError, RuntimeError, http.client.HTTPException):
            if time.monotonic() >= deadline:
                raise RuntimeError('HTTPS is not ready. Point all A/AAAA records to this node, disable CDN proxy and open TCP 80/443. Details: journalctl -u sn-selfsteal / HTTPS не готов. Направьте все A/AAAA на эту ноду, отключите CDN-прокси и откройте TCP 80/443. Подробности: journalctl -u sn-selfsteal')
            if run(['systemctl', 'is-active', '--quiet', 'sn-selfsteal.service'], check=False).returncode:
                raise RuntimeError('Website service failed; check journalctl -u sn-selfsteal / Служба сайта не запустилась; проверьте journalctl -u sn-selfsteal')
            time.sleep(5)


def commit(site):
    if site:
        # Persist first: after a crash the new Xray config still needs this domain.
        atomic(ETC / 'active.json', json.dumps({k: v for k, v in site.items() if k != 'html'}), 0o600)
        configure([site])
    elif UNIT.exists() and 'Description=Node website for REALITY' in UNIT.read_text():
        run(['systemctl', 'disable', '--now', 'sn-selfsteal.service'])
        atomic(ETC / 'active.json', 'null', 0o600)
        state('disabled')
    return read_json(ETC / 'status.json', {'phase': 'disabled'})


def main():
    operation = sys.argv[1] if len(sys.argv) == 2 else ''
    if operation not in ('prepare', 'commit'):
        raise ValueError('Selfsteal: invalid operation')
    raw = sys.stdin.buffer.read(300 * 1024 + 1)
    if len(raw) > 300 * 1024:
        raise ValueError('Selfsteal: input is too large')
    site = site_input(json.loads(raw))
    if operation == 'prepare' and site is None:
        raise ValueError('Selfsteal: missing website')
    return prepare(site) if operation == 'prepare' else commit(site)


if __name__ == '__main__':
    try:
        print(json.dumps(main()))
    except Exception as e:
        error = str(e)[:600] if isinstance(e, (RuntimeError, ValueError)) else 'Selfsteal: ' + type(e).__name__ + '; check DNS, network and service logs / проверьте DNS, сеть и журнал службы'
        if ETC.is_dir():
            state('error', error=error)
        print(json.dumps({'phase': 'error', 'error': error}))
        sys.exit(1)

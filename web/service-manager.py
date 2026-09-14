#!/usr/bin/env python3
"""Local STEALTHNET service commands for panel and standalone installations."""
import argparse
import fcntl
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import urllib.parse

ROOT = Path('/opt/stealthnet-software')
ENV_ROOT = Path('/etc')
BIN_ROOT = Path('/usr/local/bin')
MARKER = '# STEALTHNET standalone service commands\n'


def read_env(path):
    values = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            key, sep, value = line.strip().partition('=')
            if sep and not key.startswith('#'):
                values[key] = value.strip().strip('\"\'')
    return values


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def standalone(validate=True):
    result = []
    for name, key in [('sub', 'PANEL_URL'), ('cabinet', 'PANEL_API_URL')]:
        values = read_env(ENV_ROOT / ('sn-' + name) / 'env')
        if not values or not (BIN_ROOT / ('sn-' + name)).is_file():
            continue
        origin = values.get(key, '').rstrip('/')
        if validate:
            url = urllib.parse.urlsplit(origin)
            if url.scheme != 'https' or not url.hostname or url.username or url.password or url.path or url.query or url.fragment:
                raise ValueError('Проверьте HTTPS-адрес панели в настройках sn-' + name)
        result.append((name, origin))
    return result


def services():
    panel = read_env(ROOT / '.env')
    names = {name for name, _ in standalone(False)}
    if panel:
        names.update(('api', 'sub', 'worker'))
        if panel.get('BOT_TOKEN'):
            names.add('bot')
    selected = []
    for name in ('api', 'sub', 'worker', 'bot', 'cabinet'):
        if name not in names:
            continue
        state = subprocess.run(['systemctl', 'show', 'sn-' + name, '--property=LoadState', '--value'], capture_output=True, text=True)
        if state.stdout.strip() != 'loaded':
            continue
        command = run(['systemctl', 'show', 'sn-' + name, '--property=ExecStart', '--value'], capture_output=True, text=True).stdout
        paths = [ROOT / 'current/bin' / ('sn-' + name), ROOT / 'target/release' / ('sn-' + name)] if panel else []
        if name in {n for n, _ in standalone(False)}:
            paths.append(BIN_ROOT / ('sn-' + name))
        if not any('path=' + str(path) + ' ' in command for path in paths):
            raise ValueError('Нестандартная служба sn-' + name + ': проверьте ExecStart')
        selected.append('sn-' + name)
    if not selected:
        raise ValueError('Установленные службы STEALTHNET не найдены')
    return selected


def atomic(path, content, mode):
    fd, temporary = tempfile.mkstemp(prefix='.' + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
            os.fchmod(stream.fileno(), mode)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def install_entrypoints():
    # The panel's own Makefile remains responsible for its release update.
    if (ROOT / '.env').exists() or (ROOT / 'current/RELEASE.json').exists():
        return
    if not standalone():
        raise ValueError('Кабинет или сервис подписки ещё не установлен')
    ROOT.mkdir(parents=True, exist_ok=True, mode=0o755)
    makefile = ROOT / 'Makefile'
    if makefile.is_symlink() or (makefile.exists() and not makefile.read_text().startswith(MARKER)):
        raise ValueError('Существующий /opt/stealthnet-software/Makefile сохранён; используйте его команды')
    atomic(ROOT / 'service-manager.py', Path(__file__).read_text(), 0o700)
    atomic(makefile, MARKER + '.PHONY: help update start stop restart status\n'
           'help:\n\t@echo "STEALTHNET: make update | start | stop | restart | status"\n'
           'update start stop restart status:\n\t@python3 ./service-manager.py $@\n', 0o644)
    print('Каталог управления: /opt/stealthnet-software. Команды: make update, make start, make stop, make restart, make status.')


def update():
    components = standalone()
    if not components:
        raise ValueError('Отдельные сервисы не найдены. Обновите панель из её каталога.')
    for name, origin in components:
        with tempfile.TemporaryDirectory(prefix='stealthnet-update-') as directory:
            script = Path(directory) / ('update-' + name + '.sh')
            run(['curl', '--proto', '=https', '--proto-redir', '=https', '-fsSL', '--connect-timeout', '15',
                 '--max-time', '60', '--max-filesize', '1048576', origin + '/update-' + name + '.sh', '-o', script], timeout=70)
            run(['bash', '-n', script])
            print('Обновляем sn-' + name, flush=True)
            environment = os.environ.copy()
            for key in ('PANEL_URL', 'PANEL_API_URL', 'SUB_BINARY_URL'):
                environment.pop(key, None)
            run(['bash', script], env=environment)


def control(action):
    selected = services()
    if action == 'status':
        run(['systemctl', '--no-pager', '--full', 'status', *selected])
    else:
        # Start/stop do not alter boot-time enablement or feature settings.
        run(['systemctl', action, *selected])
        print('STEALTHNET ' + action + ': ' + ', '.join(selected))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('install-entrypoints', 'update', 'start', 'stop', 'restart', 'status'))
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise ValueError('Запустите команду от root')
    if args.action == 'install-entrypoints':
        install_entrypoints()
        return
    with open('/run/lock/stealthnet-install.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.action == 'update':
            update()
        else:
            control(args.action)


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print('Ошибка: ' + ('Операция прервана' if isinstance(error, KeyboardInterrupt) else str(error)), file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""Разовое заполнение Reality-параметров инбаундов из профилей.

Миграция 024 добавила колонки, но заполняются они при сохранении профиля.
У панели, которая уже работает, профили никто не пересохранял — и до
первой правки подписка продолжала бы отдавать старые значения из хостов.

Скрипт разбирает конфиги профилей и проставляет параметры так же, как это
делает панель: публичный ключ выводится из приватного, short id и имя
сервера берутся первыми из списков.

    python3 db/backfill_024_reality.py            # показать, что изменится
    python3 db/backfill_024_reality.py --apply    # записать
"""
import base64
import json
import subprocess
import sys

from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

APPLY = '--apply' in sys.argv
PSQL = ['sudo', '-u', 'postgres', 'psql', '-d', 'stealthnet', '-tAc']


def sql(query):
    out = subprocess.run(PSQL + [query], capture_output=True, text=True)
    if out.returncode:
        sys.exit(f'psql: {out.stderr.strip()}')
    return [l for l in out.stdout.split('\n') if l.strip()]


def public_from_private(priv):
    """Xray хранит ключ в base64 без выравнивания, алфавит бывает любой."""
    raw = priv.strip().replace('+', '-').replace('/', '_').rstrip('=')
    try:
        data = base64.urlsafe_b64decode(raw + '=' * (-len(raw) % 4))
    except Exception:
        return None
    if len(data) != 32:
        return None
    pub = X25519PrivateKey.from_private_bytes(data).public_key()
    return base64.urlsafe_b64encode(
        pub.public_bytes(Encoding.Raw, PublicFormat.Raw)).decode().rstrip('=')


rows = sql("SELECT id, name, config::text FROM config_profiles")
changes = []
for line in rows:
    pid, name, cfg = line.split('|', 2)
    for ib in json.loads(cfg).get('inbounds', []):
        r = (ib.get('streamSettings') or {}).get('realitySettings') or {}
        if not r:
            continue
        pub = public_from_private(r.get('privateKey', ''))
        sid = (r.get('shortIds') or [None])[0]
        sni = (r.get('serverNames') or [None])[0]
        if not pub:
            print(f'  ⚠ {name}/{ib.get("tag")}: приватный ключ не разобрался')
            continue
        changes.append((pid, ib.get('tag'), pub, sid, sni))

if not changes:
    print('  Reality-инбаундов не нашлось — заполнять нечего')
    sys.exit(0)

for pid, tag, pub, sid, sni in changes:
    print(f'  профиль {pid} · {tag}: pbk={pub[:16]}… sid={sid} sni={sni}')
    if APPLY:
        q = ("UPDATE inbounds SET public_key = {p}, short_id = {s}, sni = {n} "
             "WHERE profile_id = {i} AND tag = {t}").format(
            p=f"'{pub}'",
            s=f"'{sid}'" if sid else 'NULL',
            n=f"'{sni}'" if sni else 'NULL',
            i=pid, t=f"'{tag}'")
        sql(q)

print('\n  записано' if APPLY else '\n  ничего не менял — запустите с --apply')

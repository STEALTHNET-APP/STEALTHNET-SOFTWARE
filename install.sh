#!/usr/bin/env bash
# Download one immutable STEALTHNET release. No compiler or Git checkout required.
set -Eeuo pipefail
umask 077
export LC_ALL=C.UTF-8
REPO=STEALTHNET-APP/STEALTHNET-SOFTWARE
VERSION=latest
MODE=install
SN_ARGS=()
die() { printf '\033[31m  × %s\033[0m\n' "$*" >&2; exit 1; }
while (($#)); do
  case "$1" in
    --version) [[ $# -ge 2 ]] || die 'Specify --version vX.Y.Z'; VERSION=$2; shift 2;;
    --update) MODE=update; shift;;
    --help|-h) printf 'STEALTHNET installer\n  sudo bash install.sh [--version vX.Y.Z] [--update]\n  See docs/installation.md for unattended installation.\n'; exit 0;;
    *) SN_ARGS+=("$1"); shift;;
  esac
done
[[ $EUID -eq 0 ]] || die 'Запустите установщик от root (sudo bash install.sh).'
[[ $(uname -s) == Linux && -f /etc/os-release ]] || die 'Нужен сервер Debian или Ubuntu с systemd.'
# OS file belongs to the operating system, never user configuration.
. /etc/os-release
case "$ID:$VERSION_ID" in ubuntu:22.04|ubuntu:24.04|ubuntu:26.04|debian:12|debian:13) ;; *) die 'Поддерживаются Debian 12/13 и Ubuntu 22.04/24.04/26.04 LTS.';; esac
case $(uname -m) in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; *) die 'Поддерживаются amd64 и arm64.';; esac
[[ -d /run/systemd/system ]] || die 'Нужен сервер с работающим systemd, не обычный Docker-контейнер.'
printf '\n\033[1;36m  STEALTHNET\033[0m  ·  Установка панели\n\n'
if ! command -v curl >/dev/null || ! command -v python3 >/dev/null || [[ ! -f /etc/ssl/certs/ca-certificates.crt ]]; then
  printf '  → Подготавливаем загрузчик…\n'
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq --no-install-recommends curl ca-certificates python3
fi
SN_TMP=$(mktemp -d /tmp/stealthnet-install.XXXXXXXX)
trap 'rm -rf -- "$SN_TMP"' EXIT
fetch() { curl --proto '=https' --proto-redir '=https' --tlsv1.2 --fail --location --retry 3 --connect-timeout 15 --max-time 600 --silent --show-error "$1" -o "$2"; }
if [[ $VERSION == latest ]]; then
  fetch "https://api.github.com/repos/$REPO/releases/latest" "$SN_TMP/release.json" || die 'Опубликованный релиз не найден или GitHub недоступен. Проверьте Releases и повторите запуск.'
  VERSION=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tag_name"])' "$SN_TMP/release.json")
fi
[[ $VERSION =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9][a-zA-Z0-9.-]*)?$ ]] || die 'Некорректный тег релиза (нужен vX.Y.Z).'
ASSET="stealthnet-$VERSION-linux-$ARCH.tar.gz"
printf '  → Загружаем %s · %s…\n' "$VERSION" "$ARCH"
fetch "https://github.com/$REPO/releases/download/$VERSION/$ASSET" "$SN_TMP/release.tar.gz" || die 'Сборка для этой версии/архитектуры не найдена.'
fetch "https://github.com/$REPO/releases/download/$VERSION/$ASSET.sha256" "$SN_TMP/release.sha256" || die 'Нет контрольной суммы релиза. Установка остановлена.'
python3 - "$SN_TMP" "$VERSION" "$ARCH" <<'PY'
import hashlib,json,pathlib,re,sys,tarfile
root=pathlib.Path(sys.argv[1]); archive=root/'release.tar.gz'
expected=(root/'release.sha256').read_text().strip()
if not re.fullmatch(r'[a-fA-F0-9]{64}',expected): sys.exit('Invalid SHA256 file')
h=hashlib.sha256()
with archive.open('rb') as f:
    for block in iter(lambda:f.read(1024*1024),b''): h.update(block)
if h.hexdigest()!=expected.lower(): sys.exit('SHA256 mismatch: release was NOT installed')
dest=root/'release'; dest.mkdir()
with tarfile.open(archive) as tar:
    members=tar.getmembers()
    if len(members)>10000 or sum(m.size for m in members)>2*1024**3: sys.exit('Archive exceeds limits')
    for m in members:
        p=pathlib.PurePosixPath(m.name)
        if p.is_absolute() or '..' in p.parts or not(m.isfile() or m.isdir()): sys.exit('Unsafe release archive')
    tar.extractall(dest,members=members)
manifest=json.loads((dest/'RELEASE.json').read_text())
if manifest.get('version')!=sys.argv[2] or manifest.get('arch')!=sys.argv[3]: sys.exit('Wrong release manifest')
PY
printf '  ✓ SHA256 проверена\n'
python3 "$SN_TMP/release/deploy/installer.py" "$MODE" --release-dir "$SN_TMP/release" "${SN_ARGS[@]}"

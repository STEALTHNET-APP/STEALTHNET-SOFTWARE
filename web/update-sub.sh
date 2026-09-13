#!/usr/bin/env bash
# Update a standalone subscription service, preserving its settings and key.
set -euo pipefail
umask 077
# Verify the downloaded bytes before any executable permission or probe.
verified_download() {
  local url=$1 dest=$2 hash
  [[ "$url" = https://* ]] || return 1
  curl --proto '=https' --proto-redir '=https' -fsSL --connect-timeout 15 --max-time 180 "$url" -o "$dest" || return 1
  curl --proto '=https' --proto-redir '=https' -fsSL --connect-timeout 15 --max-time 30 "$url.sha256" -o "$dest.sha256" || return 1
  hash=$(awk 'NR==1 {print $1}' "$dest.sha256")
  [[ "$hash" =~ ^[a-fA-F0-9]{64}$ ]] || return 1
  printf '%s  %s\n' "$hash" "$dest" | sha256sum -c - >/dev/null
}
die(){ printf 'Ошибка: %s\n' "$*" >&2; exit 1; }
[[ $EUID -eq 0 ]] || die "запустите от root"
[[ -x /usr/local/bin/sn-sub && -f /etc/sn-sub/env ]] || die "эта команда для отдельного сервера; рядом с панелью используйте update.sh из каталога проекта"
PANEL_URL=${PANEL_URL:-$(sed -n 's/^PANEL_URL=//p' /etc/sn-sub/env)}; PANEL_URL=${PANEL_URL%/}
[[ "$PANEL_URL" =~ ^https://[a-zA-Z0-9._:-]+$ ]] || die "укажите HTTPS-адрес панели в PANEL_URL"
case $(uname -m) in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; *) die "неподдерживаемая архитектура";; esac
TMP=$(mktemp -d /usr/local/bin/.sub-update.XXXXXX); COMMITTED=0
cleanup(){
  code=$?
  if (( code && COMMITTED )); then
    mv -f "$TMP/old" /usr/local/bin/sn-sub
    systemctl restart sn-sub || true
    printf 'Предыдущий бинарник восстановлен; проверьте journalctl -u sn-sub -n 50\n'
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT
verified_download "${SUB_BINARY_URL:-$PANEL_URL/sn-sub-linux-$ARCH}" "$TMP/new" || die "не удалось проверить HTTPS-сборку и её SHA256; рабочая версия сохранена"
[[ $(head -c4 "$TMP/new" | od -An -tx1 | tr -d ' \n') = 7f454c46 ]] || die "панель вернула не Linux-бинарник"
chmod 755 "$TMP/new"
VERSION=$("$TMP/new" --version) && [[ "$VERSION" = sn-sub\ * ]] || die "скачанный сервис не запускается"
cp -p /usr/local/bin/sn-sub "$TMP/old"
COMMITTED=1
mv -f "$TMP/new" /usr/local/bin/sn-sub
systemctl restart sn-sub
ready=
for attempt in {1..10}; do
  if systemctl is-active --quiet sn-sub && curl -fsS --connect-timeout 2 --max-time 12 http://127.0.0.1:8081/ready >/dev/null; then ready=1; break; fi
  sleep 2
done
[[ -n "$ready" ]] || die "обновлённый сервис не готов"
COMMITTED=0
printf '%s обновлён. Настройки и служебный ключ сохранены.\n' "$VERSION"

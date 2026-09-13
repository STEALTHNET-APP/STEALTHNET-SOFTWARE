#!/usr/bin/env bash
set -euo pipefail
umask 077
[[ $EUID -eq 0 && -f /etc/sn-cabinet/env ]] || { echo 'Нужен root и установленный кабинет'; exit 1; }
PANEL_API_URL=$(sed -n 's/^PANEL_API_URL=//p' /etc/sn-cabinet/env)
[[ "$PANEL_API_URL" =~ ^https://[a-zA-Z0-9.-]+$ ]] || { echo 'Некорректный адрес панели';exit 1; }
case $(uname -m) in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; *) exit 1;; esac
TMP=$(mktemp -d);trap 'rm -rf "$TMP"' EXIT
curl -fsS --max-time 180 "$PANEL_API_URL/sn-cabinet-linux-$ARCH" -o "$TMP/sn-cabinet"
HASH=$(curl -fsS --max-time 30 "$PANEL_API_URL/sn-cabinet-linux-$ARCH.sha256" | awk '{print $1}')
[[ "$HASH" =~ ^[a-f0-9]{64}$ ]] || { echo 'Нет контрольной суммы';exit 1; }
printf '%s  %s\n' "$HASH" "$TMP/sn-cabinet" | sha256sum -c -
chmod 755 "$TMP/sn-cabinet"
[[ $("$TMP/sn-cabinet" --version) = sn-cabinet\ * ]] || exit 1
BACKUP="/usr/local/bin/sn-cabinet.previous-$(date -u +%Y%m%dT%H%M%SZ)"
cp -p /usr/local/bin/sn-cabinet "$BACKUP"
install -m755 "$TMP/sn-cabinet" /usr/local/bin/sn-cabinet.new
mv /usr/local/bin/sn-cabinet.new /usr/local/bin/sn-cabinet
systemctl restart sn-cabinet
ready=
for attempt in {1..10};do if curl -fsS --max-time 10 http://127.0.0.1:8090/ready >/dev/null;then ready=1;break;fi;sleep 2;done
if [[ -z "$ready" ]];then cp "$BACKUP" /usr/local/bin/sn-cabinet.rollback;mv /usr/local/bin/sn-cabinet.rollback /usr/local/bin/sn-cabinet;systemctl restart sn-cabinet;echo 'Проверка не прошла. Предыдущая сборка восстановлена';exit 1;fi
printf 'Кабинет обновлён. Предыдущая сборка: %s\n' "$BACKUP"

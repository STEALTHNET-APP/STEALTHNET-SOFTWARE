#!/usr/bin/env bash
# Debian/Ubuntu, either beside the native panel or on a separate server.
set -euo pipefail
umask 077
die(){ printf 'Ошибка: %s\n' "$*" >&2; exit 1; }
say(){ printf '→ %s\n' "$*"; }
[[ $EUID -eq 0 && -d /run/systemd/system ]] || die 'Нужен Linux с systemd, запуск от root'
command -v apt-get >/dev/null || die 'Поддерживаются Debian и Ubuntu'
PANEL_API_URL=${PANEL_API_URL:-}; PANEL_API_URL=${PANEL_API_URL%/}
CABINET_PUBLIC_URL=${CABINET_PUBLIC_URL:-}; CABINET_PUBLIC_URL=${CABINET_PUBLIC_URL%/}
ORIGIN='^https://([a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,63}$'
[[ "$PANEL_API_URL" =~ $ORIGIN && "$CABINET_PUBLIC_URL" =~ $ORIGIN && "$PANEL_API_URL" != "$CABINET_PUBLIC_URL" ]] || die 'Нужны разные внешние HTTPS-домены панели и кабинета без пути'
[[ ${CABINET_SERVICE_KEY:-} =~ ^[a-f0-9]{64}$ ]] || die 'Выпустите ключ в разделе «Кабинет и Mini App»'
[[ ! -e /etc/sn-cabinet/env && ! -e /etc/systemd/system/sn-cabinet.service && ! -e /usr/local/bin/sn-cabinet ]] || die 'Кабинет уже установлен. Для замены ключа исправьте CABINET_SERVICE_KEY в /etc/sn-cabinet/env и выполните systemctl restart sn-cabinet. Обновление: инструкция /cabinet-installation.html на панели.'
if ss -Hltn '( sport = :8090 )' | read -r _; then die 'Порт 8090 занят. Освободите его перед установкой'; fi
CABINET_PROXY=${CABINET_PROXY:-caddy}
[[ $CABINET_PROXY == caddy || $CABINET_PROXY == external ]] || die 'CABINET_PROXY: caddy либо external'
if [[ $CABINET_PROXY == caddy ]] && ss -Hltn '( sport = :80 or sport = :443 )' | read -r _; then
  systemctl is-active --quiet caddy || die '80/443 заняты другим сервером. Настройте существующий reverse proxy по инструкции; установщик не заменяет чужие сайты'
fi
case $(uname -m) in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; *) die 'Архитектура не поддерживается';; esac
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq curl ca-certificates jq >/dev/null
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
say '1/5 · Проверяем кабинет в панели'
curl -fsS --connect-timeout 10 --max-time 30 -H "x-cabinet-service-key: $CABINET_SERVICE_KEY" "$PANEL_API_URL/api/cabinet/config" -o "$TMP/config.json" || die 'Проверьте публикацию кабинета и ключ в админке'
jq -e '.enabled==true and (.brand|length>0)' "$TMP/config.json" >/dev/null || die 'Кабинет не опубликован'
say '2/5 · Загружаем сборку и проверяем контрольную сумму'
URL="$PANEL_API_URL/sn-cabinet-linux-$ARCH"
curl -fsS --connect-timeout 10 --max-time 180 "$URL" -o "$TMP/sn-cabinet" || die 'На панели нет сборки для этой архитектуры'
curl -fsS --connect-timeout 10 --max-time 30 "$URL.sha256" -o "$TMP/hash" || die 'На панели нет контрольной суммы сборки'
HASH=$(awk '{print $1}' "$TMP/hash"); [[ "$HASH" =~ ^[a-f0-9]{64}$ ]] || die 'Некорректная контрольная сумма'
printf '%s  %s\n' "$HASH" "$TMP/sn-cabinet" | sha256sum -c - >/dev/null || die 'Контрольная сумма не совпадает'
chmod 755 "$TMP/sn-cabinet"
[[ $("$TMP/sn-cabinet" --version) = sn-cabinet\ * ]] || die 'Сборка не запускается на этом сервере'
curl -fsS --max-time 30 "$PANEL_API_URL/update-cabinet.sh" -o "$TMP/update-cabinet"
bash -n "$TMP/update-cabinet" || die 'Некорректный скрипт обновления'
say '3/5 · Устанавливаем службу'
install -m700 "$TMP/update-cabinet" /usr/local/sbin/update-cabinet
install -m755 "$TMP/sn-cabinet" /usr/local/bin/sn-cabinet
install -d -m700 /etc/sn-cabinet
cat > /etc/sn-cabinet/env <<ENV
PANEL_API_URL=$PANEL_API_URL
CABINET_PUBLIC_URL=$CABINET_PUBLIC_URL
CABINET_SERVICE_KEY=$CABINET_SERVICE_KEY
CABINET_BIND=127.0.0.1:8090
ENV
cat > /etc/systemd/system/sn-cabinet.service <<'UNIT'
[Unit]
Description=Customer cabinet and Telegram Mini App
After=network-online.target
Wants=network-online.target
[Service]
EnvironmentFile=/etc/sn-cabinet/env
ExecStart=/usr/local/bin/sn-cabinet
Restart=always
RestartSec=5
DynamicUser=yes
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
[Install]
WantedBy=multi-user.target
UNIT
chmod 644 /etc/systemd/system/sn-cabinet.service
systemctl daemon-reload
systemctl enable --now sn-cabinet >/dev/null
ready=
for attempt in {1..12}; do if curl -fsS --max-time 10 http://127.0.0.1:8090/ready >/dev/null; then ready=1;break;fi;sleep 2;done
[[ -n "$ready" ]] || die 'Служба не готова: journalctl -u sn-cabinet -n 50'
if [[ $CABINET_PROXY == external ]]; then
  printf 'Служба готова на 127.0.0.1:8090. Добавьте %s в существующий HTTPS reverse proxy и передавайте X-Real-IP. Инструкция: %s/cabinet-installation.html#proxy\n' "$CABINET_PUBLIC_URL" "$PANEL_API_URL"
  exit 0
fi
say '4/5 · Подключаем HTTPS, сохраняя остальные сайты'
if ! command -v caddy >/dev/null; then
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq gnupg >/dev/null
  curl --proto '=https' --tlsv1.2 -fsSL --max-time 45 https://dl.cloudsmith.io/public/caddy/stable/gpg.key -o "$TMP/caddy.asc"
  gpg --batch --dearmor --output "$TMP/caddy.gpg" "$TMP/caddy.asc"
  install -m644 "$TMP/caddy.gpg" /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  printf '%s\n' 'deb [signed-by=/usr/share/keyrings/caddy-stable-archive-keyring.gpg] https://dl.cloudsmith.io/public/caddy/stable/deb/debian any-version main' > /etc/apt/sources.list.d/caddy-stable.list
  chmod 644 /etc/apt/sources.list.d/caddy-stable.list
  apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq caddy >/dev/null
fi
[[ -f /etc/caddy/Caddyfile ]] || die 'Не найден стандартный /etc/caddy/Caddyfile. Настройте reverse proxy по инструкции'
BACKUP="/etc/caddy/Caddyfile.before-cabinet-$(date -u +%Y%m%dT%H%M%SZ)"
cp -p /etc/caddy/Caddyfile "$BACKUP"
install -d -m755 /etc/caddy/customer-sites
cat > /etc/caddy/customer-sites/cabinet.caddy <<CADDY
$CABINET_PUBLIC_URL {
    encode zstd gzip
    header Strict-Transport-Security "max-age=31536000"
    reverse_proxy 127.0.0.1:8090 {
        header_up X-Real-IP {remote_host}
    }
    header -Server
}
CADDY
chmod 644 /etc/caddy/customer-sites/cabinet.caddy
if ! grep -Fq 'import /etc/caddy/customer-sites/*.caddy' /etc/caddy/Caddyfile; then printf '\nimport /etc/caddy/customer-sites/*.caddy\n' >> /etc/caddy/Caddyfile; fi
if ! caddy validate --config /etc/caddy/Caddyfile >/dev/null; then cp -p "$BACKUP" /etc/caddy/Caddyfile;die "Конфигурация HTTPS не прошла проверку. Предыдущая восстановлена: $BACKUP";fi
systemctl enable --now caddy >/dev/null
systemctl reload caddy
say '5/5 · Проверяем публичный домен'
public=
for attempt in {1..12}; do if curl -fsS --connect-timeout 5 --max-time 10 "$CABINET_PUBLIC_URL/ready" >/dev/null;then public=1;break;fi;sleep 3;done
[[ -n "$public" ]] || die 'Служба работает локально, но HTTPS ещё недоступен. Проверьте DNS A/AAAA, TCP 80/443 и journalctl -u caddy -n 50. Повторная установка не нужна'
printf 'Кабинет: %s\nMini App: %s/app/\nВыберите этот сервер в админке и включите Mini App в настройках бота.\n' "$CABINET_PUBLIC_URL" "$CABINET_PUBLIC_URL"

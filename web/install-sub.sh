#!/usr/bin/env bash
# Debian/Ubuntu: subscription service on a separate server, no database access.
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
die() { printf 'Ошибка: %s\n' "$*" >&2; exit 1; }
say() { printf '→ %s\n' "$*"; }
[[ $EUID -eq 0 ]] || die "запустите от root на сервере подписки"
[[ $(uname -s) = Linux && -d /run/systemd/system ]] || die "нужен Linux с работающим systemd"
command -v apt-get >/dev/null || die "поддерживаются Debian и Ubuntu"
PANEL_URL=${PANEL_URL:-}; PANEL_URL=${PANEL_URL%/}
SUB_PUBLIC_URL=${SUB_PUBLIC_URL:-}; SUB_PUBLIC_URL=${SUB_PUBLIC_URL%/}
ORIGIN='^https://([a-zA-Z0-9._-]+|\[[0-9a-fA-F:]+\])(:[0-9]+)?$'
[[ "$PANEL_URL" =~ $ORIGIN ]] || die "PANEL_URL должен быть HTTPS-адресом панели без пути, параметров и пробелов"
[[ "$SUB_PUBLIC_URL" =~ $ORIGIN ]] || die "SUB_PUBLIC_URL должен быть HTTPS-адресом подписки без пути и параметров"
DOMAIN=${SUB_PUBLIC_URL#*://}; HOST=${DOMAIN%%:*}
case "$DOMAIN" in localhost|localhost:*|127.*|0.0.0.0*|'[::1]'*|'[::]'*) die "нужен внешний адрес подписки, доступный клиентам";; esac
[[ "$PANEL_URL" != "$SUB_PUBLIC_URL" ]] || die "панель и подписка должны иметь разные домены"
[[ "${SUB_SERVICE_TOKEN:-}" =~ ^[a-zA-Z0-9_-]{20,200}$ ]] || die "вставьте служебный ключ из Настройки → Сервис подписки"
# The primary installer already creates sn-sub. Do not replace that service or lose its DB setup.
[[ ! -e /etc/sn-sub/env && ! -e /etc/systemd/system/sn-sub.service && ! -e /usr/local/bin/sn-sub ]] || die "sn-sub уже установлен. Повторная установка отменена; настройки и ключ сохранены. Инструкция обновления: /subscription-installation.html на панели"
case $(uname -m) in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; *) die "неподдерживаемая архитектура";; esac
missing=()
for dep in curl jq; do command -v "$dep" >/dev/null || missing+=("$dep"); done
[[ -f /etc/ssl/certs/ca-certificates.crt ]] || missing+=(ca-certificates)
if (( ${#missing[@]} )); then apt-get update -qq; DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${missing[@]}"; fi
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
say "1/4 · Проверяем панель и служебный ключ"
printf 'header = "x-service-token: %s"\n' "$SUB_SERVICE_TOKEN" | curl --config - -fsS --connect-timeout 10 --max-time 20 "$PANEL_URL/api/sub/settings" -o "$TMP/settings.json" || die "панель не подтвердила ключ. Проверьте адрес, ключ и доступ по HTTPS"
jq -e 'type=="object"' "$TMP/settings.json" >/dev/null || die "панель вернула не настройки API; проверьте домен и reverse proxy"
say "2/4 · Загружаем сервис подписки ($ARCH)"
GH_REPO=${GH_REPO:-STEALTHNET-APP/STEALTHNET-SOFTWARE}
SUB_VERSION=${SUB_VERSION:-latest}
[[ "$SUB_VERSION" =~ ^[a-zA-Z0-9._-]+$ ]] || die "некорректный тег сборки"
sources=("$PANEL_URL/sn-sub-linux-$ARCH" "https://github.com/$GH_REPO/releases/latest/download/sn-sub-linux-$ARCH")
[[ "$SUB_VERSION" = latest ]] || sources=("https://github.com/$GH_REPO/releases/download/$SUB_VERSION/sn-sub-linux-$ARCH")
[[ -z ${SUB_BINARY_URL:-} ]] || sources=("$SUB_BINARY_URL")
found=
for url in "${sources[@]}"; do
  if verified_download "$url" "$TMP/sn-sub" && [[ $(head -c4 "$TMP/sn-sub" | od -An -tx1 | tr -d ' \n') = 7f454c46 ]]; then
    chmod 755 "$TMP/sn-sub"
    if VERSION=$("$TMP/sn-sub" --version) && [[ "$VERSION" = sn-sub\ * ]]; then found=1; break; fi
  fi
done
[[ -n "$found" ]] || die "не найдена работающая сборка sn-sub для $ARCH. Обновите панель или укажите SUB_BINARY_URL"
say "3/4 · Устанавливаем службу"
install -d -m700 /etc/sn-sub
install -m755 "$TMP/sn-sub" /usr/local/bin/sn-sub
cat > /etc/sn-sub/env <<ENV
SUB_MODE=api
PANEL_URL=$PANEL_URL
SUB_SERVICE_TOKEN=$SUB_SERVICE_TOKEN
SUB_PUBLIC_URL=$SUB_PUBLIC_URL
SUB_BIND=127.0.0.1:8081
RUST_LOG=sn_sub=info
ENV
chmod 600 /etc/sn-sub/env
cat > /etc/systemd/system/sn-sub.service <<'UNIT'
[Unit]
Description=STEALTHNET subscription service
After=network-online.target
Wants=network-online.target
[Service]
EnvironmentFile=/etc/sn-sub/env
ExecStart=/usr/local/bin/sn-sub
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
chmod 644 /etc/systemd/system/sn-sub.service
systemctl daemon-reload
systemctl enable --now sn-sub
say "4/4 · Проверяем готовность и доступ к данным панели"
ready=
for attempt in {1..12}; do
  if systemctl is-active --quiet sn-sub && curl -fsS --connect-timeout 2 --max-time 12 http://127.0.0.1:8081/ready -o "$TMP/ready.json" && jq -e '.status=="ready"' "$TMP/ready.json" >/dev/null; then ready=1; break; fi
  sleep 2
done
[[ -n "$ready" ]] || die "служба не готова. Смотрите journalctl -u sn-sub -n 50; проверьте ключ и занятость порта 8081. Не запускайте установку повторно: исправьте /etc/sn-sub/env и выполните systemctl restart sn-sub"
cat <<EOF

$VERSION работает и получает данные панели. База на этом сервере не нужна.
Осталось настроить публичный домен и HTTPS:
1. Направьте DNS A/AAAA домена $DOMAIN на этот сервер. Уберите неверный AAAA.
2. Откройте входящие TCP 80 и 443. Порт 8081 оставьте закрытым снаружи.
3. Добавьте в существующий /etc/caddy/Caddyfile (не заменяйте другие сайты):

$SUB_PUBLIC_URL {
    encode zstd gzip
    reverse_proxy 127.0.0.1:8081
    header -Server
}

4. caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy
5. curl -fsS $SUB_PUBLIC_URL/ready
6. Откройте реальную ссылку клиента /s/ID в браузере и импортируйте её в VPN-приложение.

Публичный HTTPS ещё не проверен установщиком. Caddy и DNS настраиваются по инструкции:
$PANEL_URL/subscription-installation.html
EOF

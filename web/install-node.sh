#!/usr/bin/env bash
# Fresh Debian/Ubuntu node installation. The panel supplies the selected Xray version.
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
[[ $EUID -eq 0 ]] || die "запустите от root на сервере ноды"
[[ $(uname -s) = Linux ]] || die "нужен Linux с systemd (Debian или Ubuntu)"
[[ -d /run/systemd/system ]] || die "нужен запущенный systemd; для контейнера используйте образ ноды"
command -v apt-get >/dev/null || die "установщик поддерживает Debian и Ubuntu"
PANEL_URL=${PANEL_URL:-}; PANEL_URL=${PANEL_URL%/}
URL_PATTERN='^https://([a-zA-Z0-9._-]+|\[[0-9a-fA-F:]+\])(:[0-9]+)?(/[a-zA-Z0-9._/-]+)?$'
[[ "$PANEL_URL" =~ $URL_PATTERN ]] || die "PANEL_URL должен быть HTTPS-адресом панели без параметров и пробелов"
[[ "${NODE_SECRET:-}" =~ ^[a-zA-Z0-9_-]{20,200}$ ]] || die "неверный NODE_SECRET: скопируйте команду из карточки ноды"
# Never overwrite another node's identity, executable or active service during setup.
[[ ! -e /etc/sn-node/node.env && ! -e /etc/systemd/system/sn-node.service && ! -e /usr/local/bin/sn-node && ! -e /usr/local/bin/xray ]] || die "на сервере уже есть нода или Xray. Для обновления используйте update-node.sh; настройки сохранены"
case "$(uname -m)" in
  x86_64) XA=64; NA=amd64 ;;
  aarch64|arm64) XA=arm64-v8a; NA=arm64 ;;
  *) die "архитектура $(uname -m) не поддерживается" ;;
esac
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
say "1/5 · Проверяем зависимости"
missing=()
for dep in curl unzip jq python3; do command -v "$dep" >/dev/null || missing+=("$dep"); done
[[ -f /etc/ssl/certs/ca-certificates.crt ]] || missing+=(ca-certificates)
if (( ${#missing[@]} )); then apt-get update -qq; DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${missing[@]}"; fi
bootstrap() {
  printf 'header = "x-node-secret: %s"\n' "$NODE_SECRET" | curl --config - -fsS --connect-timeout 10 --max-time 20 "$PANEL_URL/api/node/bootstrap" -o "$TMP/bootstrap.json" &&
    jq -e '.node_id > 0 and (.engine_target|type=="string")' "$TMP/bootstrap.json" >/dev/null
}
say "2/5 · Проверяем панель и ключ ноды"
bootstrap || die "панель не подтвердила ключ. Проверьте PANEL_URL, доступ к панели и перевыпустите секрет, если он устарел"
if jq -e '.profile_assigned == true and .inbound_count == 0' "$TMP/bootstrap.json" >/dev/null; then
  die "выберите активные инбаунды или вариант «Без профиля» в панели, затем повторите установку"
fi
XRAY_VERSION=${XRAY_VERSION:-$(jq -r .engine_target "$TMP/bootstrap.json")}
[[ "$XRAY_VERSION" =~ ^v?[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}$ ]] || die "в панели указан неверный тег Xray"
XRAY_VERSION=v${XRAY_VERSION#v}
say "3/5 · Загружаем и проверяем Xray $XRAY_VERSION и агент ($NA)"
BASE="https://github.com/XTLS/Xray-core/releases/download/$XRAY_VERSION/Xray-linux-$XA.zip"
curl --proto '=https' --proto-redir '=https' -fsSL --connect-timeout 15 --max-time 300 "$BASE" -o "$TMP/xray.zip"
curl --proto '=https' --proto-redir '=https' -fsSL --connect-timeout 15 --max-time 60 "$BASE.dgst" -o "$TMP/xray.dgst"
HASH=$(sed -n 's/^SHA2-256= *//p' "$TMP/xray.dgst" | tr -d '\r')
[[ "$HASH" =~ ^[a-fA-F0-9]{64}$ ]] || die "в релизе отсутствует SHA256"
printf '%s  %s\n' "$HASH" "$TMP/xray.zip" | sha256sum -c - >/dev/null || die "повреждён архив Xray; установка отменена"
unzip -oq "$TMP/xray.zip" -d "$TMP/xray"
is_elf() { [[ $(head -c4 "$1" | od -An -tx1 | tr -d ' \n') = 7f454c46 ]]; }
is_elf "$TMP/xray/xray" || die "в архиве нет бинарника Linux"
chmod 755 "$TMP/xray/xray"
VERSION=$("$TMP/xray/xray" version) || die "скачанный Xray не запускается"
[[ $(awk 'NR==1{print $2}' <<<"$VERSION") = "${XRAY_VERSION#v}" ]] || die "Xray сообщил неверную версию"
GH_REPO=${GH_REPO:-STEALTHNET-APP/STEALTHNET-SOFTWARE}
AGENT_RELEASE=${AGENT_RELEASE:-latest}
GH_URL="https://github.com/$GH_REPO/releases/latest/download/sn-node-linux-$NA"
[[ "$AGENT_RELEASE" = latest ]] || GH_URL="https://github.com/$GH_REPO/releases/download/$AGENT_RELEASE/sn-node-linux-$NA"
sources=("$PANEL_URL/sn-node-linux-$NA" "$GH_URL")
[[ "$AGENT_RELEASE" = latest ]] || sources=("$GH_URL")
[[ -z "${AGENT_URL:-}" ]] || sources=("$AGENT_URL")
got=
for url in "${sources[@]}"; do
  if verified_download "$url" "$TMP/sn-node" && is_elf "$TMP/sn-node"; then
    chmod 755 "$TMP/sn-node"
    if AGENT_VERSION=$("$TMP/sn-node" --version) && [[ "$AGENT_VERSION" = sn-node\ * ]]; then got=1; break; fi
  fi
done
[[ -n "$got" ]] || die "нет работающей сборки агента $NA на панели. Загрузите sn-node-linux-$NA в каталог сайта панели и повторите установку"
# Commit only after both executables and panel credentials passed validation.
say "4/5 · Устанавливаем службу"
install -d -m755 /usr/local/share/xray
install -d -m700 /etc/sn-node
install -m755 "$TMP/xray/xray" /usr/local/bin/xray
install -m755 "$TMP/sn-node" /usr/local/bin/sn-node
install -m644 "$TMP/xray/geoip.dat" "$TMP/xray/geosite.dat" /usr/local/share/xray/
cat > /etc/sn-node/node.env <<ENV
PANEL_URL=$PANEL_URL
NODE_SECRET=$NODE_SECRET
ENGINE_BIN=/usr/local/bin/xray
ENGINE_CONFIG=/etc/sn-node/config.json
ENGINE_API=127.0.0.1:10085
XRAY_LOCATION_ASSET=/usr/local/share/xray
ENV
chmod 600 /etc/sn-node/node.env
cat > /etc/systemd/system/sn-node.service <<'UNIT'
[Unit]
Description=STEALTHNET node agent
After=network-online.target
Wants=network-online.target
[Service]
EnvironmentFile=/etc/sn-node/node.env
ExecStart=/usr/local/bin/sn-node
Restart=always
RestartSec=5
UMask=0077
[Install]
WantedBy=multi-user.target
UNIT
chmod 644 /etc/systemd/system/sn-node.service
command -v nft >/dev/null || DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nftables >/dev/null 2>&1 || say "nftables не установлен: сетевые плагины недоступны"
cat > /usr/local/bin/sn-geo-update <<'GEO'
#!/usr/bin/env bash
set -euo pipefail
DIR=/usr/local/share/xray
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

CHANGED=0

fetch() {   # fetch <имя файла> <url>
  curl --proto '=https' --proto-redir '=https' -fsSL --max-time 120 -o "$TMP/$1" "$2" || { echo "не скачал $1"; return 1; }
  # Гео-база меньше мегабайта — это заглушка или страница с ошибкой.
  [ "$(stat -c%s "$TMP/$1")" -gt 1000000 ] || { echo "$1 подозрительно мал"; return 1; }
  # Не подменяем одинаковое: движок читает эти файлы только при запуске,
  # и лишний перезапуск — это лишний обрыв всех соединений на ноде.
  if [ -f "$DIR/$1" ] && cmp -s "$TMP/$1" "$DIR/$1"; then
    echo "$1 не изменился"
    return 0
  fi
  install -m644 "$TMP/$1" "$DIR/$1.new"
  mv -f "$DIR/$1.new" "$DIR/$1"
  CHANGED=1
  echo "обновлён $1"
}

BASE=https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download
fetch geoip.dat   "$BASE/geoip.dat"
fetch geosite.dat "$BASE/geosite.dat"

# Список РКН. Нужен, только если в правилах маршрутизации есть zapret:*.
# Без файла Xray с таким правилом не запустится вовсе, поэтому качаем
# заранее, но неудачу не считаем ошибкой всей задачи.
fetch zapret.dat "https://github.com/runetfreedom/russia-blocked-geosite/releases/latest/download/geosite.dat" || true

# Перезапуск только при настоящем обновлении. Движок читает гео-базы
# при старте, поэтому иначе новые списки не применятся, — но и дёргать
# ноду, когда ничего не поменялось, незачем.
if [ "$CHANGED" = "1" ] && [ "${SN_GEO_NO_RESTART:-0}" != "1" ]; then
  echo "перезапускаем движок, чтобы подхватить новые списки"
  systemctl restart sn-node 2>/dev/null || systemctl reload-or-restart xray 2>/dev/null || true
else
  echo "изменений нет — движок не трогаем"
fi
GEO
chmod +x /usr/local/bin/sn-geo-update

cat > /etc/systemd/system/sn-geo-update.service <<'UNIT'
[Unit]
Description=Обновление гео-баз Xray
[Service]
Type=oneshot
ExecStart=/usr/local/bin/sn-geo-update
UNIT

cat > /etc/systemd/system/sn-geo-update.timer <<'UNIT'
[Unit]
Description=Еженедельное обновление гео-баз Xray
[Timer]
OnCalendar=weekly
# Со случайным разбросом: иначе весь парк нод ломится за файлами в одну секунду.
RandomizedDelaySec=6h
Persistent=true
[Install]
WantedBy=timers.target
UNIT

systemctl daemon-reload
systemctl enable --now sn-geo-update.timer >/dev/null 2>&1 || true
# Первый прогон сразу: свежие списки лучше тех, что лежали в архиве Xray.
SN_GEO_NO_RESTART=1 /usr/local/bin/sn-geo-update || say "гео-базы не обновились — останутся из архива Xray"


systemctl daemon-reload
systemctl enable --now sn-node
say "5/5 · Ждём подтверждение агента и запуск Xray (до 90 секунд)"
for attempt in {1..18}; do
  sleep 5
  if bootstrap; then
    if jq -e '.ready == true' "$TMP/bootstrap.json" >/dev/null; then
      say "Готово: панель видит ноду, Xray работает. $AGENT_VERSION · Xray ${XRAY_VERSION#v}"
      say "Теперь проверьте доступность порта VPN снаружи и подключение тестового клиента. Логи: journalctl -u sn-node -f"
      exit 0
    fi
    if jq -e '.idle_ready == true' "$TMP/bootstrap.json" >/dev/null; then
      say "Готово: агент на связи, нода без профиля. $AGENT_VERSION · Xray ${XRAY_VERSION#v}"
      say "Ноду можно выбрать для проверки профиля. Чтобы включить VPN, назначьте профиль в панели. Логи: journalctl -u sn-node -f"
      exit 0
    fi
  fi
done
die "файлы установлены, но панель ещё не подтвердила работающий Xray. Проверьте профиль, сертификаты и занятые порты: journalctl -u sn-node -n 50. Повторная установка не нужна"

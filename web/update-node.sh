#!/usr/bin/env bash
# Update the agent without replacing its identity or configuration.
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
[[ $EUID -eq 0 ]] || die "запустите от root"
# Legacy panel installations start the agent from the project directory.
# Replace the binary systemd actually starts, not an unrelated installed copy.
EXEC_START=$(systemctl show sn-node --property=ExecStart --value)
AGENT_BIN=$(sed -n 's/.*path=\([^ ;]*\).*/\1/p' <<<"$EXEC_START")
[[ "$AGENT_BIN" = /*/sn-node && "$AGENT_BIN" != *'/../'* && -x "$AGENT_BIN" ]] || die "не удалось определить бинарник sn-node из systemd; проверьте systemctl cat sn-node"
if [[ -z ${PANEL_URL:-} ]]; then
  if [[ -f /etc/sn-node/node.env ]]; then
    PANEL_URL=$(sed -n 's/^PANEL_URL=//p' /etc/sn-node/node.env)
  else
    PANEL_URL=$(sed -n 's/^Environment=PANEL_URL=//p' /etc/systemd/system/sn-node.service)
  fi
fi
PANEL_URL=${PANEL_URL:-}; PANEL_URL=${PANEL_URL%/}
URL_PATTERN='^https://([a-zA-Z0-9._-]+|\[[0-9a-fA-F:]+\])(:[0-9]+)?(/[a-zA-Z0-9._/-]+)?$'
[[ "$PANEL_URL" =~ $URL_PATTERN ]] || die "не найден корректный PANEL_URL; укажите адрес панели"
case "$(uname -m)" in x86_64) NA=amd64; XA=64;; aarch64|arm64) NA=arm64; XA=arm64-v8a;; *) die "архитектура не поддерживается";; esac
command -v curl >/dev/null || die "установите curl: apt-get install -y curl ca-certificates"
TMP=$(mktemp -d "$(dirname "$AGENT_BIN")/.sn-update.XXXXXX")
COMMITTED=0; XRAY_CHANGED=0
replace_core() {
  local next
  next=$(mktemp /usr/local/bin/.xray-update.XXXXXX)
  install -m755 "$1" "$next"
  mv -f "$next" /usr/local/bin/xray
}
cleanup() {
  code=$?
  if (( code != 0 && COMMITTED == 1 )); then
    say "Новая служба не запустилась; восстанавливаем предыдущие бинарники"
    mv -f "$TMP/sn-node.old" "$AGENT_BIN"
    if (( XRAY_CHANGED )); then replace_core "$TMP/xray.old"; fi
    systemctl restart sn-node || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT
is_elf() { [[ $(head -c4 "$1" | od -An -tx1 | tr -d ' \n') = 7f454c46 ]]; }
sources=("$PANEL_URL/sn-node-linux-$NA" "https://github.com/${GH_REPO:-STEALTHNET-APP/STEALTHNET-SOFTWARE}/releases/latest/download/sn-node-linux-$NA")
[[ -z ${AGENT_URL:-} ]] || sources=("$AGENT_URL")
got=
for url in "${sources[@]}"; do
  if verified_download "$url" "$TMP/sn-node" && is_elf "$TMP/sn-node"; then
    chmod 755 "$TMP/sn-node"
    if VERSION=$("$TMP/sn-node" --version) && [[ "$VERSION" = sn-node\ * ]]; then got=1; break; fi
  fi
done
[[ -n "$got" ]] || die "не удалось скачать работающую сборку агента $NA; старая версия сохранена"
# Optional explicit core update. Otherwise the panel remains the version authority.
if [[ -n ${XRAY_VERSION:-} ]]; then
  [[ "$XRAY_VERSION" =~ ^v?[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}$ ]] || die "неверный тег Xray"
  command -v unzip >/dev/null || die "установите unzip: apt-get install -y unzip"
  WANT=${XRAY_VERSION#v}
  CURRENT=$(/usr/local/bin/xray version | awk 'NR==1{print $2}')
  if [[ "$CURRENT" != "$WANT" ]]; then
    BASE="https://github.com/XTLS/Xray-core/releases/download/v$WANT/Xray-linux-$XA.zip"
    curl --proto '=https' --proto-redir '=https' -fsSL --connect-timeout 15 --max-time 300 "$BASE" -o "$TMP/xray.zip"
    curl --proto '=https' --proto-redir '=https' -fsSL --connect-timeout 15 --max-time 60 "$BASE.dgst" -o "$TMP/xray.dgst"
    HASH=$(sed -n 's/^SHA2-256= *//p' "$TMP/xray.dgst" | tr -d '\r')
    [[ "$HASH" =~ ^[a-fA-F0-9]{64}$ ]] || die "нет SHA256 в релизе Xray"
    printf '%s  %s\n' "$HASH" "$TMP/xray.zip" | sha256sum -c - >/dev/null || die "SHA256 Xray не совпадает"
    unzip -oq "$TMP/xray.zip" xray -d "$TMP"
    is_elf "$TMP/xray" || die "в архиве нет бинарника Linux"
    chmod 755 "$TMP/xray"
    PROBE=$("$TMP/xray" version) || die "скачанный Xray не запускается"
    [[ $(awk 'NR==1{print $2}' <<<"$PROBE") = "$WANT" ]] || die "версия Xray не совпадает"
    XRAY_LOCATION_ASSET=/usr/local/share/xray "$TMP/xray" run -test -c /etc/sn-node/config.json >"$TMP/config-check.log" 2>&1 || die "новый Xray отверг текущий конфиг; рабочие бинарники сохранены"
    cp -p /usr/local/bin/xray "$TMP/xray.old"
    XRAY_CHANGED=1
  fi
fi
cp -p "$AGENT_BIN" "$TMP/sn-node.old"
COMMITTED=1
mv -f "$TMP/sn-node" "$AGENT_BIN"
if (( XRAY_CHANGED )); then replace_core "$TMP/xray"; fi
systemctl restart sn-node
sleep 5
systemctl is-active --quiet sn-node || die "служба не поднялась: journalctl -u sn-node -n 50"
COMMITTED=0
say "Установлен $VERSION. Нода отчитается в панель в течение 15 секунд. Проверьте статус Xray в карточке ноды"

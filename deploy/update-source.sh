#!/usr/bin/env bash
# Update code already present in this directory; preserve installed settings.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo 'Run as root'; exit 1; }
APP_DIR=$(cd "$(dirname "$0")/.." && pwd)
[[ -f "$APP_DIR/.env" ]] || { echo 'No .env found. Use install.sh for a new installation.'; exit 1; }
# EnvironmentFile is data, never a shell program. Do not source it.
read_setting() { python3 - "$APP_DIR/.env" "$1" <<'PY'
import sys
for line in open(sys.argv[1]):
    key,sep,value=line.strip().partition('=')
    if sep and key==sys.argv[2]:
        value=value.strip()
        if len(value)>1 and value[0]==value[-1] and value[0] in "\"'": value=value[1:-1]
        print(value);break
PY
}
export DATABASE_URL=$(read_setting DATABASE_URL)
WEB_ROOT=$(read_setting WEB_ROOT)
[[ -n "$DATABASE_URL" && -n "$WEB_ROOT" && "$WEB_ROOT" != / ]] || { echo 'DATABASE_URL or WEB_ROOT is missing'; exit 1; }
BACKUP_DIR="$APP_DIR/backups/$(date -u +%Y%m%dT%H%M%SZ)"
umask 077
mkdir -p "$BACKUP_DIR/bin"
cp "$APP_DIR/.env" "$BACKUP_DIR/env"
python3 "$APP_DIR/deploy/pg-env.py" pg_dump -Fc -f "$BACKUP_DIR/database.dump"
[[ ! -d "$APP_DIR/shared" ]] || cp -a "$APP_DIR/shared" "$BACKUP_DIR/shared"
for svc in api sub bot worker node; do
  [[ ! -f "$APP_DIR/target/release/sn-$svc" ]] || cp "$APP_DIR/target/release/sn-$svc" "$BACKUP_DIR/bin/"
done
[[ ! -f /usr/local/bin/sn-node ]] || cp /usr/local/bin/sn-node "$BACKUP_DIR/bin/sn-node-installed"
tar -czf "$BACKUP_DIR/web.tgz" -C "$WEB_ROOT" .
[[ ! -f "$HOME/.cargo/env" ]] || source "$HOME/.cargo/env"
cd "$APP_DIR"
# Keep the running services' binaries intact until both build and migrations succeed.
cargo build --release --locked --workspace --target-dir "$APP_DIR/target/update"
bash "$APP_DIR/deploy/migrate.sh"
# Preserve the independent encryption key across updates. Never replace a lost
# key while encrypted codes exist; restore the original environment backup.
if [[ -z $(read_setting CABINET_CODE_KEY) ]]; then
  sealed=$(python3 "$APP_DIR/deploy/pg-env.py" psql -X -Atqc 'SELECT EXISTS(SELECT 1 FROM cabinet_credentials WHERE code_sealed IS NOT NULL)')
  [[ "$sealed" == f ]] || { echo 'Restore CABINET_CODE_KEY from the panel environment backup before updating.'; exit 1; }
  printf '\nCABINET_CODE_KEY=%s\n' "$(openssl rand -hex 32)" >> "$APP_DIR/.env"
  chmod 600 "$APP_DIR/.env"
fi
mkdir -p "$APP_DIR/target/release"
for binary in "$APP_DIR"/target/update/release/sn-*; do
  [[ -f "$binary" && -x "$binary" && "$binary" != *.d ]] || continue
  name=$(basename "$binary")
  install -m755 "$binary" "$APP_DIR/target/release/$name.new"
  mv -f "$APP_DIR/target/release/$name.new" "$APP_DIR/target/release/$name"
done
python3 - "$APP_DIR/web" "$WEB_ROOT" <<'PY'
from pathlib import Path
import shutil,sys
source,destination=map(Path,sys.argv[1:])
for path in source.rglob('*'):
    relative=path.relative_to(source)
    if any(p in ('.impeccable','__pycache__') for p in relative.parts) or path.name=='DESIGN.md': continue
    target=destination/relative
    if path.is_dir(): target.mkdir(parents=True,exist_ok=True); continue
    # Existing operator assets take precedence over bundled example branding.
    if relative.parts[0] in ('customer-brand','uploads','custom') and target.exists(): continue
    if path.resolve()!=target.resolve(): shutil.copy2(path,target)
PY
cp "$APP_DIR/web/miniapp-unavailable.html" "$WEB_ROOT/app/index.html"
case $(uname -m) in x86_64) SN_ARCH=amd64;; aarch64) SN_ARCH=arm64;; *) SN_ARCH=;; esac
if [[ -n "$SN_ARCH" ]]; then
  install -m755 target/release/sn-node "$WEB_ROOT/sn-node-linux-$SN_ARCH"
  install -m755 target/release/sn-sub "$WEB_ROOT/sn-sub-linux-$SN_ARCH"
  install -m755 "$APP_DIR/target/release/sn-cabinet" "$WEB_ROOT/sn-cabinet-linux-$SN_ARCH"
  for component in sn-node sn-sub sn-cabinet; do
    sha256sum "$WEB_ROOT/$component-linux-$SN_ARCH" | awk '{print $1}' > "$WEB_ROOT/$component-linux-$SN_ARCH.sha256"
  done
fi
if [[ ! -f /etc/systemd/system/sn-worker.service ]]; then
  cat > /etc/systemd/system/sn-worker.service <<EOF
[Unit]
Description=STEALTHNET background worker
After=network.target postgresql.service
[Service]
EnvironmentFile=$APP_DIR/.env
ExecStart=$APP_DIR/target/release/sn-worker
WorkingDirectory=$APP_DIR
Restart=always
RestartSec=3
NoNewPrivileges=true
ProtectSystem=full
PrivateTmp=true
[Install]
WantedBy=multi-user.target
EOF
fi
systemctl daemon-reload
systemctl enable sn-worker
services=(sn-api sn-sub sn-worker)
if systemctl is-enabled --quiet sn-bot; then services+=(sn-bot); fi
if systemctl is-enabled --quiet sn-node; then
  if [[ -f /usr/local/bin/sn-node ]]; then
    install -m755 target/release/sn-node /usr/local/bin/sn-node.new
    mv -f /usr/local/bin/sn-node.new /usr/local/bin/sn-node
  fi
  services+=(sn-node)
fi
systemctl restart "${services[@]}"
sleep 3
systemctl is-active "${services[@]}"
echo "Updated. Backup: $BACKUP_DIR"

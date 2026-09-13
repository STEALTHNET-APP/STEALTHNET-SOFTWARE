#!/usr/bin/env bash
set -euo pipefail
SN_DIR=$(cd "$(dirname "$0")" && pwd)
if [[ -f "$SN_DIR/current/RELEASE.json" ]]; then
  exec bash "$SN_DIR/current/install.sh" --update "$@"
fi
if [[ -f "$SN_DIR/RELEASE.json" ]]; then
  exec bash "$SN_DIR/install.sh" --update "$@"
fi
# Source installations remain supported; do not migrate a running layout implicitly.
exec bash "$SN_DIR/deploy/update-source.sh" "$@"

#!/usr/bin/env bash
# Upload source, then use the same backed-up update path as installed panels.
set -euo pipefail
: "${HOST:?Set HOST to the SSH destination}"
SRC=${SRC:-/opt/stealthnet-software}
[[ "$HOST" =~ ^[a-zA-Z0-9_@.-]+$ ]] || { echo 'Invalid SSH host' >&2; exit 1; }
[[ "$SRC" =~ ^/[a-zA-Z0-9_./-]+$ && "$SRC" != / ]] || { echo 'Invalid remote source path' >&2; exit 1; }
[[ -z "${MIGRATION_BASELINE:-}" || "$MIGRATION_BASELINE" =~ ^[0-9]{1,3}$ ]] || { echo 'Invalid migration baseline' >&2; exit 1; }
cd "$(dirname "$0")/.."
archive=$(mktemp /tmp/sn-source.XXXXXX)
trap 'rm -f "$archive"' EXIT
git rev-parse --verify HEAD >/dev/null
[[ -z $(git status --porcelain) ]] || { echo "Commit source changes before deploying." >&2; exit 1; }
git archive --format=tar.gz --output="$archive" HEAD
remote=$(ssh "$HOST" 'mktemp /tmp/sn-source.XXXXXX')
[[ "$remote" =~ ^/tmp/sn-source\.[a-zA-Z0-9]+$ ]] || { echo 'Invalid remote temporary path' >&2; exit 1; }
scp -q "$archive" "$HOST:$remote"
ssh "$HOST" "bash -s -- '$SRC' '$remote' '${MIGRATION_BASELINE:-}'" <<'REMOTE'
set -euo pipefail
source_dir=$1
archive=$2
baseline=$3
trap 'rm -f "$archive"' EXIT
[[ -f "$source_dir/.env" ]] || { echo 'Install the panel first; .env is missing' >&2; exit 1; }
umask 077
backup="$source_dir/backups/source-$(date -u +%Y%m%dT%H%M%SZ).tgz"
mkdir -p "$source_dir/backups"
tar czf "$backup" -C "$source_dir" Cargo.toml Cargo.lock crates web db deploy install.sh
tar xzf "$archive" -C "$source_dir"
export MIGRATION_BASELINE="$baseline"
bash "$source_dir/update.sh"
REMOTE

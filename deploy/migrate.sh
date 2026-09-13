#!/usr/bin/env bash
# Apply each migration once. DATABASE_URL is read from the caller's environment.
set -euo pipefail
: "${DATABASE_URL:?DATABASE_URL is required}"
SN_MIGRATIONS_DIR=${SN_MIGRATIONS_DIR:-$(cd "$(dirname "$0")/../db/migrations" && pwd)}
if [[ ${SN_PG_ENV_READY:-} != 1 ]]; then
  exec python3 "$(dirname "$0")/pg-env.py" bash "$0" "$@"
fi
PSQL=(psql -X -q -v ON_ERROR_STOP=1)
existing=$("${PSQL[@]}" -Atc "SELECT to_regclass('public.clients') IS NOT NULL AND to_regclass('public.sn_schema_migrations') IS NULL")
if [[ "$existing" == t && -z "${MIGRATION_BASELINE:-}" ]]; then
  echo 'Existing schema has no migration history. Set MIGRATION_BASELINE to the last verified applied migration number before upgrading.' >&2
  exit 1
fi
if [[ -n "${MIGRATION_BASELINE:-}" && ! "$MIGRATION_BASELINE" =~ ^[0-9]{1,3}$ ]]; then echo 'Invalid MIGRATION_BASELINE' >&2; exit 1; fi
"${PSQL[@]}" -c 'CREATE TABLE IF NOT EXISTS sn_schema_migrations(name text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())'
for file in "$SN_MIGRATIONS_DIR"/[0-9]*.sql; do
  name=$(basename "$file")
  [[ "$name" =~ ^[0-9]{3}_[a-zA-Z0-9_]+\.sql$ ]] || { echo "Invalid migration filename: $name" >&2; exit 1; }
  number=${name%%_*}
  if [[ -n "${MIGRATION_BASELINE:-}" ]] && (( 10#$number <= 10#$MIGRATION_BASELINE )); then
    "${PSQL[@]}" -c "INSERT INTO sn_schema_migrations(name) VALUES('$name') ON CONFLICT DO NOTHING"
    continue
  fi
  {
    printf "SELECT pg_advisory_xact_lock(hashtext('stealthnet.schema'));\n"
    printf "SELECT NOT EXISTS(SELECT 1 FROM sn_schema_migrations WHERE name='%s') AS apply_migration \\gset\n" "$name"
    printf '\\if :apply_migration\n'
    cat "$file"
    printf "\nINSERT INTO sn_schema_migrations(name) VALUES('%s');\n" "$name"
    printf '\\endif\n'
  } | "${PSQL[@]}" --single-transaction >/dev/null
  echo "Schema checked: $name"
done

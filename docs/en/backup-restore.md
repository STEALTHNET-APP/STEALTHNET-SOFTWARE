[All guides](README.md) · [Русский](../ru/backup-restore.md) / [English](../en/backup-restore.md)

# Backup and restore

Recovery needs **the database and its matching .env file**. Without `CABINET_CODE_KEY`, stored access codes cannot be decrypted. Keep proxy configuration and separate-service environments as well.

## Back up a standard installation

Run as root on the server with the local `stealthnet` database:

```bash
umask 077
sn_backup_dir="/root/stealthnet-backup-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir "$sn_backup_dir"
runuser -u postgres -- pg_dump -Fc stealthnet > "$sn_backup_dir/database.dump"
cp /opt/stealthnet-software/.env "$sn_backup_dir/panel.env"
cp /opt/stealthnet-software/installation.json "$sn_backup_dir/installation.json"
cp -a /etc/caddy "$sn_backup_dir/caddy"
pg_restore --list "$sn_backup_dir/database.dump" > "$sn_backup_dir/contents.txt"
```

Store a protected copy off-server and rehearse a restore into an isolated database. External PostgreSQL needs its own backup procedure matching DATABASE_URL; do not pass passwords as command arguments. Separately back up `/etc/sn-cabinet/env`, `/etc/sn-sub/env` and `/etc/sn-node/node.env` on their servers.

## Restore a standard installation

Below, `/root/stealthnet-restore` contains the **selected verified backup**, not automatically the newest one. This replaces current data. Use a maintenance window and make an additional backup of the current state first.

1. Inspect `pg_restore --list` and PostgreSQL compatibility. Install the application release matching the backup.
2. Stop services, restore the database and matching environment:

```bash
set -e
systemctl stop sn-api sn-sub sn-worker sn-bot
runuser -u postgres -- pg_restore --exit-on-error --clean --if-exists --no-owner --role=stealthnet -d stealthnet < /root/stealthnet-restore/database.dump
install -m 600 /root/stealthnet-restore/panel.env /opt/stealthnet-software/.env
systemctl start sn-api sn-sub sn-worker
```

3. Start sn-bot only when its token is configured. Run `stealthnet doctor`; test administrator login, customer code, subscription delivery and a real VPN connection.
4. Restore Caddy selectively after comparing domains; run `caddy validate` before reloading. Preserve sites created after the backup.

`make update` saves the database and environment automatically before upgrading. Returning to previous binaries after a failure does not reverse applied migrations or replace a restore rehearsal.

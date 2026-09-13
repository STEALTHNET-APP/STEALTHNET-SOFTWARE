[All guides](README.md) · [Русский](../ru/installation.md) / [English](../en/installation.md)

# Install on a clean server

The installer downloads prebuilt GitHub release files, installs PostgreSQL, Caddy and the panel services, and creates the owner account. Rust, Git, Node.js and Docker are not required on the customer server.

> The public installation command requires published source code and a stable release in STEALTHNET-APP/STEALTHNET-SOFTWARE. The first public release is still pending. A draft release is not installable by customers.

## Install from GitHub

SSH into a clean Debian/Ubuntu server **as root**, then run:

```bash
apt-get update
apt-get install -y git curl ca-certificates
git clone --branch v0.1.1 --depth 1 https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE.git /root/stealthnet-installer
cd /root/stealthnet-installer
bash install.sh --version v0.1.1
```

The wizard asks for panel/subscription domains, service name, currency and owner credentials. It downloads the release for your server architecture, verifies SHA256, and installs PostgreSQL, system services and HTTPS. You do not need to compile Rust.

The repository is cloned into `/root/stealthnet-installer`; the running panel is installed in `/opt/stealthnet-software`. Use `make update` from that installation directory for subsequent panel updates.

## Requirements

- Debian 12/13 or Ubuntu 22.04/24.04/26.04 LTS with systemd; amd64 or arm64.
- Recommended: 2 vCPU, 2 GB RAM and 10 GB free disk. The installer rejects less than 1 GB RAM or 3 GB free disk.
- Root SSH access to a clean server. Existing databases and installations are not silently replaced.
- Separate panel and subscription domains, for example panel.example.com and sub.example.com. Point A records at the server; any AAAA records must also point at a working address on this server.
- Allow TCP 80/443 in the hosting firewall and preserve SSH access. PostgreSQL and application ports remain on loopback. Active UFW receives only the required HTTP/HTTPS rules.

The customer website/Mini App and VPN nodes are installed separately from the admin panel after this setup.

## Download and run

Run as root after the first stable release is published:

```bash
apt-get update -qq
apt-get install -y --no-install-recommends curl ca-certificates
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/STEALTHNET-APP/STEALTHNET-SOFTWARE/main/install.sh -o /root/stealthnet-install.sh
bash /root/stealthnet-install.sh
```

The wizard requests domains, service name, currency, owner username/password and an optional Telegram token. Leaving the password empty generates one. Hidden inputs and private configuration files keep secrets out of command arguments. The command-line wizard currently uses Russian prompts; this guide provides the English setup instructions.

The downloader selects the architecture and stable tag, verifies the HTTPS archive and SHA256, then validates the per-file manifest. An incomplete release, wrong architecture or checksum mismatch stops installation. The installer creates the database and applies ordered migrations. It does not add demo customers, plans or payment keys.

Caddy comes from the signed official stable repository when no executable is installed. Other Caddy sites are retained through an imported configuration file. An unrelated server occupying the required ports is not stopped automatically.

Success requires a working database, API, subscription readiness, systemd services and both public HTTPS domains with valid certificates. Owner credentials are written to `/root/stealthnet-access.txt` with permissions 600. Move them into a password manager, then remove that file.

## Manage the installation

```bash
stealthnet doctor
stealthnet status
stealthnet logs api
stealthnet logs sub
stealthnet bot-token
stealthnet admin-password
```

`doctor` checks release integrity, local services, database and HTTPS. `bot-token` changes the bot token through hidden input. `admin-password` changes an existing administrator password. The bot stays disabled until a token is configured.

Continue with [First setup](sections/getting-started.md): profile → node → host → internal squad → plan → payments → customer test. Configure the customer website through [its installation guide](cabinet-installation.md).

## Update

```bash
cd /opt/stealthnet-software
make update
```

Or run `stealthnet update` from any directory. To select a particular **published** tag:

```bash
make update VERSION=v0.1.1
```

The tag above is a syntax example, not a claim that this release exists. The updater downloads and verifies files, saves a PostgreSQL dump and configuration, applies migrations, atomically switches `current`, restarts panel services and checks readiness. Nodes/Xray, a separate subscription service and the customer website have their own update procedures.

Backups are stored in `/opt/stealthnet-software/backups/<UTC timestamp>/`. A startup failure returns to the previous binaries; **applied database migrations are not automatically reversed**. Restoring a database is a separate maintenance operation and discards changes after the backup. Release migrations must remain backward-compatible with the previous binaries.

The version button in the admin header shows build metadata and checks stable GitHub releases. Results are cached for 15 minutes, errors for one minute. No releases, newer local build and network failure have distinct states.

## Interrupted setup

Inspect `/var/log/stealthnet/install-<timestamp>.log` and `journalctl -u sn-api -n 100 --no-pager`. Fix the cause and repeat the same command. Pending configuration preserves the original keys and database settings. Do not delete `.env` or `.install-pending.json` to restart blindly.

| Problem | Check |
|---|---|
| HTTP 404 / no release | Published stable release, exact tag and both architecture assets |
| SHA256 mismatch | Download again; inspect release assets if it persists; never disable verification |
| HTTPS failure | A/AAAA records, public 80/443, Caddy logs and ACME rate limits |
| Port occupied | Existing proxy or application; the installer does not stop it automatically |
| Existing role/database | Existing installation or partial setup; its password is not automatically replaced |
| APT locked | Wait for unattended upgrades; APT waits up to 180 seconds for its lock |

## Existing reverse proxy and unattended setup

Use a root-owned JSON file with permissions 600. Required fields: `panel_domain`, `sub_domain`, `brand`, `currency`, `admin_user`, `admin_password`. Optional: `bot_token` and `proxy` (`caddy` or `external`). Password length: 12–128 characters.

```bash
chmod 600 /root/install.json
bash /root/stealthnet-install.sh --config /root/install.json
```

With `proxy: "external"`, the installer leaves your proxy/firewall alone, checks local services and writes `/opt/stealthnet-software/Caddyfile.example`. You configure and verify public HTTPS. Serve `current/web`, proxy `/api/*` to `127.0.0.1:8080` and the subscription domain to `127.0.0.1:8081`. The panel’s `/app*` path must serve the Mini App unavailable page. Use the generated Caddy example for the complete path and header rules.

If GitHub is inaccessible, transfer the published archive and checksum over a trusted channel, verify the external SHA256, safely extract it and run `python3 <release>/deploy/installer.py install --release-dir <release> --config /root/install.json`. The installer also verifies its internal manifest.

## Important paths

| Path | Purpose |
|---|---|
| `/opt/stealthnet-software/.env` | Database and service secrets, code-encryption key; mode 600 |
| `/opt/stealthnet-software/releases/` | Immutable release directories |
| `/opt/stealthnet-software/current` | Active release symlink |
| `/var/lib/stealthnet` | Service working directory |
| `/etc/caddy/stealthnet/panel.caddy` | Panel/subscription domains |
| `/var/log/stealthnet` | Private installer logs |

Back up `.env`, especially `CABINET_CODE_KEY`, with the database. Older source-based deployments continue through `deploy/update-source.sh`; running an update does not silently convert them to the release-directory layout.

## What updates preserve

`make update` changes release code and applies new migrations once. Customers, subscriptions, payments, plans, profiles, branding and texts live in PostgreSQL; updates do not run `db/seed.sql` or recreate the owner. The `.env` file and `CABINET_CODE_KEY` remain unchanged. Updates do not rewrite the existing reverse proxy configuration or systemd customizations.

Store your images in `/opt/stealthnet-software/shared/public/`, for example `logo.svg`. With the standard Caddy installation, use `https://panel.example.com/custom/logo.svg` in admin branding settings. Releases never replace `shared/`, and the update backup includes it. This directory is public: never put keys or configuration files there. Configure an equivalent `/custom/` route when using an external proxy.

`releases/` and `current/web` contain versioned application code; do not put operator files or manual edits there. Editing release source/CSS is different from changing branding settings. Previous releases stay on disk; failed readiness returns to the previous binaries. Reverting binaries does not undo applied migrations; data rollback uses the saved database dump.

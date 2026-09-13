[All guides](README.md) · [Русский](../ru/installation.md) / [English](../en/installation.md)

# Install on a clean server

The installer downloads prebuilt GitHub release files, installs PostgreSQL, Caddy and the selected local services, and creates the owner account. Rust, Git, Node.js and Docker are not required on the customer server.

The commands below install a published stable release from STEALTHNET-APP/STEALTHNET-SOFTWARE. Without `--version`, the downloader selects the latest stable release automatically. Draft releases are not available to customers.

## Install from GitHub

SSH into a clean Debian/Ubuntu server **as root**, then run:

```bash
apt-get update
apt-get install -y git curl ca-certificates
git clone --branch v0.1.8 --depth 1 https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE.git /root/stealthnet-installer
cd /root/stealthnet-installer
bash install.sh --version v0.1.8
```

The wizard asks for panel/subscription domains, service name, currency and owner credentials. It downloads the release for your server architecture, verifies SHA256, and installs PostgreSQL, system services and HTTPS. You do not need to compile Rust.

The repository is cloned into `/root/stealthnet-installer`; the running panel is installed in `/opt/stealthnet-software`. Use `make update` from that installation directory for subsequent panel updates.

## Hosting choice and field hints

The wizard starts with **1 — on this server** or **2 — on a separate server**.

- **On this server:** both domains point to the panel server. Setup installs `sn-sub`, configures its HTTPS and verifies readiness.
- **Separate server:** only the panel domain points to this server. The subscription domain is saved for customer links; its DNS can be configured later on the second server. Setup does not install local `sn-sub`, add its Caddy site or wait for the remote subscription to become ready.

| Field | Example and meaning |
|---|---|
| Panel domain | `panel.example.com` — administrator login; A/AAAA point to the panel server |
| Subscription domain | `sub.example.com` — customer connection address; a different domain pointing to the selected subscription server, without `/s/ID` |
| Service name | `My VPN` — the name customers see |
| Currency | `USD`, `EUR`, `RUB`, `UAH` — a three-letter code for prices and balances, not an amount or `$` symbol. Configure Stars separately |

An origin such as `https://panel.example.com/` is accepted and normalized to its domain. Paths, ports, query parameters and credentials are rejected. Invalid domains or currencies are requested again without restarting the wizard.

For a separate server, finish panel setup, then open **Settings → Subscription service → Installation → Separate server**. Issue a service key and run the command on the second machine. Configure its DNS/HTTPS and check `/ready` plus a real customer link. The remote subscription is not ready until these steps are complete. See [the full guide](subscription-installation.md).

The choice is saved in `installation.json`. Panel updates and `stealthnet doctor` only check locally managed services; update the separate subscription on its own server. Existing installations without this field keep their previous local subscription behavior.

## Requirements

- Debian 12/13 or Ubuntu 22.04/24.04/26.04 LTS with systemd; amd64 or arm64.
- Recommended: 2 vCPU, 2 GB RAM and 10 GB free disk. The installer rejects less than 1 GB RAM or 3 GB free disk.
- Root SSH access to a clean server. Existing databases and installations are not silently replaced.
- Separate panel and subscription domains, for example panel.example.com and sub.example.com. Point A/AAAA records at the server hosting each service. For a separate subscription, its DNS can be configured after panel setup.
- Allow TCP 80/443 in the hosting firewall and preserve SSH access. PostgreSQL and application ports remain on loopback. Active UFW receives only the required HTTP/HTTPS rules.

The customer website/Mini App and VPN nodes are installed separately from the admin panel after this setup.

## Download and run

Run as root to install the latest stable release without cloning the repository:

```bash
apt-get update -qq
apt-get install -y --no-install-recommends curl ca-certificates
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/STEALTHNET-APP/STEALTHNET-SOFTWARE/main/install.sh -o /root/stealthnet-install.sh
bash /root/stealthnet-install.sh
```

The wizard requests domains, service name, currency, owner username/password and an optional Telegram token. Leaving the password empty generates one. Hidden inputs and private configuration files keep secrets out of command arguments. The command-line wizard currently uses Russian prompts; this guide provides the English setup instructions.

The downloader selects the architecture and stable tag, verifies the HTTPS archive and SHA256, then validates the per-file manifest. An incomplete release, wrong architecture or checksum mismatch stops installation. The installer creates the database and applies ordered migrations. It does not add demo customers, plans or payment keys.

Caddy comes from the signed official stable repository when no executable is installed. Other Caddy sites are retained through an imported configuration file. An unrelated server occupying the required ports is not stopped automatically.

Success requires a working database, API, locally managed systemd services and the panel HTTPS domain with a valid certificate. Local placement also checks subscription readiness and its HTTPS domain. Separate subscription setup is completed and verified on the second server. Owner credentials are written to `/root/stealthnet-access.txt` with permissions 600. Move them into a password manager, then remove that file.

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
make update VERSION=v0.1.8
```

The example pins the published release used in this guide. `make update` without a version selects the latest stable release. The updater downloads and verifies files, saves a PostgreSQL dump and configuration, applies migrations, atomically switches `current`, restarts panel services and checks readiness. Nodes/Xray, a separate subscription service and the customer website have their own update procedures.

Backups are stored in `/opt/stealthnet-software/backups/<UTC timestamp>/`. A startup failure returns to the previous binaries; **applied database migrations are not automatically reversed**. Restoring a database is a separate maintenance operation and discards changes after the backup. Release migrations must remain backward-compatible with the previous binaries.

The version button in the admin header shows build metadata and checks stable GitHub releases. Results are cached for 15 minutes, errors for one minute. No releases, newer local build and network failure have distinct states.

## Interrupted setup

### The panel opens without styling

Releases before **0.1.7** could match `/app.css` with Caddy's `/app*` route and return the Mini App placeholder. Browsers reported `MIME type ('text/html') is not a supported stylesheet`. Update the installed panel as root:

```bash
cd /opt/stealthnet-software
make update VERSION=v0.1.8
```

The update creates a backup and repairs the legacy rule in installer-managed configuration while preserving other settings. Afterwards, reload the browser without cached content (`Ctrl+F5` / `Cmd+Shift+R`). For a custom reverse proxy, match `/app` and `/app/*` without intercepting `/app.css`. New installations and updates check the main CSS and JavaScript content types and contents before reporting success. The login page displays the API version without a staging label.

### Diagnose incomplete setup

Inspect `/var/log/stealthnet/install-<timestamp>.log` and `journalctl -u sn-api -n 100 --no-pager`. Fix the cause and repeat the same command. Pending configuration preserves the original keys and database settings. Do not delete `.env` or `.install-pending.json` to restart blindly.

| Problem | Check |
|---|---|
| HTTP 404 / no release | Published stable release, exact tag and both architecture assets |
| SHA256 mismatch | Download again; inspect release assets if it persists; never disable verification |
| HTTPS failure | A/AAAA records, public 80/443, Caddy logs and ACME rate limits |
| Port occupied | Existing proxy or application; the installer does not stop it automatically |
| Existing role/database | Existing installation or partial setup; its password is not automatically replaced |
| APT locked | Wait for unattended upgrades; APT waits up to 180 seconds for its lock |

### The final step stopped at HTTPS / `URLError`

If the error follows the HTTPS and certificate check message, the local API, database and subscription service have already passed their checks. Older installers show only `URLError`: this is a general connection error, not proof of a certificate problem.

Run on the server as root:

```bash
systemctl is-active caddy sn-api sn-sub
ss -lntp | grep -E ':(80|443|8080|8081)\b'
curl --connect-timeout 5 --max-time 10 -sS http://127.0.0.1:8080/api/health
journalctl -u caddy -n 50 --no-pager
```

Compare the panel DNS with its server IP. With local placement, the subscription domain must point there too. With separate placement, verify subscription DNS on its own server. An AAAA record requires working IPv6. Check TCP 80/443 in both the server firewall and the provider's network rules; application ports 8080/8081 should stay private.

- `Connection refused`: no accessible listener at the selected IP and port, or a firewall actively rejects the connection. Check the address, Caddy status and listening ports.
- `Timeout`: check the address, routing and dropped traffic.
- Certificate error: check Caddy logs, DNS, ACME port access and the server clock. Keep TLS verification enabled.
- HTTP 502: the web server is accessible but cannot obtain a valid application response; check the local API and reverse proxy route.

After fixing the cause, **repeat the installation command for the same version**. Setup stopped at step 7 is incomplete; `make update` cannot finish it. Keep `.env`, `.install-pending.json` and the database: the next installation run reuses them.

## Existing reverse proxy and unattended setup

Use a root-owned JSON file with permissions 600. Required fields: `panel_domain`, `sub_domain`, `brand`, `currency`, `admin_user`, `admin_password`. Optional: `bot_token`, `proxy` (`caddy` or `external`) and `subscription_placement` (`local` by default or `remote`). The subscription domain is required in both modes, but remote mode accepts a future domain without DNS yet. Proxy management and subscription placement are independent choices. Password length: 12–128 characters.

```bash
chmod 600 /root/install.json
bash /root/stealthnet-install.sh --config /root/install.json
```

With `proxy: "external"`, the installer leaves your proxy/firewall alone, checks local services and writes `/opt/stealthnet-software/Caddyfile.example`. You configure and verify public HTTPS. Serve `current/web`, proxy `/api/*` to `127.0.0.1:8080` and, for local subscription placement only, the subscription domain to `127.0.0.1:8081`. The panel’s `/app` and `/app/*` paths must serve the Mini App unavailable page. Use the generated Caddy example for the complete path and header rules.

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

## Owner and teammates

- [Admin profile](sections/admin-profile.md)
- [First owner and team](sections/team.md)

## “No terminal” before the setup questions

Older installers opened the SSH terminal as a seekable file. Version **0.1.6** fixes this. If setup stopped immediately after SHA256 verification with “Нет терминала”, run the downloader again without pinning the old version. Normal interactive setup does not require an `install.json` file; the database and owner account had not yet been created at that stage.

For an SSH command launched without a terminal, open an interactive session with `ssh -t root@SERVER_IP` or use the unattended `--config` option described above.

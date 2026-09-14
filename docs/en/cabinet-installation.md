[All guides](README.md) · [Русский](../ru/cabinet-installation.md) / [English](../en/cabinet-installation.md)

# Install the customer website and Mini App

The customer gateway (`sn-cabinet`) is a separate service on the panel server or another server. It serves the storefront, account and `/app/` Mini App through a restricted panel API. It does not need PostgreSQL credentials.

## Installation steps

1. Open **Customer website & Mini App**. Starter RU/EN text, SEO, colors, and a standard logo are prefilled in empty fields. Edit them if needed, enable **Publish portal and allow Mini App**, and click **Save settings**.
2. Create an installation with name, HTTPS domain, IP and hosting mode. Its domain must differ from the panel and subscription domain even on one server.
3. Point A/AAAA records at the selected server, allow TCP 80/443 and preserve SSH access.
4. Copy the generated command and run it as root on the destination server. The key belongs to this installation and remains in its server environment.
5. Wait for binary/checksum, local readiness and public HTTPS checks. Verify the installation reports contact in the admin panel.
6. Select it for Mini App in bot settings. Mini App is unavailable without the customer gateway.
7. Test the public storefront, registration, code saving/viewing, sign-in from another browser and Telegram linking. Then test plans, add-ons, payments and support.

Starter content fills empty form fields and is saved only when you click **Save settings**. Existing text, links, colors, and feature switches are preserved. The logo and favicon are served by your panel domain; prices come from configured plans.

## Same server

The installer adds a separate Caddy site through `/etc/caddy/customer-sites/cabinet.caddy`. The existing panel keeps its domain. If another proxy occupies the ports, use external-proxy mode rather than stopping it blindly.

## Separate server

Use Debian/Ubuntu, systemd, amd64/arm64 and outbound HTTPS to the panel. Do not copy the database password. The service binds to `127.0.0.1:8090`; publish its HTTPS domain through a reverse proxy.

## Updates

When updating a release-based installation to v0.2.5 or later, a standard customer portal installed on the same server for this panel is updated with it:

```bash
cd /opt/stealthnet-software && make update
```

No separate portal command is needed in this setup. The updater preserves the key and settings, checks service readiness, and restores the previous binaries on failure. A disabled and stopped service is not started automatically. Running the command again also updates a stale portal binary when the panel is already current.

A portal **on another server** is updated on that server after the panel:

```bash
/usr/local/sbin/update-cabinet
```

This separate command is also available for custom layouts and panels installed from source. It updates the portal and Mini App. The subscription page and client app list belong to the subscription service.

## Environment and maintenance

Secrets are in `/etc/sn-cabinet/env`, permissions 600. The systemd service is `sn-cabinet`. Check its status:

```bash
systemctl status sn-cabinet --no-pager
journalctl -u sn-cabinet -n 80 --no-pager
curl -fsS http://127.0.0.1:8090/ready
```

Also check the public domain’s `/ready` after an update. If an installation or key is revoked, update its environment using the new panel instructions. Key revocation does not delete customer purchases.

For existing nginx or a container proxy, use `CABINET_PROXY=external` and configure HTTPS yourself. Detailed proxy and recovery steps are in the [bundled installation page](../../web/cabinet-installation-en.html).

[Branding and translations](branding.md) · [Customer website](sections/cabinet.md) · [Add-ons](addons.md)

## Revoked keys and publication settings

If a server card says **Key revoked**, click **Restore key and command**. Recovery is available even when publication is disabled. Before running the command, enable publication and save settings; the command dialog links to the required switch.

Revoking a key also deselects that server for Mini App. An old reference to a revoked or deleted server no longer blocks saving publication, text, or branding. After restoring and starting the portal, select an available Mini App server again. Revoking a different server preserves the current selection.

If an older version reports “Кабинет не найден или его ключ отозван” (portal not found or key revoked), update the panel and reload the page. Portal data is preserved; no SQL publication switch or panel reinstall is needed.

## HTTP 400 at the first installation step

“Сайт ещё не опубликован” means that customer portal publication is disabled in the panel. This applies both beside the panel and on a separate server.

Open **Customer portal and Mini App → Branding and content**, complete the required fields, enable **Publish portal and allow Mini App**, and click **Save settings**. Wait for a successful save, then repeat **the same SSH command**. Checking the box without saving is not enough. You do not need a new key or a panel reinstall: the portal service has not been installed at this step.

HTTP 401/403 indicates denied access: check the key and obtain the current command from **Servers and installation**. Other HTTP 400 responses or HTML instead of JSON may come from the reverse proxy; check its rules and the `sn-api` log.

[All guides](README.md) · [Русский](../ru/cabinet-installation.md) / [English](../en/cabinet-installation.md)

# Install the customer website and Mini App

The customer gateway (`sn-cabinet`) is a separate service on the panel server or another server. It serves the storefront, account and `/app/` Mini App through a restricted panel API. It does not need PostgreSQL credentials.

## Installation steps

1. Open **Customer website & Mini App**. Set branding, SEO, RU/EN content, plan descriptions and instructions. Publish your own content.
2. Create an installation with name, HTTPS domain, IP and hosting mode. Its domain must differ from the panel and subscription domain even on one server.
3. Point A/AAAA records at the selected server, allow TCP 80/443 and preserve SSH access.
4. Copy the generated command and run it as root on the destination server. The key belongs to this installation and remains in its server environment.
5. Wait for binary/checksum, local readiness and public HTTPS checks. Verify the installation reports contact in the admin panel.
6. Select it for Mini App in bot settings. Mini App is unavailable without the customer gateway.
7. Test the public storefront, registration, code saving/viewing, sign-in from another browser and Telegram linking. Then test plans, add-ons, payments and support.

## Same server

The installer adds a separate Caddy site through `/etc/caddy/customer-sites/cabinet.caddy`. The existing panel keeps its domain. If another proxy occupies the ports, use external-proxy mode rather than stopping it blindly.

## Separate server

Use Debian/Ubuntu, systemd, amd64/arm64 and outbound HTTPS to the panel. Do not copy the database password. The service binds to `127.0.0.1:8090`; publish its HTTPS domain through a reverse proxy.

## Environment and maintenance

Secrets are in `/etc/sn-cabinet/env`, permissions 600. The systemd service is `sn-cabinet`. Setup installs this update command:

```bash
/usr/local/sbin/update-cabinet
systemctl status sn-cabinet --no-pager
journalctl -u sn-cabinet -n 80 --no-pager
curl -fsS http://127.0.0.1:8090/ready
```

Also check the public domain’s `/ready` after an update. If an installation or key is revoked, update its environment using the new panel instructions. Key revocation does not delete customer purchases.

For existing nginx or a container proxy, use `CABINET_PROXY=external` and configure HTTPS yourself. Detailed proxy and recovery steps are in the [bundled installation page](../../web/cabinet-installation-en.html).

[Branding and translations](branding.md) · [Customer website](sections/cabinet.md) · [Add-ons](addons.md)

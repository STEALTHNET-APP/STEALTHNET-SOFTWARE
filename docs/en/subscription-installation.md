[All guides](README.md) · [Русский](../ru/subscription-installation.md) / [English](../en/subscription-installation.md)

# Install the subscription page

`sn-sub` serves both the connection page and client configurations at `/s/ID`. A browser receives instructions and apps; a VPN client receives the selected configuration format. Purchases take place in the bot and customer website/Mini App.

## Choose hosting

| | With the panel | Separate server |
|---|---|---|
| Data source | Local PostgreSQL, `SUB_MODE=db` | Panel HTTPS API, `SUB_MODE=api` |
| Installation | Select “On this server” during panel setup | Settings → Subscription service |
| Service key | Not required | Required |
| Database exposure | Local only | No database credentials or open database port |

Use different domains for panel, subscription service and customer website. Configure full HTTPS origins under **Settings → Subscription service**, without `/s/ID`, credentials or query parameters.

## On the panel server

When “On this server” was selected, the panel installer already installs `sn-sub`. Do not run the separate-server installer over it.

```bash
systemctl status sn-sub --no-pager
curl -fsS http://127.0.0.1:8081/ready
journalctl -u sn-sub -n 50 --no-pager
```

Expect `{"status":"ready"}`. Its systemd environment uses the panel’s `.env`: `SUB_MODE=db`, a valid `DATABASE_URL`, `SUB_BIND=127.0.0.1:8081` and the public subscription URL. Restart only sn-sub after changing its environment.

For an existing Compose deployment, use `docker compose up -d sub` and the in-container readiness check. Caddy in the same network connects to `sub:8081`, not its own `127.0.0.1`.

## On a separate server

During new panel setup, select “On a separate server” and enter the future subscription domain. Point its DNS at the second server when ready; panel setup does not check that domain or install a local subscription service.

1. Prepare Debian/Ubuntu, systemd, root SSH and amd64/arm64. Permit outbound HTTPS to the panel.
2. Save the public domains in the panel. Issue and securely retain the service key. Viewing instructions does not require rotating an existing working key.
3. Select **Separate server → Installation**. Run the complete generated command on the target server.
4. The installer validates the key, API response, architecture, checksum and executable, then creates `/etc/sn-sub/env` (600) and a systemd service.
5. Verify local `/ready`, then configure DNS and the reverse proxy. A local success does not certify public HTTPS.

Release packages include binaries and SHA256 files for both architectures. Missing artifacts or an HTML response stop installation. A custom build may use `SUB_BINARY_URL`; Docker mode requires a configured real `SUB_IMAGE` and is not required for shell setup.

## Public HTTPS

Point the subscription domain at its server and allow TCP 80/443. In Caddy, replace the example domain with your own:

```caddyfile
sub.example.com {
    reverse_proxy 127.0.0.1:8081
}
```

Validate and reload the existing configuration; avoid duplicate domain blocks:

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
curl -fsS https://sub.example.com/ready
```

## Keys, updates and verification

The service key stays in `/etc/sn-sub/env`, not a publicly readable unit file. Separate instances share the project’s active key. Rotating it requires updating `SUB_SERVICE_TOKEN` on every separate instance and restarting sn-sub. Never disable HTTPS certificate verification.

Use the update command in the installation instructions for the appropriate deployment. Check readiness after updating. Do not replace a systemd service with Compose instructions accidentally.

Finally, open a real active customer’s subscription link in a browser and import it into the intended VPN app. Test platform selection, install/download action, deep-link import, copy/QR, then a real connection. `/health` checks the process; `/ready` checks its data source. Neither proves a VPN tunnel works.

Brand logos, favicon and colors are shared with [Customer website & Mini App](sections/cabinet.md). Edit app links and platform instructions in [Subscription page](sections/subpage.md).

<p align="center"><a href="README.ru.md">Русский</a> · <a href="README.md"><b>English</b></a></p>

![STEALTHNET — your VPN service](docs/media/cover-en.svg)

<p align="center">
  <a href="docs/en/installation.md"><img src="docs/media/navigation/button-install-en.svg" width="180" alt="Install" /></a>
  <a href="docs/en/README.md"><img src="docs/media/navigation/button-docs-en.svg" width="180" alt="Documentation" /></a>
  <a href="https://t.me/stealthnet_admin_panel"><img src="docs/media/navigation/button-community-en.svg" width="180" alt="Community" /></a>
  <a href="#support-project"><img src="docs/media/navigation/button-donate-en.svg" width="180" alt="Donate" /></a>
</p>

**STEALTHNET** brings VPN infrastructure, subscriptions, sales and customer accounts into one platform. Built with Rust and PostgreSQL, with independent services and Russian/English interfaces.

**v0.1.0 · first public release.** [Download Linux builds](https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE/releases/tag/v0.1.0) · [Compatibility and test scope](docs/compatibility.md).

## One service, three interfaces

| Owner control panel | Customer website & Mini App | Infrastructure |
|---|---|---|
| Customers, plans, payments, support and alerts | Storefront, code sign-in, purchases, devices and traffic | Profiles, nodes, hosts, squads and subscriptions |
| Versions, diagnostics, installation and branding | RU/EN, light/dark themes, linked Telegram account | Xray agent, access control and bgp.tools information |

## Interface gallery

Click a screenshot to open it at full size.

<table>
<tr>
<td width="50%" valign="top"><h3>Owner control panel</h3><a href="docs/media/panel-nodes-en.png"><img src="docs/media/panel-nodes-en.png" width="640" alt="Owner control panel" /></a><p>Nodes, diagnostics and infrastructure</p></td>
<td width="50%" valign="top"><h3>Public website</h3><a href="docs/media/storefront-en.png"><img src="docs/media/storefront-en.png" width="640" alt="Public website" /></a><p>Service information and plans before sign-in</p></td>
</tr>
<tr>
<td width="50%" valign="top"><h3>Customer account · Desktop</h3><a href="docs/media/cabinet-en.png"><img src="docs/media/cabinet-en.png" width="640" alt="Desktop customer account" /></a><p>Subscription, purchases, devices and traffic</p></td>
<td width="50%" valign="top" align="center"><h3>Mini App · Telegram</h3><a href="docs/media/miniapp-en.png"><img src="docs/media/miniapp-en.png" width="160" alt="STEALTHNET Mini App" /></a><p>The same account and subscription inside Telegram</p></td>
</tr>
</table>

[About the screenshots](docs/media/README.md): actual interfaces in a test environment with demonstration data.

## Included

- **Sales:** plans/periods, promo codes and referrals; Platega, RollyPay, ParityPay v2, Stars, CryptoBot and manual payments.
- **Add-ons:** packages and prices per plan. Device slots last until the paid term ends; traffic lasts until reset or expiry.
- **Access:** internal squads grant inbounds; an external squad overrides subscription templates and settings.
- **Connection:** subscription page, app instructions and QR; Xray JSON, share-links, Clash/Mihomo/Stash and sing-box output.
- **Customer website:** public storefront, persistent access code, Telegram linking, purchases and support. The website installs separately and hosts Mini App.
- **Branding:** light/dark logos, favicon, colors, SEO, RU/EN content, FAQ and instructions configured in the admin panel.
- **Operations:** release installer, `make update`, pre-update backup, version checks, logs and team alerts.

## Installation

Supported targets: **Debian 12/13**, **Ubuntu 22.04/24.04/26.04 LTS**, **amd64/arm64**. Fresh installation and update were exercised on Debian 13, Ubuntu 24.04 and 26.04 amd64. ARM binaries are built; a separate ARM VM run is still pending.

[Open the installation guide](docs/en/installation.md) for requirements, DNS, wizard, HTTPS and diagnostics. [Nodes](docs/en/node-installation.md), [subscriptions](docs/en/subscription-installation.md) and [customer website](docs/en/cabinet-installation.md) have dedicated instructions, including shared/separate servers.

On an installed panel:

```bash
cd /opt/stealthnet-software
make update
```

The updater downloads a published release, verifies SHA256 and creates a backup. Returning to previous binaries does not reverse database migrations. [Backup and restore](docs/en/backup-restore.md).

## Task-based documentation

<table>
<tr>
<td width="50%"><a href="docs/en/installation.md"><img src="docs/media/navigation/install-en.svg" width="640" alt="Installation" /></a></td>
<td width="50%"><a href="docs/en/README.md"><img src="docs/media/navigation/guide-en.svg" width="640" alt="Panel guide" /></a></td>
</tr>
<tr>
<td width="50%"><a href="docs/en/profiles.md"><img src="docs/media/navigation/profiles-en.svg" width="640" alt="Profiles & configs" /></a></td>
<td width="50%"><a href="docs/en/subscription-installation.md"><img src="docs/media/navigation/subscription-en.svg" width="640" alt="Subscription page" /></a></td>
</tr>
<tr>
<td width="50%"><a href="docs/en/cabinet-installation.md"><img src="docs/media/navigation/cabinet-en.svg" width="640" alt="Website & Mini App" /></a></td>
<td width="50%"><a href="docs/en/backup-restore.md"><img src="docs/media/navigation/operations-en.svg" width="640" alt="Updates & backups" /></a></td>
</tr>
</table>

**[All 34 panel sections](docs/en/README.md)** · [Payments](docs/en/payment-gateways.md) · [Add-ons](docs/en/addons.md) · [Branding & languages](docs/en/branding.md) · [Alerts](docs/en/team-notifications.md) · [Troubleshooting](docs/en/troubleshooting.md)

## Architecture

```mermaid
flowchart LR
  Admin[Administrator] --> API[Panel API]
  API --> DB[(PostgreSQL)]
  Bot[Telegram bot] --> DB
  Worker[Background jobs] --> DB
  Site[Website and Mini App] --> Gateway[Customer gateway]
  Gateway --> API
  Node[Node agent / Xray] --> API
  Sub[Subscription service] --> API
  Client[VPN client] --> Sub
  Client --> Node
```

A co-located subscription service can use the local database. Separate customer and subscription gateways in API mode do not need PostgreSQL access.

## Development and license

[Architecture and development](docs/en/development.md) · [Contributing](CONTRIBUTING.md) · [Security reports](SECURITY.md) · [Third-party notices](THIRD_PARTY_NOTICES.md)

Project license: **AGPL-3.0-only**. Full terms: [LICENSE](LICENSE). Dependencies and fonts retain their own licenses.

<a id="support-project"></a>

## Community and project support

[News, discussions and help · @stealthnet_admin_panel](https://t.me/stealthnet_admin_panel)

Optional donations support STEALTHNET development. **Network: TRON · TRC20.**

```text
THQA9Qnx87NcHAwYrcCTBGSi6BhY72LXEZ
```

[All guides](README.md) · [Русский](../ru/troubleshooting.md) / [English](../en/troubleshooting.md)

# Troubleshooting

Start with the specific failing operation and its timestamp. Do not publish .env files, tokens, access codes or full subscription URLs.

| Symptom | Check |
|---|---|
| Panel unavailable | DNS, 80/443, Caddy, sn-api status, `stealthnet doctor` |
| API responds, database fails | PostgreSQL, DATABASE_URL, API logs and free disk |
| Subscription service unavailable | Local/public `/ready`, sn-sub, db/api mode and service key |
| Agent online, VPN fails | Xray health, profile, enabled inbound, port, host and internal squad |
| Location missing | Customer expiry/status/traffic, squad, live node, enabled host and format exclusions |
| Add-ons missing | Packages on the actual plan, eligibility, website purchases and bot button |
| Payment pending | Provider status, HTTPS callback, amount/currency and transaction ID |
| Mini App unavailable | Installed gateway, selected installation, `/ready`, key and bot settings |
| Copy failed | HTTPS and clipboard permission; inspect the success/error feedback |
| English content missing | Language selector and English branding/plan fields |
| No update offered | Published stable release, both archives, SHA256 and GitHub check status |

## A useful bug report

Include panel/Xray versions, OS/architecture, page, reproduction steps, expected/actual behavior, timestamp/time zone and a redacted log excerpt. For visual bugs include viewport size, language, theme and a screenshot without personal data. Use the [private-reporting process](../../SECURITY.md) for vulnerabilities.

API availability, subscription delivery and VPN connectivity are separate checks. After a fix, repeat the original scenario and its adjacent outcome: after payment, verify both subscription expiry and actual connectivity.

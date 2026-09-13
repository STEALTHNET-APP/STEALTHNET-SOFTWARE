# Selfsteal: your own website on a node

[Documentation](README.md) · [Profiles](profiles.md) · [Node installation](node-installation.md)

Selfsteal serves a real mini-site from the same node and domain as a VLESS REALITY connection. Visitors opening the domain in a browser see the website. VPN clients use the same public TCP port 443.

## Before you start

- Update the panel and the node agent to a release that supports Selfsteal. Older agents keep their current configuration and ask for an update.
- Use a native node on Debian 12/13 or Ubuntu 22.04/24.04/26.04 with systemd. The managed website does not run inside the Docker node image.
- Use a separate profile and domain for each node. Point every A/AAAA record for that domain to this node; remove an incorrect AAAA record if IPv6 is unavailable.
- Allow inbound TCP 80 and 443 in the server and provider firewalls. In Cloudflare, use **DNS only**. The panel domain and subscription domain belong to their own services.
- TCP 80 and local `127.0.0.1:9443` must be available for the website. An existing web service is never stopped or replaced automatically.

## Create the profile

1. Open **Profiles → New profile** and choose **VLESS REALITY · own node website**. You can also enable **Own website on the node · Selfsteal** for an existing VLESS REALITY TCP/RAW connection in the parameter editor.
2. Choose a mini-site and open **Preview** to inspect it on desktop or mobile. Select its language separately from the panel language.
3. Enter the website domain, for example `nl.example.com`. Optionally change the title and homepage text. Empty text fields use the chosen site's defaults.
4. Validate the profile and save it. Missing REALITY keys are generated; existing keys are preserved.
5. Assign the profile to its node and select its inbound. Add a host with this node's public domain/address and port 443, then grant the test customer access through a squad.
6. Check **Nodes → server → Diagnostics → Website on the node**. Wait for the website and trusted certificate to be ready, then test both the browser URL and an actual VPN client.

The editor sets the REALITY target, SNI and website listener together. You do not need to enter a JSON array or copy a private certificate key into the profile.

## Nine mini-sites

| Design | Included content |
| --- | --- |
| Design studio | Original abstract artwork and design notes |
| Journal | Short, expandable articles |
| Travel | Itinerary ideas and a packing checklist |
| Recipes | Recipes with a servings calculator |
| Text tools | Local word/character counters and text formatting |
| Bookshelf | Searchable reading list |
| Gallery | Original geometric studies |
| Garden | Expandable plant-care notes |
| Clock | Live clock with time-zone selection |

Every template has Russian and English content. These are bundled static sites with small browser-side interactions. They do not need an external service or an account. Template changes, title and text are applied by the node agent on its next synchronization.

## How it runs

```text
Browser or VPN client → node:443 (Xray REALITY)
                              └→ 127.0.0.1:9443 (website HTTPS)
Certificate authority → node:80 (HTTP-01 challenge)
```

The agent downloads a pinned official Caddy binary and verifies SHA256. A separate `sn-selfsteal.service` runs under its own service account. Caddy obtains and renews the certificate using HTTP-01; keep port 80 and DNS working after installation. Xray forwards to Caddy without a PROXY protocol header (`xver: 0`).

The profile stores website metadata in `_selfsteal`. The agent removes this block before writing the Xray configuration. Site files live in `/var/lib/sn-selfsteal/sites/`; certificate storage remains on the node under `/var/lib/sn-selfsteal/data/`. Do not publish or copy certificate keys into Git.

A changed domain is prepared before switching Xray. If preparation fails, the previous engine configuration and active website are retained. A syntax check or a ready certificate alone does not prove that a VPN client can connect.

## Test, change or disable

For **Test-node rehearsal**, use a free node and a separate test domain pointing to it. Publishing the tested configuration keeps the production profile's domain. Never point the production domain at a test node just to run the rehearsal.

To change the design, open the profile parameters, select another mini-site, validate and apply. To stop the managed site, switch the profile back to **External website** and enter that website's domain, or remove the profile from the node. Once the replacement engine configuration is successfully applied, the agent stops its managed web service. Certificate files remain available for reuse.

## Troubleshooting

Run on the node:

```bash
systemctl status sn-node sn-selfsteal --no-pager
journalctl -u sn-selfsteal -n 80 --no-pager
journalctl -u sn-node -n 80 --no-pager
ss -lntp
```

- **Port occupied:** move the existing website or choose a separate node. The agent will not stop unrelated Caddy, Nginx or Apache services.
- **HTTPS not ready:** check all A/AAAA records, DNS-only mode, incoming TCP 80, and outbound HTTPS access. Certificate authorities may limit repeated issuance attempts; do not delete certificate storage to retry.
- **Agent update required:** update the node from the panel's engine/agent controls, then retry synchronization.
- **Website opens, VPN does not:** verify the assigned inbound, host port, REALITY public key/SNI/short ID, squad access and the customer's active subscription.

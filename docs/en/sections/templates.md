[All sections](../README.md) · [Русский](../../ru/sections/templates.md) / [English](../../en/sections/templates.md)

# Custom DNS and routing

Templates define the response wrapper. The system generates server entries with the current customer’s keys.

**Where to find it:** `#/templates` — Subscriptions.

## Workflow

1. Choose a format.
2. create a template from a preset.
3. test with sample servers.
4. assign it as default or in a response rule. The editor includes placeholder help.

## Selection order

Each format has its own default template. A response rule can select a specific template; external-squad and host assignments are then considered. Use placeholder help and a test-customer preview. The system inserts the customer’s servers and secrets.

## Verify the result

Never put another customer’s keys or addresses in a shared template. Valid JSON may still be incompatible with a particular VPN app.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

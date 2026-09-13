[All sections](../README.md) · [Русский](../../ru/sections/node-plugins.md) / [English](../../en/sections/node-plugins.md)

# Node filters and blocking

Built-in agent features include inbound and outbound IP filters, ports, blocking and shared lists.

**Where to find it:** `#/node-plugins` — Infrastructure.

## Workflow

1. Choose a node.
2. check nftables availability and permissions.
3. configure rules.
4. save.
5. wait for the agent to confirm application.

## Applying rules

Check SSH access and management-address exceptions first. Save rules on one node, wait for the agent report and test both allowed and denied connections. If application fails, inspect the error and nftables availability; a saved form does not prove successful application.

## Verify the result

Inbound filters can lock you out of the server. Keep management addresses accessible. Shared lists are referenced as ext:name.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

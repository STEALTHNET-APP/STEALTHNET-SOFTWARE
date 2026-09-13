[All sections](../README.md) · [Русский](../../ru/sections/nodes-metrics.md) / [English](../../en/sections/nodes-metrics.md)

# Current server health

CPU, memory, network speed and last contact come from the agent. They describe the whole server, not the speed of one customer.

**Where to find it:** `#/nodes-metrics` — Infrastructure.

## Workflow

Check measurement freshness. For high load, open the node and compare customer count, memory and traffic.

## Stale measurements

Open the node and compare the last report with the current time. Check outbound HTTPS to the panel and sn-node logs. Interpret CPU and memory alongside customer counts and traffic; these are server measurements, not individual VPN-user measurements.

## Verify the result

Zero activity does not prove a working connection. If metrics are stale, check the agent service and connectivity to the panel.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

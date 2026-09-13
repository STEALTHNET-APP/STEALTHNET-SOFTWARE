[All sections](../README.md) · [Русский](../../ru/sections/torrent-reports.md) / [English](../../en/sections/torrent-reports.md)

# Blocking events

The agent collects rejected connection counters from Xray logs.

**Where to find it:** `#/torrent-reports` — Tools.

## Workflow

Choose a period and inspect the customer to investigate repeated events. Review the profile routing rules.

## Checking why traffic was blocked

Compare event time with profile routing rules and engine logs. The same BLOCK rule may serve several categories. Check which nodes use the profile before changing a blocking rule.

## Verify the result

A blocked destination alone does not prove torrent activity. One block rule may cover multiple traffic categories.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

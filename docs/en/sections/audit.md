[All sections](../README.md) · [Русский](../../ru/sections/audit.md) / [English](../../en/sections/audit.md)

# Change history

The audit log links actions with administrators, objects and timestamps to help investigate changes and incidents.

**Where to find it:** `#/audit` — System.

## Workflow

Choose an action or object and find entries around the time of the problem. Compare them with node and subscription state.

## Investigating a change

Start with the incident time, then filter by object and administrator. Compare the record with current settings. The log does not provide automatic rollback; check later changes by other administrators before restoring anything.

## Verify the result

A missing IP means it was not recorded, not that the request was safe. The audit log is not a backup.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

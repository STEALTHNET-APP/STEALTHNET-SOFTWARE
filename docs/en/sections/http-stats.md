[All sections](../README.md) · [Русский](../../ru/sections/http-stats.md) / [English](../../en/sections/http-stats.md)

# Domain statistics

Optional statistics of customer requests. Disabled by default.

**Where to find it:** `#/http-stats` — Tools.

## Workflow

If needed, enable collection, set retention and verify that new records appear.

## Collection and retention

Set the required retention period and verify worker cleanup. If records are missing, check node support and collection settings. Disabling collection does not instantly delete all existing history.

## Verify the result

This is sensitive activity history. Enabling it should be intentional. Disabling stops new records; retention removes old ones.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

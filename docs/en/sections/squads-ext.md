[All sections](../README.md) · [Русский](../../ru/sections/squads-ext.md) / [English](../../en/sections/squads-ext.md)

# Subscription settings for a group

External squads override templates, routing, headers and host parameters for members. A customer can have one external squad; internal squads grant node access.

**Where to find it:** `#/squads-ext` — Infrastructure.

## Workflow

1. Create a squad.
2. configure overrides.
3. assign it in the customer record. Response rules take priority over external squad templates, followed by host templates or Default.

## Settings inheritance

A customer can have one external squad. Select a template for each needed format, then subscription and host overrides. Squad host overrides affect every returned host. An explicit empty value differs from no override. Verify several client apps, especially when response rules are configured.

## Verify the result

Empty text and disabled switches are explicit override values. Clearing an override restores inheritance. Without an external squad, global settings apply. Partner host export is a separate squad menu action.

## Related guides

[Custom DNS and routing](templates.md) · [Choosing a format by client app](response-rules.md) · [One customer record for access and purchases](users.md)

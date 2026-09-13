[All sections](../README.md) · [Русский](../../ru/sections/config.md) / [English](../../en/sections/config.md)

# Xray configuration editor

Profile and routing presets help build a configuration without rewriting all the JSON.

**Where to find it:** `#/config` — Infrastructure.

## Workflow

1. Choose a preset.
2. fill in parameters.
3. format.
4. validate.
5. save.
6. check the engine on the node.

## Validation failures

Check JSON syntax, unique inbound tags, ports and routing references to existing outboundTag values. Validate the whole document after inserting a snippet. Structural checks are supplemented by xray -test only when an Xray executable exists in the API environment; this does not verify every server.

## Verify the result

Routing rules are evaluated in order; the first match wins. Keep private keys and passwords private. Valid syntax does not replace a real connection test.

## Related guides

[Configurations and inbounds](profiles.md) · [Connecting a VPN server](nodes.md)

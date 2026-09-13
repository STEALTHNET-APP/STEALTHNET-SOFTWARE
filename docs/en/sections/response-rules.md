[All sections](../README.md) · [Русский](../../ru/sections/response-rules.md) / [English](../../en/sections/response-rules.md)

# Choosing a format by client app

Rules match request headers using AND/OR: User-Agent, Accept, operating system and others. The first match selects the format, template, headers and HWID exemption.

**Where to find it:** `#/response-rules` — Subscriptions.

## Workflow

Test a User-Agent. Verify that Happ receives Xray JSON, Clash receives YAML, sing-box receives its JSON, and browsers receive the connection page.

## Testing matches

Test browser and supported client User-Agents. Put narrow rules before broad rules. For an unexpected format, find the request in SRH Inspector, use its headers in the rule tester and compare the selected template.

## Verify the result

An overly broad rule can intercept later rules. Check the User-Agent in the request log and update the client app when troubleshooting.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

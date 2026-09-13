[All sections](../README.md) · [Русский](../../ru/sections/happ-routing.md) / [English](../../en/sections/happ-routing.md)

# Happ routing

A routing profile is sent with the subscription in the routing header. It defines VPN, direct and blocked destinations and DNS.

**Where to find it:** `#/happ-routing` — Subscriptions.

## Workflow

1. Open the builder.
2. enter domains and IPs.
3. check priorities and DNS.
4. apply the profile.
5. refresh the subscription in Happ and reconnect.

## Testing routes

Test one destination for each route type: VPN, direct and blocked. Refresh the subscription and reconnect after changes. For advanced JSON, verify extra fields and link size; express large lists with geosite/geoip where suitable.

## Verify the result

JSON and import preserve advanced fields. Links have an 8 KB limit; use geosite/geoip for large lists. Profile support depends on the Happ version.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

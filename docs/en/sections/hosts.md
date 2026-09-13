[All sections](../README.md) · [Русский](../../ru/sections/hosts.md) / [English](../../en/sections/hosts.md)

# What customers see in their server list

A host is a published location: its name, address, port and profile inbound. A node is the machine running that inbound.

**Where to find it:** `#/hosts` — Infrastructure.

## Workflow

Create a profile and connect a node first. Then add a host, select the inbound, enter the public address and port, and enable the host.

## Checking a published location

The address and port must be reachable by the customer, not only by the panel. Match the host inbound with the node’s enabled inbounds and the customer’s squad. Xray JSON options such as Mux and SockOpt affect applicable formats only. Excluding a subscription format can hide the host in one app while keeping it in another.

## Verify the result

Reality keys come from the profile. Check host bindings after replacing an inbound. Disabling a host hides the location from subscriptions; it does not stop the server port.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

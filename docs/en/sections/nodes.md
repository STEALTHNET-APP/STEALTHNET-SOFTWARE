[All sections](../README.md) · [Русский](../../ru/sections/nodes.md) / [English](../../en/sections/nodes.md)

# Connecting a VPN server

The node agent contacts the panel, downloads its profile and sends metrics. Agent status and Xray health are checked separately.

**Where to find it:** `#/nodes` — Infrastructure.

## Workflow

1. Connect a node.
2. select a profile.
3. copy the installation command.
4. run it on your server.
5. wait for contact.
6. add a host and test the VPN.

## Versions and bgp.tools

The Xray version button opens available releases; test a version on one node first. In the network tab, run bgp.tools lookup for IP, ASN, operator and prefix information. This does not prove VPN connectivity. If the agent responds but Xray does not, inspect diagnostics and sn-node logs, configuration and occupied ports.

## Verify the result

Open the profile inbound port in the firewall. Restarting the engine interrupts connections. Test version updates on one node first.

![Connecting a VPN server](../../media/panel-nodes-en.png)

Interface shown with demonstration data.

## Related guides

[node-installation](../node-installation.md) · [Configurations and inbounds](profiles.md) · [What customers see in their server list](hosts.md)

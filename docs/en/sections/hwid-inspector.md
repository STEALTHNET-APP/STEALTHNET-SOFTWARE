[All sections](../README.md) · [Русский](../../ru/sections/hwid-inspector.md) / [English](../../en/sections/hwid-inspector.md)

# Customer devices

Apps send HWIDs when refreshing subscriptions. One identifier used by multiple customers may warrant investigation.

**Where to find it:** `#/hwid-inspector` — Tools.

## Workflow

Find a device by model, platform, customer or HWID. Open its record before removing it.

## Freeing a slot

Open the device and customer, then compare the model and recent use. Removal frees a slot; the app can register it again on its next refresh. Physical devices cannot be reliably counted when a client does not support HWID.

## Verify the result

Removing a device frees a slot without resetting traffic. HWID support depends on the app; it is not cryptographic identification of physical hardware.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

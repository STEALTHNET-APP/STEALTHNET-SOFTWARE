[All sections](../README.md) · [Русский](../../ru/sections/settings.md) / [English](../../en/sections/settings.md)

# Addresses, access and branding

Configure public addresses, the subscription service key, sign-in security and external API access.

**Where to find it:** `#/settings` — System.

## Workflow

Set panel.example.com and sub.example.com first. Issue a key for a separate subscription service and install it. Enable 2FA and retain server access.

## Security and services

Enable 2FA and retain server recovery access. Issue service keys only for the intended installation. Rotating a key requires updating the separate service environment. The panel distinguishes API, database, node and subscription-service health; inspect the failing service instead of restarting everything.

## Verify the result

The customer website is installed separately on its own domain. Rotating a service key requires updating that service’s configuration. Service keys are only shown when issued.

## Related guides

[installation](../installation.md) · [subscription-installation](../subscription-installation.md) · [backup-restore](../backup-restore.md)

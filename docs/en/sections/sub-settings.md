[All sections](../README.md) · [Русский](../../ru/sections/sub-settings.md) / [English](../../en/sections/sub-settings.md)

# Client subscription settings

Profile title, announcements, refresh interval and status messages are sent to the client app with its subscription.

**Where to find it:** `#/sub-settings` — Subscriptions.

## Workflow

1. Review the starter content: the service name, a short Announce instruction, and explanations for six access states. Empty form fields are prefilled in the panel language.
2. Set your project's support contact. If one is saved in portal or bot settings, the form suggests it; otherwise it displays an example address.
3. Click **Save changes**, then refresh the subscription in the app. Starter content is not sent to clients until you save it.

Existing values are preserved. After the first save, intentionally cleared fields stay empty. Happ routes and extra HTTP headers are optional; empty sections explain what is configured. An empty Happ routing field does not change the app's routes.

These settings and client app cards belong to the subscription service. They do not require installing or updating the customer portal, which handles the storefront, registration, purchases, and Mini App.

## Checking the client app

Refresh the subscription manually after changing headers or update intervals. If an announcement is missing, check client header support and selected format. An external squad may override global values.

## Verify the result

Apps support different headers. When a subscription expires or a device limit is reached, the client receives an explanation instead of usable locations.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

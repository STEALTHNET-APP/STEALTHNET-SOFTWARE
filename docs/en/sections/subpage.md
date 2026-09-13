[All sections](../README.md) · [Русский](../../ru/sections/subpage.md) / [English](../../en/sections/subpage.md)

# Connection page

Customers see subscription status, available locations, apps and instructions. Purchases are handled in the bot and the customer website.

**Where to find it:** `#/subpage` — Subscriptions.

## Workflow

Add apps for every platform, including download links, deep links and clear steps. Test on a phone and a computer.

## Shared brand and hosting

The connection page uses logos, favicon and colors from Customer website & Mini App. Apps and instructions are edited here. Host sn-sub with the panel or separately through its API. Test /ID in a browser and a VPN app; each must receive the appropriate response.

## Verify the result

Use {{URL}} in deep links. A separate subscription service serves the page; configure its domain in settings.

## Related guides

[subscription-installation](../subscription-installation.md) · [branding](../branding.md) · [Custom DNS and routing](templates.md)

## Language and short links

Customer links use `https://sub.example.com/ID`. Existing `/s/ID` links continue to work without redirects or re-importing subscriptions. QR codes, the bot, customer portal and Mini App use the short URL.

The connection page supports Russian and English. The **RU / EN** button at the top changes the language and remembers it on this domain. The first visit follows the browser language; use `?lang=en` or `?lang=ru` to set a specific link’s language. Page language does not change the configuration imported by VPN apps. Plan names, location names and custom owner messages remain as entered by the owner.

For separate servers, update `sn-sub` on the subscription server to **v0.1.9 or later** before updating the panel, which will start generating short links. Existing links keep working after the update. See the [update instructions](../subscription-installation.md).

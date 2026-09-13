[All guides](README.md) · [Русский](../ru/addons.md) / [English](../en/addons.md)

# Device and traffic add-ons

Add-on packages and prices are configured **per plan**, not globally. Open a plan and locate add-ons for the bot and Mini App. Enter device-slot quantity or traffic GB, the project-currency price and optional Telegram Stars price, then save.

## Availability

The customer must have the corresponding eligible subscription. Traffic packages are offered only for limited-traffic subscriptions. Device packages increase the current device limit. Check website purchases, Mini App shop settings and the bot’s add-on menu button. The website and Mini App show actions on their device/traffic cards; the bot shows its add-ons menu.

Changing a package price affects new invoices. The invoice retains the purchased package snapshot. Removing a package hides it from future sales; already paid additions remain accounted for. Project currency changes do not convert prices.

## Duration

- Additional devices: until the paid subscription term ends.
- Traffic: until the applicable reset or subscription expiry.
- Neither purchase extends the subscription term or changes its base plan.

Traffic purchases can restore access when the traffic cap is exhausted. Reducing purchased device slots does not automatically delete registered devices. A payment received after expiry is retained as pending activation until a suitable subscription becomes active; traffic requires a limited-traffic subscription.

## Payment and refunds

The server calculates price and eligibility. A provider redirect or browser click is not proof of payment. Verified callbacks apply the purchased package transactionally; duplicates must not grant it again. A confirmed add-on refund removes that addition rather than ending the entire subscription. Recording a refund in the panel does not transfer funds back through the provider.

## A package is missing

1. Compare the customer’s actual plan with the plan where you saved the package.
2. Check plan activity, customer status, subscription expiry and traffic-cap eligibility.
3. Check website purchases, Mini App shop and the bot’s add-on button.
4. Reload the account or open `/start` again. Packages are fetched on request.
5. If the package is visible but checkout unavailable, configure a payment method for its currency.

[Plans](sections/tariffs.md) · [Payment providers](payment-gateways.md)

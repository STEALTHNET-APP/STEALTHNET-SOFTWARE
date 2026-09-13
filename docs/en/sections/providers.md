[All sections](../README.md) · [Русский](../../ru/sections/providers.md) / [English](../../en/sections/providers.md)

# Setting up payments

Providers create invoices and confirm payments. Available options include Stars, CryptoBot, manual payment and a configurable HTTP provider.

**Where to find it:** `#/providers` — Sales.

## Workflow

1. Choose the project currency.
2. configure provider keys.
3. enable supported currencies.
4. set plan prices.
5. make a test purchase.

## Provider configuration

Platega uses Merchant ID and secret; RollyPay uses an API key and signing secret; ParityPay v2 uses Shop ID and two secrets. The provider must reach its callback over HTTPS. Enter keys in provider settings and prices in plans. A disabled module or unsupported currency is not an available payment method. See the payment guide for callback paths and verification.

## Verify the result

A configured key does not prove that money was received. A custom API may require a Rust module and rebuild. Automatic charging requires provider support.

## Related guides

[payment-gateways](../payment-gateways.md) · [Invoices, payments and refunds](payments.md)

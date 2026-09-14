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

On a new installation, **Manual payments are enabled by default** for every panel currency: USD, EUR, RUB, UAH, KZT, TRY and GBP. No keys are required: customers receive instructions to contact support, and an administrator confirms receipt under Payments. You can disable the module or individual currencies; updates preserve that choice. Existing installations keep their previously saved settings.

An empty or absent `PAY_MANUAL_CURRENCIES` in `.env` means all currencies listed above. An explicit list, such as `USD,RUB`, continues to restrict the module. Telegram Stars are configured separately.

Platega uses Merchant ID and secret; RollyPay uses an API key and signing secret; ParityPay v2 uses Shop ID and two secrets. The provider must reach its callback over HTTPS. Enter keys in provider settings and prices in plans. A disabled module or unsupported currency is not an available payment method. See the payment guide for callback paths and verification.

A plan with a zero price activates without payment providers, including when purchases and renewals are disabled in the cabinet or Mini App. Paid plans remain unavailable. Trial access is still one-time; disable or hide the trial plan to withdraw it.

## Verify the result

A configured key does not prove that money was received. A custom API may require a Rust module and rebuild. Automatic charging requires provider support.

## Related guides

[payment-gateways](../payment-gateways.md) · [Invoices, payments and refunds](payments.md)

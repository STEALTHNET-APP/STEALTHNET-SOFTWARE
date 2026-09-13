[All guides](README.md) · [Русский](../ru/payment-gateways.md) / [English](../en/payment-gateways.md)

# Payment providers

Configure providers in **Payment methods & currencies**, then set prices on plans and add-on packages. New integrations start disabled. An update does not create a merchant account or insert test keys. Keep keys out of screenshots and support issues.

## Connect and verify

1. Select the project currency and check each plan price; changing currency does not convert existing amounts.
2. Create the merchant/shop with the payment provider and obtain its credentials.
3. Enter credentials in the provider card, choose supported methods/currencies and save.
4. Make the callback reachable over the public panel HTTPS domain.
5. Make a test purchase, verify the provider transaction, invoice amount/currency and exactly one access grant.
6. Test a duplicate callback and refund in an isolated environment before relying on automation.

| Provider | Credentials | Callback on your panel |
|---|---|---|
| Platega | Merchant ID, secret | `/api/pay/webhook/platega` |
| RollyPay | API key, signing secret | `/api/pay/webhook/rollypay` |
| ParityPay v2 | Shop ID, secret 1, secret 2 | `/api/pay/webhook/paritypay` |

## Platega

The adapter uses `https://app.platega.io`, merchant/secret headers and transaction IDs saved with invoices. Supported methods must match the merchant account and configured currency. `CONFIRMED` applies payment; `CHARGEBACKED` records a refund. Callback merchant credentials, amount and currency are checked. See the [provider documentation](https://docs.platega.io/).

## RollyPay

The adapter creates `/api/v1/payments` using `X-API-Key` and a unique `X-Nonce`. Callback authentication uses HMAC-SHA256 of timestamp + `.` + the original body, with a five-minute timestamp tolerance. `paid` applies payment; `refunded` and `chargeback` record refunds. Callbacks marked `test=true` do not grant real access. Configure the merchant-supported method; EUR uses international cards and has a minimum of 1 EUR. See [payments](https://docs.rollypay.io/api/payments/) and [callbacks](https://docs.rollypay.io/api/callbacks/).

## ParityPay API v2

The adapter uses `https://api.paritypay.net`, `X-ShopId` and secret 1 for invoice API calls. It creates `/v2/invoice/create` and checks `/v2/invoice/status`. Secret 2 validates callback HMAC signatures, including shop identity. Access uses the original `amount`, not the post-fee `credited` amount. `PAID` confirms payment and `REFUNDED` records a refund. This integration covers invoices, not payouts or automatic recurring charges. See the [v2 documentation](http://docs.paritypay.net/).

## Shared payment rules

Provider, transaction ID, amount and currency are verified before applying payment. Duplicate events do not extend access twice. A late pending event cannot cancel a successful payment; an old successful event cannot revive a refunded invoice.

A provider-confirmed add-on refund reverses the associated addition. A plan refund is recorded; the administrator decides what happens to service access. Referral rewards are adjusted. The panel’s Refund action records a refund already performed elsewhere; it does not send money.

Other available modules include Telegram Stars, CryptoBot and manual confirmation. Stars use an explicit XTR price. Manual invoices require verifying receipt before approval. Integration code and extension points are described in [development](development.md).

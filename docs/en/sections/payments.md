[All sections](../README.md) · [Русский](../../ru/sections/payments.md) / [English](../../en/sections/payments.md)

# Invoices, payments and refunds

Pending means an invoice was created; success means payment was applied; canceled means it expired or was canceled; refunded means an administrator recorded a refund.

**Where to find it:** `#/payments` — Sales.

## Workflow

Configure a payment provider, set a plan price in its currency and make a test purchase. For manual transfers, verify receipt of funds first.

## Funds received but access is missing

Find the invoice by customer and provider transaction ID. Check its provider status, amount and currency, then refresh the payment status. Pending or test callbacks do not grant access. Verify receipt before manual confirmation; creating another charge is not a repair.

## Verify the result

Recording a refund does not transfer money back. Refund through the provider, then record it here. Repeated webhooks must not extend access twice.

## Related guides

[payment-gateways](../payment-gateways.md) · [addons](../addons.md)

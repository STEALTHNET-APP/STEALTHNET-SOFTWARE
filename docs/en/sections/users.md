[All sections](../README.md) · [Русский](../../ru/sections/users.md) / [English](../../en/sections/users.md)

# One customer record for access and purchases

Expiry, limits, devices, squads, payments and customer history are kept together.

**Where to find it:** `#/users` — Customers.

## Workflow

1. Create a customer.
2. assign a plan or days and squads.
3. open the subscription link.
4. test the connection. Search supports names, UUIDs, Telegram IDs, email addresses and tags.

## Access code and identity

Open access-code management in the customer record. Viewing and copying require the appropriate administrator permissions. Resetting issues a new code, revokes the old one and ends website sessions; subscriptions and purchases remain. Customers link Telegram and the website using one-time confirmation. Do not create a second paid subscription just to enable website sign-in.

## Verify the result

Extending the expiry restores access only if traffic is available and the customer was not disabled manually. Removing a device does not reset traffic. Revoking access changes the keys and invalidates old links.

## Related guides

[Plans connect prices with access](tariffs.md) · [Access groups](squads-int.md) · [Customer website & Mini App](cabinet.md)

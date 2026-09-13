-- Prices belong to a tariff. Paid orders keep an immutable snapshot even after editing the catalog.
CREATE TABLE tariff_addons (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 tariff_id bigint NOT NULL REFERENCES tariffs(id) ON DELETE CASCADE,
 kind text NOT NULL CHECK (kind IN ('traffic','devices')),
 quantity integer NOT NULL CHECK (quantity > 0 AND quantity <= 100000),
 currency text NOT NULL,
 amount_minor bigint NOT NULL CHECK (amount_minor > 0),
 stars_minor bigint CHECK (stars_minor > 0),
 is_active boolean NOT NULL DEFAULT true,
 sort_order integer NOT NULL DEFAULT 0
);
ALTER TABLE payments ADD COLUMN addon_snapshot jsonb;
CREATE TABLE subscription_addons (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 payment_id bigint NOT NULL UNIQUE REFERENCES payments(id),
 client_id bigint NOT NULL REFERENCES clients(id),
 subscription_id bigint REFERENCES subscriptions(id),
 kind text NOT NULL CHECK (kind IN ('traffic','devices')),
 quantity bigint NOT NULL CHECK (quantity > 0),
 activated_at timestamptz,
 expires_at timestamptz,
 ended_at timestamptz
);
CREATE INDEX subscription_addons_client_idx ON subscription_addons(client_id) WHERE ended_at IS NULL;
CREATE INDEX subscription_addons_expiry_idx ON subscription_addons(expires_at) WHERE ended_at IS NULL;

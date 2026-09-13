-- Keep the original commission for deterministic partial-refund rounding.
ALTER TABLE partner_commissions ADD COLUMN IF NOT EXISTS base_amount_minor bigint NOT NULL DEFAULT 0;
UPDATE partner_commissions SET base_amount_minor = amount_minor WHERE base_amount_minor = 0;

-- The primary balance remains compatible with existing manual adjustments.
-- Other currencies have independent ledgers; no implicit exchange rate is used.
CREATE OR REPLACE VIEW partner_wallets AS
WITH currencies AS (
    SELECT id AS partner_id, currency FROM partners
    UNION SELECT partner_id, currency FROM partner_commissions
    UNION SELECT partner_id, currency FROM partner_payouts
), earned AS (
    SELECT partner_id, currency, sum(amount_minor)::bigint AS amount FROM partner_commissions GROUP BY 1,2
), paid AS (
    SELECT partner_id, currency, sum(amount_minor)::bigint AS amount FROM partner_payouts GROUP BY 1,2
)
SELECT c.partner_id, c.currency, COALESCE(e.amount,0) AS earned_minor,
       COALESCE(o.amount,0) AS paid_minor,
       CASE WHEN c.currency=p.currency THEN p.balance_minor
            ELSE COALESCE(e.amount,0)-COALESCE(o.amount,0) END AS balance_minor
FROM currencies c JOIN partners p ON p.id=c.partner_id
LEFT JOIN earned e ON e.partner_id=c.partner_id AND e.currency=c.currency
LEFT JOIN paid o ON o.partner_id=c.partner_id AND o.currency=c.currency;

DO $$ DECLARE app_role text; BEGIN
  app_role := COALESCE(NULLIF(current_setting('sn.app_role', true), ''),
                       (SELECT pg_get_userbyid(datdba) FROM pg_database WHERE datname=current_database()));
  EXECUTE format('ALTER VIEW partner_wallets OWNER TO %I', app_role);
END $$;

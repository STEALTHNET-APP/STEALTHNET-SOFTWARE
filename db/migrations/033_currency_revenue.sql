ALTER FUNCTION effective_status(client_status, timestamptz, bigint, bigint) STABLE;
-- Customer lifetime revenue excludes recorded partial refunds.
CREATE OR REPLACE VIEW client_overview AS
 SELECT c.id,
    c.public_id,
    c.username,
    effective_status(c.status, s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes) AS status,
    c.tag,
    c.note,
    c.short_id,
    c.last_online_at,
    c.created_at,
    c.referred_by,
    p.title AS referrer_title,
    p.slug AS referrer_slug,
    t.code AS tariff_code,
    s.expires_at,
    s.traffic_used_bytes,
    s.traffic_limit_bytes,
    s.device_limit,
    s.autorenew,
    COALESCE(( SELECT sum(tu.upload_bytes + tu.download_bytes)::bigint AS sum
           FROM traffic_usage tu
          WHERE tu.client_id = c.id), 0::bigint) AS traffic_total_bytes,
    ( SELECT count(*) AS count
           FROM devices d
          WHERE d.client_id = c.id) AS device_count,
    COALESCE(( SELECT sum(GREATEST(pm.amount_minor - COALESCE((pm.metadata->>'refunded_minor')::bigint, 0), 0))::bigint AS sum
           FROM payments pm
          WHERE pm.client_id = c.id AND pm.status = 'success'::payment_status
            AND pm.currency=COALESCE((SELECT upper(value #>> '{}') FROM settings WHERE key='billing.currency'),'USD')), 0::bigint) AS ltv_minor,
    ( SELECT count(*) AS count
           FROM payments pm
          WHERE pm.client_id = c.id AND pm.status = 'success'::payment_status) AS payment_count,
    -- Новые колонки приписаны в конец: CREATE OR REPLACE VIEW не умеет
    -- вставлять их в середину — только дописывать.
    s.reset_strategy,
    s.traffic_reset_at,
    COALESCE((SELECT upper(value #>> '{}') FROM settings WHERE key='billing.currency'),'USD') AS ltv_currency,
    COALESCE((SELECT jsonb_agg(to_jsonb(v) ORDER BY v.currency) FROM (
      SELECT pm.currency, sum(GREATEST(pm.amount_minor-COALESCE((pm.metadata->>'refunded_minor')::bigint,0),0))::bigint AS amount_minor
      FROM payments pm WHERE pm.client_id=c.id AND pm.status='success' GROUP BY pm.currency
    ) v),'[]'::jsonb) AS ltv_by_currency
   FROM clients c
     LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
     LEFT JOIN tariffs t ON t.id = s.tariff_id
     LEFT JOIN partners p ON p.id = c.referred_by
  WHERE c.deleted_at IS NULL;

DO $$
DECLARE app_role text := current_setting('sn.app_role', true);
BEGIN
    IF app_role IS NULL OR app_role = '' THEN
        SELECT pg_get_userbyid(datdba) INTO app_role FROM pg_database WHERE datname = current_database();
    END IF;
    EXECUTE format('ALTER VIEW client_overview OWNER TO %I', app_role);
END $$;

-- ═══════════════════════════════════════════════════════════════════
--  sum(bigint) в PostgreSQL возвращает numeric, а не bigint.
--  Без явного приведения клиент на стороне приложения либо падает,
--  либо (что хуже) молча получает NULL — так LTV в списке клиентов
--  всегда оказывался пустым.
-- ═══════════════════════════════════════════════════════════════════

-- Тип колонки во вьюхе менять нельзя через REPLACE — пересоздаём.
DROP VIEW IF EXISTS client_overview;
CREATE VIEW client_overview AS
SELECT
    c.id, c.public_id, c.username, c.status, c.tag, c.short_id,
    c.last_online_at, c.created_at,
    t.code                AS tariff_code,
    s.expires_at,
    s.traffic_used_bytes,
    s.traffic_limit_bytes,
    s.device_limit,
    s.autorenew,
    (SELECT count(*) FROM devices d WHERE d.client_id = c.id)      AS device_count,
    COALESCE((SELECT sum(p.amount_minor)::bigint FROM payments p
              WHERE p.client_id = c.id AND p.status = 'success'), 0) AS ltv_minor,
    (SELECT count(*) FROM payments p
      WHERE p.client_id = c.id AND p.status = 'success')            AS payment_count
FROM clients c
LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
LEFT JOIN tariffs t       ON t.id = s.tariff_id
WHERE c.deleted_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
--  Фактический статус подписки.
--
--  Проблема, которую чиним: `clients.status` меняется только событиями
--  (оплата, действие админа, приём трафика). Когда просто проходит дата
--  окончания, статус остаётся `active` — и подписка продолжает выдавать
--  конфиги бесплатно. Фоновая задача это исправит, но полагаться только
--  на неё нельзя: упадёт воркер — и сервис раздаёт доступ даром.
--
--  Решение: функция считает статус ЗДЕСЬ И СЕЙЧАС из даты и трафика.
--  Все места (выдача подписки, конфиг для ноды, список клиентов)
--  спрашивают её, а не хранимое поле. Воркер лишь материализует
--  результат, чтобы по нему можно было фильтровать и строить отчёты.
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION effective_status(
    stored      client_status,
    expires_at  timestamptz,
    used_bytes  bigint,
    limit_bytes bigint
) RETURNS client_status
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT CASE
        -- Отключение вручную и без того сильнее всего остального.
        WHEN stored = 'disabled' THEN 'disabled'::client_status
        WHEN expires_at IS NOT NULL AND expires_at <= now() THEN 'expired'::client_status
        WHEN limit_bytes IS NOT NULL AND COALESCE(used_bytes, 0) >= limit_bytes
             THEN 'limited'::client_status
        WHEN stored = 'active' THEN 'active'::client_status
        ELSE stored
    END
$$;

-- Витрина клиента теперь показывает фактический статус, а не хранимый.
DROP VIEW IF EXISTS client_overview;
CREATE VIEW client_overview AS
SELECT
    c.id, c.public_id, c.username, c.tag, c.short_id,
    effective_status(c.status, s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes) AS status,
    c.status AS stored_status,
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

-- Кого фоновая задача должна перевести в новый статус.
CREATE OR REPLACE VIEW clients_needing_status_sync AS
SELECT c.id,
       c.status AS stored_status,
       effective_status(c.status, s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes) AS actual_status
  FROM clients c
  LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
 WHERE c.deleted_at IS NULL
   AND c.status <> effective_status(c.status, s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes);

-- Быстрый поиск истекающих: по нему работают напоминания и автопродление.
CREATE INDEX IF NOT EXISTS subscriptions_expiring_soon_idx
    ON subscriptions (expires_at)
    WHERE is_current AND expires_at IS NOT NULL;

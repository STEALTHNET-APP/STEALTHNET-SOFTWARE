-- Заметка администратора, источник клиента и трафик за всё время.
--
-- Заметка писалась в clients.note, но во вьюху не попадала — панель
-- сохраняла её и тут же «забывала»: при повторном открытии карточки поле
-- было пустым.
--
-- Источник (по какой партнёрской ссылке пришёл клиент) в базе был всегда,
-- но карточка показывала выдуманную строку вместо него.
--
-- Трафик за всё время карточка показывала числом-заглушкой. Считаем его
-- из traffic_usage: traffic_used_bytes в подписке обнуляется при сбросе
-- периода и всей историей не является.

DROP VIEW IF EXISTS client_overview;
CREATE VIEW client_overview AS
SELECT
    c.id, c.public_id, c.username, c.status, c.tag, c.note, c.short_id,
    c.last_online_at, c.created_at,
    c.referred_by,
    p.title               AS referrer_title,
    p.slug                AS referrer_slug,
    t.code                AS tariff_code,
    s.expires_at,
    s.traffic_used_bytes,
    s.traffic_limit_bytes,
    s.device_limit,
    s.autorenew,
    COALESCE((SELECT sum(tu.upload_bytes + tu.download_bytes)::bigint
                FROM traffic_usage tu WHERE tu.client_id = c.id), 0)  AS traffic_total_bytes,
    (SELECT count(*) FROM devices d WHERE d.client_id = c.id)         AS device_count,
    COALESCE((SELECT sum(pm.amount_minor)::bigint FROM payments pm
              WHERE pm.client_id = c.id AND pm.status = 'success'), 0) AS ltv_minor,
    (SELECT count(*) FROM payments pm
      WHERE pm.client_id = c.id AND pm.status = 'success')            AS payment_count
FROM clients c
LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
LEFT JOIN tariffs t       ON t.id = s.tariff_id
LEFT JOIN partners p      ON p.id = c.referred_by
WHERE c.deleted_at IS NULL;

-- Пересозданная вьюха принадлежит тому, кто выполнил миграцию. Если это
-- postgres, приложение теряет доступ и все списки клиентов отвечают
-- «внутренняя ошибка». Возвращаем владельца явно.
DO $$
DECLARE app_role text := current_setting('sn.app_role', true);
BEGIN
    IF app_role IS NULL OR app_role = '' THEN
        SELECT pg_get_userbyid(datdba) INTO app_role FROM pg_database WHERE datname = current_database();
    END IF;
    EXECUTE format('ALTER VIEW client_overview OWNER TO %I', app_role);
    EXECUTE format('GRANT SELECT ON client_overview TO %I', app_role);
END $$;

-- Продление даты должно возвращать клиента в строй.
--
-- Было: последняя ветка `ELSE stored`. Клиент со статусом «expired»
-- оставался «expired» даже с датой окончания в будущем — ни одно из
-- условий выше не срабатывало, и функция возвращала прежнее значение.
--
-- Заметить это трудно: администратор ставит дату в карточке клиента,
-- панель показывает новую дату, в базе она тоже новая — а доступа нет,
-- и в приложении по-прежнему «подписка закончилась». Воркер не спасал:
-- он приводит хранимый статус к тому, что считает эта же функция, то
-- есть подтверждал «expired» снова и снова. Выйти из этого состояния
-- можно было только начислением дней, которое ставит статус явно.
--
-- Теперь: если срок задан и он в будущем, а трафик не исчерпан —
-- клиент активен, каким бы ни был хранимый статус.
--
-- Отключённого вручную это не касается: `disabled` сильнее всего
-- остального и снимается только руками.
--
-- Клиентов без подписки (`expires_at IS NULL`) не трогаем: там пустая
-- дата означает не «бессрочно», а «подписки нет вовсе», и оживлять их
-- нельзя. Поэтому оживление привязано к наличию даты.

CREATE OR REPLACE FUNCTION effective_status(
    stored      client_status,
    expires_at  timestamptz,
    used_bytes  bigint,
    limit_bytes bigint
) RETURNS client_status
LANGUAGE sql IMMUTABLE PARALLEL SAFE
AS $$
    SELECT CASE
        -- Отключение вручную и без того сильнее всего остального.
        WHEN stored = 'disabled' THEN 'disabled'::client_status
        WHEN expires_at IS NOT NULL AND expires_at <= now() THEN 'expired'::client_status
        WHEN limit_bytes IS NOT NULL AND COALESCE(used_bytes, 0) >= limit_bytes
             THEN 'limited'::client_status
        WHEN stored = 'active' THEN 'active'::client_status
        -- Срок в будущем и трафик в пределах: клиент оплачен, каким бы
        -- ни был хранимый статус. Без этой ветки продление даты не
        -- возвращало доступ.
        WHEN expires_at IS NOT NULL THEN 'active'::client_status
        ELSE stored
    END
$$;

COMMENT ON FUNCTION effective_status(client_status, timestamptz, bigint, bigint) IS
    'Фактический статус клиента по срокам и трафику. Хранимый учитывается только для disabled и как запасной вариант при отсутствии подписки.';

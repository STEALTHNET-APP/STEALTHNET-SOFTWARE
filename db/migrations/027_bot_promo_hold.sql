-- Промокод, введённый в боте, до оплаты.
--
-- Между вводом кода и нажатием «оплатить» несколько экранов: выбор
-- периода, выбор способа. Держать код всё это время негде — в `bot_await`
-- лежит то, чего бот ждёт следующим сообщением, и оно вычитывается
-- один раз.
--
-- Отдельная запись, одна на клиента: одновременно применять два кода
-- незачем, а новый ввод просто заменяет прежний. Протухает, чтобы
-- брошенная на полпути покупка не всплыла скидкой через неделю.

CREATE TABLE IF NOT EXISTS bot_promo_hold (
    client_id  bigint PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    promo_id   bigint NOT NULL REFERENCES promo_codes(id) ON DELETE CASCADE,
    code       text   NOT NULL,
    expires_at timestamptz NOT NULL DEFAULT now() + interval '1 hour'
);

COMMENT ON TABLE bot_promo_hold IS
    'Промокод, выбранный клиентом в боте, до момента оплаты. Живёт час.';

-- Владельца задаём явно: миграции запускают от postgres, и приложение
-- иначе получит «permission denied» ровно в момент нажатия кнопки.
-- На этом уже дважды обожглись — с client_overview и bot_await.
DO $$
DECLARE app_role text := current_setting('sn.app_role', true);
BEGIN
    IF app_role IS NULL OR app_role = '' THEN
        SELECT pg_get_userbyid(datdba) INTO app_role FROM pg_database WHERE datname = current_database();
    END IF;
    EXECUTE format('ALTER TABLE bot_promo_hold OWNER TO %I', app_role);
END $$;

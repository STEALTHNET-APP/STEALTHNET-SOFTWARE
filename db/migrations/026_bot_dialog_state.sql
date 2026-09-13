-- Состояние диалога с ботом.
--
-- Обращение в поддержку — это два шага: нажали «написать», прислали
-- текст. Между ними нужно помнить, чего мы ждём от человека и по какому
-- поводу, иначе следующее его сообщение уйдёт в общий обработчик и
-- откроет главное меню вместо того, чтобы стать обращением.
--
-- Отдельная таблица, а не bot_state: там значение целое и хранится
-- смещение апдейтов, а здесь нужны строка и срок годности. Ожидание
-- протухает: человек мог передумать и вернуться через сутки, и его
-- «привет» не должно тогда стать текстом обращения.

CREATE TABLE IF NOT EXISTS bot_await (
    client_id   bigint PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    -- Чего ждём: 'ticket_new' — текст нового обращения,
    -- 'ticket_reply' — ответ в уже открытое.
    kind        text        NOT NULL,
    -- К какому обращению относится ответ.
    ticket_id   bigint      REFERENCES tickets(id) ON DELETE CASCADE,
    expires_at  timestamptz NOT NULL DEFAULT now() + interval '30 minutes'
);

COMMENT ON TABLE bot_await IS
    'Чего бот ждёт от клиента следующим сообщением. Протухает через полчаса.';

-- Владельца задаём явно.
--
-- Миграции запускают от postgres, и созданная таблица досталась бы ему:
-- приложение ходит под своей ролью и получило бы «permission denied»
-- ровно в тот момент, когда человек нажмёт «написать обращение». Тем же
-- кончилось пересоздание вида client_overview — не повторяем.
DO $$
DECLARE app_role text := current_setting('sn.app_role', true);
BEGIN
    IF app_role IS NULL OR app_role = '' THEN
        SELECT pg_get_userbyid(datdba) INTO app_role FROM pg_database WHERE datname = current_database();
    END IF;
    EXECUTE format('ALTER TABLE bot_await OWNER TO %I', app_role);
END $$;

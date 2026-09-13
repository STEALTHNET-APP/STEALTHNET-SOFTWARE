-- ═══════════════════════════════════════════════════════════════════
--  Токены служебных сервисов.
--
--  Сервис подписок можно вынести на отдельный сервер: там нет доступа
--  к базе, и все данные он получает через API панели. Доступ к этому
--  API открывает токен — раньше он задавался только переменной
--  окружения SUB_SERVICE_TOKEN, то есть выпустить или сменить его без
--  захода на сервер панели было нельзя.
--
--  Храним не сам токен, а его SHA-256: базу и бэкапы читает больше
--  людей, чем нужно для доступа к API. Показать токен можно ровно
--  один раз — в момент выпуска.
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS service_tokens (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- Какому сервису выдан. Пока это только 'sub', но поле оставляем:
    -- следующими будут токены внешних интеграций.
    kind        text        NOT NULL,
    name        text        NOT NULL,
    token_hash  bytea       NOT NULL,
    -- Первые символы токена. Нужны, чтобы отличить токены в списке,
    -- не храня их целиком.
    prefix      text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    created_by  bigint      REFERENCES admins(id) ON DELETE SET NULL,
    -- Когда токеном пользовались в последний раз. Это первое, на что
    -- смотрят, когда «сабка не отвечает»: пустое поле означает, что
    -- сервис не дошёл до панели ни разу.
    last_used_at timestamptz,
    revoked_at  timestamptz
);

-- Поиск идёт по хешу на каждом запросе сервиса — без индекса это
-- последовательное чтение таблицы.
CREATE UNIQUE INDEX IF NOT EXISTS service_tokens_hash_idx
    ON service_tokens (token_hash);

-- Действующий токен у каждого вида сервиса ровно один: выпуск нового
-- отзывает предыдущий. Частичный индекс не даёт развести дубли.
CREATE UNIQUE INDEX IF NOT EXISTS service_tokens_active_kind_idx
    ON service_tokens (kind) WHERE revoked_at IS NULL;

-- Публичный адрес сабки. Панель и бот собирают из него ссылки для
-- клиентов, поэтому он должен быть тем адресом, который виден снаружи,
-- а не localhost.
INSERT INTO settings (key, value) VALUES
    ('subscription.public_url', '""')
ON CONFLICT (key) DO NOTHING;

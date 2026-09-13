-- ═══════════════════════════════════════════════════════════════════
--  Журнал обращений внешних сквадов.
--
--  Нужен ограничению частоты: без него утёкший токен будут выкачивать
--  непрерывно, и заметить это будет нечем. Заодно по журналу видно,
--  пользуется ли партнёр доступом вообще.
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS external_squad_requests (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    external_squad_id bigint NOT NULL REFERENCES external_squads(id) ON DELETE CASCADE,
    ip                inet,
    at                timestamptz NOT NULL DEFAULT now()
);

-- Считаем запросы за последнюю минуту на каждом обращении.
CREATE INDEX IF NOT EXISTS ext_squad_requests_recent_idx
    ON external_squad_requests (external_squad_id, at DESC);

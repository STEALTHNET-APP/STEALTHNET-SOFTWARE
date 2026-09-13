-- ═══════════════════════════════════════════════════════════════════
--  Отчёты из журнала доступа движка.
--
--  Xray умеет писать, что и куда пошло, но наружу этого не отдаёт:
--  журнал лежит файлом на ноде. Агент его читает, сворачивает в
--  счётчики и присылает — сырые строки в панель не уезжают.
--
--  Два разных отчёта из одного источника:
--   • заблокированные торренты — кто пытался, сколько раз;
--   • домены — куда ходят клиенты.
--
--  Второй выключен по умолчанию: это журнал посещённых сайтов, самое
--  ценное, что может утечь из VPN-панели.
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS torrent_reports (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id bigint REFERENCES clients(id) ON DELETE CASCADE,
    node_id   bigint REFERENCES nodes(id) ON DELETE CASCADE,
    day       date   NOT NULL,
    hits      int    NOT NULL DEFAULT 0,
    -- Последняя цель — чтобы отличить торрент от ложного срабатывания.
    last_target text,
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (client_id, node_id, day)
);

CREATE INDEX IF NOT EXISTS torrent_reports_day_idx ON torrent_reports (day DESC);

CREATE TABLE IF NOT EXISTS http_domain_stats (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    node_id bigint REFERENCES nodes(id) ON DELETE CASCADE,
    day     date NOT NULL,
    domain  text NOT NULL,
    hits    int  NOT NULL DEFAULT 0,
    UNIQUE (node_id, day, domain)
);

CREATE INDEX IF NOT EXISTS http_domain_stats_day_idx ON http_domain_stats (day DESC, hits DESC);

INSERT INTO settings (key, value) VALUES
    -- Блокировка торрентов работает всегда; речь только об отчётах.
    ('reports.torrents_enabled', 'true'),
    -- Сбор доменов по умолчанию выключен: включение должно быть
    -- осознанным решением владельца сервиса.
    ('reports.http_enabled', 'false'),
    -- Сколько дней хранить. Журнал посещений не должен копиться вечно.
    ('reports.retention_days', '30')
ON CONFLICT (key) DO NOTHING;

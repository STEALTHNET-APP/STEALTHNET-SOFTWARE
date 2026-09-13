-- ═══════════════════════════════════════════════════════════════════
--  Системная информация ноды.
--
--  Администратор смотрит на список нод, чтобы за секунду понять, где
--  беда: какая перегружена, какая молчит, какая упёрлась в канал.
--  Раньше панель знала только статус и версии — по ним этого не видно.
--
--  Медленно меняющееся (модель процессора, ядро, объём памяти) держим
--  в самой ноде: писать это в метрики каждые 15 секунд значит хранить
--  одну и ту же строку миллионы раз.
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE nodes
    ADD COLUMN IF NOT EXISTS cpu_model       text,
    ADD COLUMN IF NOT EXISTS cpu_cores       int,
    ADD COLUMN IF NOT EXISTS kernel          text,
    ADD COLUMN IF NOT EXISTS mem_total_bytes bigint,
    ADD COLUMN IF NOT EXISTS mem_used_bytes  bigint,
    ADD COLUMN IF NOT EXISTS uptime_seconds  bigint,
    -- Сетевой интерфейс, по которому считаем скорость, и его счётчики
    -- с момента загрузки сервера.
    ADD COLUMN IF NOT EXISTS iface           text,
    ADD COLUMN IF NOT EXISTS rx_total_bytes  bigint,
    ADD COLUMN IF NOT EXISTS tx_total_bytes  bigint;

-- Средняя загрузка за 1/5/15 минут. Одно мгновенное значение CPU врёт:
-- по нему не отличить пик от постоянной перегрузки.
ALTER TABLE node_metrics
    ADD COLUMN IF NOT EXISTS la1  real,
    ADD COLUMN IF NOT EXISTS la5  real,
    ADD COLUMN IF NOT EXISTS la15 real;

-- ═══════════════════════════════════════════════════════════════════
--  Автоматическое обслуживание партиций.
--
--  Зачем: партиционированная таблица без подходящей партиции ОТКЛОНЯЕТ вставку.
--  На проде это значит, что первого числа месяца сбор трафика молча ломается,
--  если никто заранее не создал партицию. Для проекта, который ставят себе
--  чужие люди, полагаться на «админ не забудет» нельзя.
--
--  Решение из двух слоёв:
--   1) `ensure_partitions()` создаёт партиции на N месяцев вперёд — вызывается
--      при установке и потом по расписанию;
--   2) DEFAULT-партиция ловит всё, что всё-таки не попало в диапазон, — данные
--      не теряются.
--
--  ⚠️ Оговорка про DEFAULT: если в него уже попали строки за месяц X, то
--  создать позже обычную партицию за X не выйдет, пока эти строки оттуда не
--  убрать. Поэтому DEFAULT — аварийный буфер, а не штатный режим; следите,
--  чтобы он оставался пустым (см. `partition_health`).
-- ═══════════════════════════════════════════════════════════════════

-- Создаёт месячную партицию, если её ещё нет. Идемпотентна.
CREATE OR REPLACE FUNCTION create_month_partition(parent text, month_start date)
RETURNS text LANGUAGE plpgsql AS $$
DECLARE
    part_name text;
    month_end date := (month_start + interval '1 month')::date;
BEGIN
    part_name := format('%s_%s', parent, to_char(month_start, 'YYYY_MM'));

    IF to_regclass(part_name) IS NOT NULL THEN
        RETURN part_name || ' (уже была)';
    END IF;

    EXECUTE format(
        'CREATE TABLE %I PARTITION OF %I FOR VALUES FROM (%L) TO (%L)',
        part_name, parent, month_start, month_end
    );
    RETURN part_name || ' (создана)';
END $$;

-- Создаёт партиции в окне [сегодня - months_back, сегодня + months_ahead].
CREATE OR REPLACE FUNCTION ensure_partitions(months_back int DEFAULT 6,
                                             months_ahead int DEFAULT 6)
RETURNS TABLE(result text) LANGUAGE plpgsql AS $$
DECLARE
    m date;
    parent text;
BEGIN
    FOREACH parent IN ARRAY ARRAY['traffic_usage', 'subscription_requests'] LOOP
        m := date_trunc('month', current_date - (months_back || ' months')::interval)::date;
        WHILE m <= date_trunc('month', current_date + (months_ahead || ' months')::interval)::date LOOP
            result := create_month_partition(parent, m);
            RETURN NEXT;
            m := (m + interval '1 month')::date;
        END LOOP;
    END LOOP;
END $$;

-- Аварийные партиции: лучше принять строку в DEFAULT, чем потерять трафик.
CREATE TABLE IF NOT EXISTS traffic_usage_default PARTITION OF traffic_usage DEFAULT;
CREATE TABLE IF NOT EXISTS subscription_requests_default PARTITION OF subscription_requests DEFAULT;

-- Здоровье партиционирования: DEFAULT должен быть пустым, а запас — не кончаться.
CREATE OR REPLACE VIEW partition_health AS
SELECT 'traffic_usage'                                   AS relation,
       (SELECT count(*) FROM traffic_usage_default)       AS rows_in_default,
       (SELECT count(*) FROM pg_class c
          JOIN pg_inherits i ON i.inhrelid = c.oid
         WHERE i.inhparent = 'traffic_usage'::regclass)    AS partitions
UNION ALL
SELECT 'subscription_requests',
       (SELECT count(*) FROM subscription_requests_default),
       (SELECT count(*) FROM pg_class c
          JOIN pg_inherits i ON i.inhrelid = c.oid
         WHERE i.inhparent = 'subscription_requests'::regclass);

-- Раскатываем окно партиций прямо сейчас.
SELECT * FROM ensure_partitions(12, 12);

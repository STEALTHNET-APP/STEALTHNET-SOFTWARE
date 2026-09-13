-- ═══════════════════════════════════════════════════════════════════
--  Расходы на инфраструктуру.
--
--  Выручка без расходов — не прибыль. Продавец должен видеть, во что
--  ему обходится парк серверов, рядом с тем, сколько он заработал,
--  и не держать это в отдельной таблице в почте.
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE nodes
    ADD COLUMN IF NOT EXISTS infra_provider_id  bigint REFERENCES infra_providers(id) ON DELETE SET NULL,
    -- Стоимость в минимальных единицах валюты системы: та же валюта,
    -- что и у выручки, иначе маржу пришлось бы считать по курсу.
    ADD COLUMN IF NOT EXISTS monthly_cost_minor bigint NOT NULL DEFAULT 0,
    -- День месяца, когда хостер списывает деньги. Нужен, чтобы панель
    -- предупреждала заранее, а не постфактум.
    ADD COLUMN IF NOT EXISTS bill_day           int;

ALTER TABLE nodes
    ADD CONSTRAINT nodes_bill_day_range
    CHECK (bill_day IS NULL OR (bill_day BETWEEN 1 AND 31)) NOT VALID;

-- Отчёт «сколько ушло за месяц» строится по этому полю.
CREATE INDEX IF NOT EXISTS infra_payments_paid_on_idx ON infra_payments (paid_on DESC);

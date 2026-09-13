-- ═══════════════════════════════════════════════════════════════════
--  Плагины ноды.
--
--  Это не «запуск чужого кода на сервере», как звучит по названию, а
--  фиксированный набор возможностей, который агент включает по JSON
--  и применяет правилами nftables:
--
--   • блокировщик торрентов — при срабатывании правила движка адрес
--     отправителя блокируется на время;
--   • входящий фильтр — не пускать эти адреса на ноду;
--   • исходящий фильтр — не выпускать с ноды на эти адреса и порты;
--   • общие списки — переиспользуемые наборы адресов.
--
--  Конфигурация лежит у ноды, а не у профиля: профиль общий для
--  нескольких нод, а блокировки почти всегда нужны точечные.
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE nodes
    ADD COLUMN IF NOT EXISTS plugins jsonb NOT NULL DEFAULT '{}'::jsonb;

-- Что агент сообщил о готовности сервера: без nftables и прав
-- NET_ADMIN плагины не заработают, и панель не должна делать вид,
-- что всё включено.
ALTER TABLE nodes
    ADD COLUMN IF NOT EXISTS plugins_status jsonb;

-- Блокировки, наложенные блокировщиком торрентов. Нужны панели,
-- чтобы показать, кого именно и до какого времени отрезали.
CREATE TABLE IF NOT EXISTS node_ip_blocks (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    node_id   bigint NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    ip        inet   NOT NULL,
    reason    text   NOT NULL,
    client_id bigint REFERENCES clients(id) ON DELETE SET NULL,
    until     timestamptz,
    at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (node_id, ip)
);

CREATE INDEX IF NOT EXISTS node_ip_blocks_until_idx ON node_ip_blocks (until);

-- Снятые администратором блокировки. Нужны отдельной таблицей: агент
-- должен убрать запись из ядра, а узнать об этом ему неоткуда —
-- удалённой строки в node_ip_blocks уже нет.
CREATE TABLE IF NOT EXISTS node_ip_unblocks (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    node_id bigint NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    ip      inet   NOT NULL,
    at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS node_ip_unblocks_at_idx ON node_ip_unblocks (node_id, at DESC);

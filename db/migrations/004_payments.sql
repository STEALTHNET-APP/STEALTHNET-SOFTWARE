-- ═══════════════════════════════════════════════════════════════════
--  Модульные платежи и Telegram-бот.
--
--  Идея: ядро не знает ни одного платёжного провайдера в лицо.
--  Оно умеет «выставить счёт» и «принять уведомление об оплате»,
--  а КАК это делается — дело модуля. Валюта выбирает модуль:
--  у рублей может быть один провайдер, у USDT — другой.
-- ═══════════════════════════════════════════════════════════════════

-- Реестр установленных платёжных модулей.
-- Строка появляется автоматически при старте: модуль сам себя регистрирует.
CREATE TABLE payment_providers (
    id           text        PRIMARY KEY,        -- 'stars', 'cryptobot', 'manual'
    title        text        NOT NULL,
    -- Валюты, которые модуль умеет принимать. Пересечение с ценами тарифа
    -- определяет, какие способы оплаты увидит клиент.
    currencies   text[]      NOT NULL DEFAULT '{}',
    is_enabled   boolean     NOT NULL DEFAULT false,
    is_configured boolean    NOT NULL DEFAULT false,  -- заданы ли ключи
    sort_order   integer     NOT NULL DEFAULT 100,
    -- Настройки модуля (ключи, комиссии, лимиты). Секреты сюда не кладём —
    -- они приходят из окружения; здесь только не-секретная конфигурация.
    config       jsonb       NOT NULL DEFAULT '{}',
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- Счёт на оплату: живёт до успеха, отказа или истечения.
ALTER TABLE payments
    ADD COLUMN pay_url          text,
    ADD COLUMN expires_at       timestamptz,
    ADD COLUMN provider_payload jsonb NOT NULL DEFAULT '{}';

-- Незакрытые счета ищем часто — по клиенту и по сроку.
CREATE INDEX payments_pending_idx ON payments (client_id, created_at DESC)
    WHERE status = 'pending';
CREATE INDEX payments_expiring_idx ON payments (expires_at)
    WHERE status = 'pending' AND expires_at IS NOT NULL;

-- Журнал входящих вебхуков: нужен и для разбора инцидентов,
-- и как второй рубеж защиты от повторной обработки.
CREATE TABLE payment_webhooks (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    provider    text        NOT NULL,
    external_id text,                            -- id события у провайдера
    payload     jsonb       NOT NULL,
    signature_ok boolean    NOT NULL DEFAULT false,
    processed   boolean     NOT NULL DEFAULT false,
    payment_id  bigint      REFERENCES payments ON DELETE SET NULL,
    error       text,
    received_at timestamptz NOT NULL DEFAULT now()
);
-- Одно и то же событие провайдер может прислать несколько раз.
CREATE UNIQUE INDEX payment_webhooks_external_key
    ON payment_webhooks (provider, external_id) WHERE external_id IS NOT NULL;
CREATE INDEX payment_webhooks_received_idx ON payment_webhooks (received_at DESC);

-- ── Telegram-бот ────────────────────────────────────────────────────

-- Состояние диалога. Хранить в памяти нельзя: бот перезапускается,
-- а человек в этот момент вводит промокод.
CREATE TABLE bot_dialogs (
    telegram_id  bigint      PRIMARY KEY,
    client_id    bigint      REFERENCES clients ON DELETE CASCADE,
    state        text        NOT NULL DEFAULT 'idle',
    context      jsonb       NOT NULL DEFAULT '{}',
    last_message_id bigint,
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- Смещение long-polling, чтобы после перезапуска не обрабатывать заново.
CREATE TABLE bot_state (
    key   text PRIMARY KEY,
    value bigint NOT NULL
);

-- Тексты бота редактируются из панели, а не правкой кода.
CREATE TABLE bot_texts (
    key        text PRIMARY KEY,
    locale     text NOT NULL DEFAULT 'ru',
    body       text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO bot_texts (key, body) VALUES
('start', E'👋 Добро пожаловать!\n\nЗдесь вы можете купить доступ к VPN и управлять подпиской.'),
('no_subscription', E'У вас пока нет активной подписки.\nВыберите тариф — это займёт минуту.'),
('payment_success', E'✅ Оплата получена!\n\nПодписка активна до {expires}.\nСсылка для подключения ниже.'),
('help', E'Если что-то не работает — напишите нам, поможем.');

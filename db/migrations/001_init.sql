-- ═══════════════════════════════════════════════════════════════════
--  STEALTHNET CORE — начальная схема (PostgreSQL 16+)
--
--  Соглашения:
--   • PK  — bigint identity (компактный индекс, важно для таблиц трафика)
--   • внешний идентификатор — отдельная колонка uuid, только там где нужен
--   • деньги — bigint в минорных единицах (центы) + ISO-код валюты, НИКОГДА float
--   • трафик — bigint в байтах, НИКОГДА гигабайты дробью
--   • время — timestamptz, всегда UTC
--   • удаление сущностей с историей — мягкое (deleted_at), а не DELETE
-- ═══════════════════════════════════════════════════════════════════

CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_uuid()

-- ─────────────────────────── перечисления ───────────────────────────

CREATE TYPE client_status     AS ENUM ('active','disabled','limited','expired');
CREATE TYPE payment_status    AS ENUM ('pending','success','failed','refunded','canceled');
CREATE TYPE payment_kind      AS ENUM ('purchase','renewal','autorenewal','topup','addon');
CREATE TYPE identity_kind     AS ENUM ('telegram','email');
CREATE TYPE promo_kind        AS ENUM ('percent','fixed','days');
CREATE TYPE reset_strategy    AS ENUM ('no_reset','day','week','month');
CREATE TYPE node_status       AS ENUM ('online','offline','disabled','provisioning');
CREATE TYPE host_security     AS ENUM ('reality','tls','none');
CREATE TYPE ticket_status     AS ENUM ('open','pending','closed');
CREATE TYPE ticket_priority   AS ENUM ('low','normal','high');
CREATE TYPE broadcast_status  AS ENUM ('draft','scheduled','sending','sent','canceled');
CREATE TYPE engine_kind       AS ENUM ('xray','singbox');

-- вспомогательный триггер: автообновление updated_at
CREATE FUNCTION touch_updated_at() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END $$;


-- ═══════════════════════════════════════════════════════════════════
--  1. ДОСТУП В ПАНЕЛЬ
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE admins (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id     uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    username      text        NOT NULL,
    email         text,
    password_hash text        NOT NULL,           -- argon2id
    totp_secret   text,                           -- NULL = 2FA выключена
    role          text        NOT NULL DEFAULT 'admin',   -- owner | admin | support | readonly
    is_active     boolean     NOT NULL DEFAULT true,
    last_login_at timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX admins_username_key ON admins (lower(username));
CREATE UNIQUE INDEX admins_email_key    ON admins (lower(email)) WHERE email IS NOT NULL;
CREATE TRIGGER admins_touch BEFORE UPDATE ON admins
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- WebAuthn / passkeys
CREATE TABLE admin_passkeys (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin_id      bigint      NOT NULL REFERENCES admins ON DELETE CASCADE,
    credential_id bytea       NOT NULL UNIQUE,
    public_key    bytea       NOT NULL,
    sign_count    bigint      NOT NULL DEFAULT 0,
    label         text        NOT NULL,
    last_used_at  timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE admin_sessions (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin_id     bigint      NOT NULL REFERENCES admins ON DELETE CASCADE,
    token_hash   bytea       NOT NULL UNIQUE,     -- храним ТОЛЬКО хэш
    user_agent   text,
    ip           inet,
    expires_at   timestamptz NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX admin_sessions_admin_idx ON admin_sessions (admin_id);

-- токены внешних интеграций (бот, сайт, метрики)
CREATE TABLE api_tokens (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        text        NOT NULL,
    prefix      text        NOT NULL,             -- показываем в UI: sn_live_9f2k…
    token_hash  bytea       NOT NULL UNIQUE,
    scopes      text[]      NOT NULL DEFAULT '{}',
    created_by  bigint      REFERENCES admins ON DELETE SET NULL,
    last_used_at timestamptz,
    revoked_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- журнал действий: кто, что, над чем
CREATE TABLE audit_log (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_kind  text        NOT NULL,             -- admin | api_token | system | bot
    actor_id    bigint,
    action      text        NOT NULL,             -- client.disable, node.restart, …
    entity_type text,
    entity_id   bigint,
    payload     jsonb       NOT NULL DEFAULT '{}',
    ip          inet,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_entity_idx  ON audit_log (entity_type, entity_id, created_at DESC);
CREATE INDEX audit_log_created_idx ON audit_log (created_at DESC);


-- ═══════════════════════════════════════════════════════════════════
--  2. КОНФИГУРАЦИИ, ИНБАУНДЫ, НОДЫ, ХОСТЫ
-- ═══════════════════════════════════════════════════════════════════

-- профиль = один конфиг движка, раздаваемый группе нод
CREATE TABLE config_profiles (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id   uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    name        text        NOT NULL UNIQUE,
    engine      engine_kind NOT NULL DEFAULT 'xray',
    config      jsonb       NOT NULL,
    version     integer     NOT NULL DEFAULT 1,   -- растёт при каждом сохранении
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER config_profiles_touch BEFORE UPDATE ON config_profiles
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- инбаунды, распарсенные из конфига профиля (денормализация ради связей)
CREATE TABLE inbounds (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    profile_id  bigint      NOT NULL REFERENCES config_profiles ON DELETE CASCADE,
    tag         text        NOT NULL,             -- vless-reality
    protocol    text        NOT NULL,             -- vless | hysteria2 | …
    network     text,                             -- tcp | ws | xhttp | udp
    security    host_security NOT NULL DEFAULT 'none',
    port        integer     NOT NULL CHECK (port BETWEEN 1 AND 65535),
    UNIQUE (profile_id, tag)
);

CREATE TABLE nodes (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id      uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    name           text        NOT NULL UNIQUE,
    country_code   char(2)     NOT NULL,
    address        text        NOT NULL,
    api_port       integer     NOT NULL DEFAULT 2222,
    profile_id     bigint      REFERENCES config_profiles ON DELETE RESTRICT,
    status         node_status NOT NULL DEFAULT 'provisioning',

    -- аутентификация агента: панель хранит только хэш секрета
    agent_secret_hash bytea    NOT NULL,
    agent_version  text,
    engine_version text,                          -- версия xray/sing-box на ноде
    last_seen_at   timestamptz,

    -- экономика
    traffic_multiplier numeric(4,2) NOT NULL DEFAULT 1.00 CHECK (traffic_multiplier >= 0),
    count_traffic  boolean     NOT NULL DEFAULT true,
    notify         boolean     NOT NULL DEFAULT true,

    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);
CREATE INDEX nodes_status_idx ON nodes (status) WHERE deleted_at IS NULL;
CREATE TRIGGER nodes_touch BEFORE UPDATE ON nodes
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- какие инбаунды реально включены на конкретной ноде
CREATE TABLE node_inbounds (
    node_id    bigint NOT NULL REFERENCES nodes ON DELETE CASCADE,
    inbound_id bigint NOT NULL REFERENCES inbounds ON DELETE CASCADE,
    PRIMARY KEY (node_id, inbound_id)
);

-- хост = строка локации в подписке клиента
CREATE TABLE hosts (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id   uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    sort_order  integer     NOT NULL DEFAULT 0,
    remark      text        NOT NULL,             -- «🇳🇱 Амстердам · Reality»
    address     text        NOT NULL,
    port        integer     NOT NULL CHECK (port BETWEEN 1 AND 65535),
    inbound_id  bigint      NOT NULL REFERENCES inbounds ON DELETE RESTRICT,
    security    host_security NOT NULL DEFAULT 'reality',
    sni         text,
    fingerprint text,
    alpn        text,
    host_header text,
    path        text,
    public_key  text,                             -- reality pbk
    short_id    text,
    extra       jsonb       NOT NULL DEFAULT '{}',-- mux, xhttp-параметры и пр.
    is_enabled  boolean     NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX hosts_order_idx ON hosts (sort_order) WHERE is_enabled;
CREATE TRIGGER hosts_touch BEFORE UPDATE ON hosts
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();


-- ═══════════════════════════════════════════════════════════════════
--  3. СКВАДЫ (группы доступа)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE squads (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id   uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    name        text        NOT NULL UNIQUE,
    description text,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE squad_inbounds (
    squad_id   bigint NOT NULL REFERENCES squads ON DELETE CASCADE,
    inbound_id bigint NOT NULL REFERENCES inbounds ON DELETE CASCADE,
    PRIMARY KEY (squad_id, inbound_id)
);

-- выдача хостов наружу (реселлеры, партнёрские боты)
CREATE TABLE external_squads (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        text        NOT NULL UNIQUE,
    token_hash  bytea       NOT NULL UNIQUE,
    template_id bigint,                           -- FK ниже, после templates
    rate_limit_per_min integer NOT NULL DEFAULT 100,
    allowed_ips inet[]      NOT NULL DEFAULT '{}',
    is_active   boolean     NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE external_squad_hosts (
    external_squad_id bigint NOT NULL REFERENCES external_squads ON DELETE CASCADE,
    host_id           bigint NOT NULL REFERENCES hosts ON DELETE CASCADE,
    PRIMARY KEY (external_squad_id, host_id)
);


-- ═══════════════════════════════════════════════════════════════════
--  4. ТАРИФЫ
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE tariffs (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id      uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    code           text        NOT NULL UNIQUE,   -- PRO, START, TRIAL
    title          text        NOT NULL,
    description    text,                          -- HTML для бота/сайта
    badge          text,                          -- «⭐ Популярный»
    icon           text,
    sort_order     integer     NOT NULL DEFAULT 0,

    device_limit   integer     NOT NULL DEFAULT 1 CHECK (device_limit > 0),
    traffic_limit_bytes bigint CHECK (traffic_limit_bytes IS NULL OR traffic_limit_bytes > 0), -- NULL = безлимит
    reset_strategy reset_strategy NOT NULL DEFAULT 'month',

    is_active      boolean     NOT NULL DEFAULT true,   -- продаётся
    is_visible     boolean     NOT NULL DEFAULT true,   -- виден в витрине
    allow_autorenew boolean    NOT NULL DEFAULT true,
    is_trial       boolean     NOT NULL DEFAULT false,

    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER tariffs_touch BEFORE UPDATE ON tariffs
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- цена за период. Разные валюты — разные строки.
CREATE TABLE tariff_prices (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tariff_id    bigint      NOT NULL REFERENCES tariffs ON DELETE CASCADE,
    period_days  integer     NOT NULL CHECK (period_days > 0),
    currency     char(3)     NOT NULL DEFAULT 'USD',
    amount_minor bigint      NOT NULL CHECK (amount_minor >= 0),  -- 499 = $4.99
    is_active    boolean     NOT NULL DEFAULT true,
    UNIQUE (tariff_id, period_days, currency)
);

CREATE TABLE tariff_squads (
    tariff_id bigint NOT NULL REFERENCES tariffs ON DELETE CASCADE,
    squad_id  bigint NOT NULL REFERENCES squads  ON DELETE CASCADE,
    PRIMARY KEY (tariff_id, squad_id)
);


-- ═══════════════════════════════════════════════════════════════════
--  5. КЛИЕНТЫ (покупатель + VPN-пользователь в одной сущности)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE clients (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id     uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    username      text        NOT NULL,

    -- VPN-идентичность
    vpn_uuid      uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,  -- уходит в конфиги
    short_id      text        NOT NULL UNIQUE,    -- в ссылке подписки: /s/fQ8yKk2c

    status        client_status NOT NULL DEFAULT 'active',
    tag           text,
    note          text,                           -- заметка админа
    locale        text        NOT NULL DEFAULT 'ru',
    referred_by   bigint,                         -- FK на partners, ниже

    last_online_at timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz
);
CREATE INDEX clients_status_idx   ON clients (status) WHERE deleted_at IS NULL;
CREATE INDEX clients_username_idx ON clients (lower(username));
CREATE INDEX clients_tag_idx      ON clients (tag) WHERE tag IS NOT NULL;
CREATE TRIGGER clients_touch BEFORE UPDATE ON clients
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- каналы входа: telegram / email. Позволяет слить веб-аккаунт и бота в одного клиента.
CREATE TABLE client_identities (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id  bigint        NOT NULL REFERENCES clients ON DELETE CASCADE,
    kind       identity_kind NOT NULL,
    value      text          NOT NULL,            -- tg id или email (в нижнем регистре)
    is_verified boolean      NOT NULL DEFAULT false,
    created_at timestamptz   NOT NULL DEFAULT now(),
    UNIQUE (kind, value)
);
CREATE INDEX client_identities_client_idx ON client_identities (client_id);

-- активная подписка. История — отдельными строками, актуальная одна.
CREATE TABLE subscriptions (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id     bigint      NOT NULL REFERENCES clients ON DELETE CASCADE,
    tariff_id     bigint      REFERENCES tariffs ON DELETE SET NULL,

    started_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz,                    -- NULL = бессрочно
    canceled_at   timestamptz,

    device_limit  integer     NOT NULL DEFAULT 1,
    traffic_limit_bytes bigint,                   -- NULL = безлимит
    traffic_used_bytes  bigint NOT NULL DEFAULT 0 CHECK (traffic_used_bytes >= 0),
    reset_strategy reset_strategy NOT NULL DEFAULT 'month',
    traffic_reset_at timestamptz,                 -- когда сбрасывать счётчик

    autorenew     boolean     NOT NULL DEFAULT false,
    is_current    boolean     NOT NULL DEFAULT true,

    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
-- у клиента ровно одна текущая подписка
CREATE UNIQUE INDEX subscriptions_one_current ON subscriptions (client_id) WHERE is_current;
CREATE INDEX subscriptions_expires_idx ON subscriptions (expires_at) WHERE is_current;
CREATE TRIGGER subscriptions_touch BEFORE UPDATE ON subscriptions
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- персональный доступ к сквадам (обычно наследуется от тарифа, но можно переопределить)
CREATE TABLE client_squads (
    client_id bigint NOT NULL REFERENCES clients ON DELETE CASCADE,
    squad_id  bigint NOT NULL REFERENCES squads  ON DELETE CASCADE,
    PRIMARY KEY (client_id, squad_id)
);

-- устройства (HWID)
CREATE TABLE devices (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id   bigint      NOT NULL REFERENCES clients ON DELETE CASCADE,
    hwid        text        NOT NULL,
    platform    text,
    model       text,
    app_version text,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (client_id, hwid)
);
CREATE INDEX devices_hwid_idx ON devices (hwid);   -- поиск шаринга: один hwid у многих клиентов


-- ═══════════════════════════════════════════════════════════════════
--  6. ДЕНЬГИ
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE promo_codes (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code          text        NOT NULL,
    kind          promo_kind  NOT NULL,
    value         integer     NOT NULL CHECK (value > 0),  -- % | минорные единицы | дни
    currency      char(3),                        -- только для kind='fixed'
    max_uses      integer     CHECK (max_uses IS NULL OR max_uses > 0),
    used_count    integer     NOT NULL DEFAULT 0,
    per_client_limit integer  NOT NULL DEFAULT 1,
    valid_from    timestamptz,
    valid_until   timestamptz,
    tariff_id     bigint      REFERENCES tariffs ON DELETE CASCADE,  -- NULL = все тарифы
    first_purchase_only boolean NOT NULL DEFAULT false,
    is_active     boolean     NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CHECK (kind <> 'fixed' OR currency IS NOT NULL)
);
CREATE UNIQUE INDEX promo_codes_code_key ON promo_codes (upper(code));

CREATE TABLE payments (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id      uuid        NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    client_id      bigint      NOT NULL REFERENCES clients ON DELETE RESTRICT,
    subscription_id bigint     REFERENCES subscriptions ON DELETE SET NULL,
    tariff_id      bigint      REFERENCES tariffs ON DELETE SET NULL,
    promo_code_id  bigint      REFERENCES promo_codes ON DELETE SET NULL,

    kind           payment_kind   NOT NULL DEFAULT 'purchase',
    status         payment_status NOT NULL DEFAULT 'pending',

    amount_minor   bigint      NOT NULL CHECK (amount_minor >= 0),
    discount_minor bigint      NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
    currency       char(3)     NOT NULL DEFAULT 'USD',
    period_days    integer,

    provider       text        NOT NULL,          -- platega | crypto_ton | tg_stars | manual
    provider_txid  text,
    error_message  text,
    -- метаданные провайдера: маска карты, сеть крипты, доп-устройства и пр.
    metadata       jsonb       NOT NULL DEFAULT '{}',

    paid_at        timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX payments_provider_txid_key ON payments (provider, provider_txid)
    WHERE provider_txid IS NOT NULL;     -- защита от двойного зачисления по вебхуку
CREATE INDEX payments_client_idx  ON payments (client_id, created_at DESC);
CREATE INDEX payments_status_idx  ON payments (status, created_at DESC);
CREATE INDEX payments_paid_idx    ON payments (paid_at DESC) WHERE status = 'success';
CREATE TRIGGER payments_touch BEFORE UPDATE ON payments
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE refunds (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    payment_id   bigint      NOT NULL REFERENCES payments ON DELETE RESTRICT,
    amount_minor bigint      NOT NULL CHECK (amount_minor > 0),
    reason       text,
    revoke_subscription boolean NOT NULL DEFAULT true,
    created_by   bigint      REFERENCES admins ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE promo_redemptions (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    promo_code_id bigint      NOT NULL REFERENCES promo_codes ON DELETE CASCADE,
    client_id     bigint      NOT NULL REFERENCES clients ON DELETE CASCADE,
    payment_id    bigint      REFERENCES payments ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX promo_redemptions_client_idx ON promo_redemptions (client_id, promo_code_id);

-- сохранённые платёжные методы для автосписания.
-- ВНИМАНИЕ: только токен провайдера, никаких PAN/CVV.
CREATE TABLE payment_methods (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id     bigint      NOT NULL REFERENCES clients ON DELETE CASCADE,
    provider      text        NOT NULL,
    provider_token text       NOT NULL,
    brand         text,                           -- visa | mastercard | mir
    last4         char(4),
    expires_at    date,
    is_default    boolean     NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payment_methods_client_idx ON payment_methods (client_id);


-- ═══════════════════════════════════════════════════════════════════
--  7. ПАРТНЁРКА
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE partners (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id     bigint      REFERENCES clients ON DELETE SET NULL,  -- NULL = внешний партнёр (канал)
    title         text        NOT NULL,
    slug          text        NOT NULL UNIQUE,    -- stealthnet.app/r/<slug>
    share_percent numeric(5,2) NOT NULL CHECK (share_percent BETWEEN 0 AND 100),
    currency      char(3)     NOT NULL DEFAULT 'USD',
    balance_minor bigint      NOT NULL DEFAULT 0,
    is_active     boolean     NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE clients
    ADD CONSTRAINT clients_referred_by_fkey
    FOREIGN KEY (referred_by) REFERENCES partners ON DELETE SET NULL;
CREATE INDEX clients_referred_by_idx ON clients (referred_by) WHERE referred_by IS NOT NULL;

-- начисление комиссии с конкретного платежа
CREATE TABLE partner_commissions (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    partner_id    bigint      NOT NULL REFERENCES partners ON DELETE CASCADE,
    payment_id    bigint      NOT NULL REFERENCES payments ON DELETE CASCADE,
    amount_minor  bigint      NOT NULL,
    currency      char(3)     NOT NULL DEFAULT 'USD',
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (payment_id)                            -- одна комиссия на платёж
);

CREATE TABLE partner_payouts (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    partner_id    bigint      NOT NULL REFERENCES partners ON DELETE CASCADE,
    amount_minor  bigint      NOT NULL CHECK (amount_minor > 0),
    currency      char(3)     NOT NULL DEFAULT 'USD',
    note          text,
    created_by    bigint      REFERENCES admins ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);


-- ═══════════════════════════════════════════════════════════════════
--  8. ПОДПИСКА: шаблоны, правила ответов, история запросов
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE subscription_templates (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code       text        NOT NULL UNIQUE,       -- xray_json | mihomo | singbox | clash | stash
    title      text        NOT NULL,
    body       text        NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE external_squads
    ADD CONSTRAINT external_squads_template_fkey
    FOREIGN KEY (template_id) REFERENCES subscription_templates ON DELETE SET NULL;

-- маршрутизация по User-Agent: какому клиенту какой формат отдать
CREATE TABLE response_rules (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sort_order  integer     NOT NULL DEFAULT 0,
    name        text        NOT NULL,
    ua_pattern  text        NOT NULL,             -- regex
    template_id bigint      REFERENCES subscription_templates ON DELETE SET NULL,
    action      text        NOT NULL DEFAULT 'template',  -- template | web_page | block
    is_active   boolean     NOT NULL DEFAULT true
);
CREATE INDEX response_rules_order_idx ON response_rules (sort_order) WHERE is_active;

-- SRH: кто, чем и когда тянул подписку. Партиционируем — растёт быстро.
CREATE TABLE subscription_requests (
    id          bigint GENERATED ALWAYS AS IDENTITY,
    client_id   bigint      NOT NULL,
    requested_at timestamptz NOT NULL DEFAULT now(),
    user_agent  text,
    ip          inet,
    matched_rule_id bigint,
    response_code text,
    PRIMARY KEY (id, requested_at)
) PARTITION BY RANGE (requested_at);

CREATE INDEX subscription_requests_client_idx ON subscription_requests (client_id, requested_at DESC);

CREATE TABLE subscription_requests_2026_08 PARTITION OF subscription_requests
    FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');
CREATE TABLE subscription_requests_2026_09 PARTITION OF subscription_requests
    FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');


-- ═══════════════════════════════════════════════════════════════════
--  9. ТРАФИК И МЕТРИКИ
-- ═══════════════════════════════════════════════════════════════════

-- потребление: клиент × нода × сутки. Основная растущая таблица.
-- 8 000 клиентов × 6 нод × 365 дней ≈ 17 млн строк в год → партиции по месяцу.
CREATE TABLE traffic_usage (
    client_id    bigint NOT NULL,
    node_id      bigint NOT NULL,
    day          date   NOT NULL,
    upload_bytes   bigint NOT NULL DEFAULT 0 CHECK (upload_bytes   >= 0),
    download_bytes bigint NOT NULL DEFAULT 0 CHECK (download_bytes >= 0),
    PRIMARY KEY (day, client_id, node_id)
) PARTITION BY RANGE (day);

CREATE TABLE traffic_usage_2026_08 PARTITION OF traffic_usage
    FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');
CREATE TABLE traffic_usage_2026_09 PARTITION OF traffic_usage
    FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');

-- агрегат по нодам на сутки — для графиков, чтобы не сканировать сырьё
CREATE TABLE node_daily_stats (
    node_id        bigint NOT NULL REFERENCES nodes ON DELETE CASCADE,
    day            date   NOT NULL,
    upload_bytes   bigint NOT NULL DEFAULT 0,
    download_bytes bigint NOT NULL DEFAULT 0,
    peak_online    integer NOT NULL DEFAULT 0,
    PRIMARY KEY (node_id, day)
);

-- снимки состояния ноды (cpu/ram/online) — короткий срок хранения
CREATE TABLE node_metrics (
    node_id     bigint      NOT NULL REFERENCES nodes ON DELETE CASCADE,
    at          timestamptz NOT NULL DEFAULT now(),
    cpu_percent real,
    ram_percent real,
    online_count integer,
    uplink_bps  bigint,
    downlink_bps bigint,
    PRIMARY KEY (node_id, at)
);


-- ═══════════════════════════════════════════════════════════════════
--  10. ИНФРА-БИЛЛИНГ (расходы на серверы)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE infra_providers (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name       text        NOT NULL UNIQUE,
    url        text,
    note       text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE node_billing (
    node_id      bigint      PRIMARY KEY REFERENCES nodes ON DELETE CASCADE,
    provider_id  bigint      REFERENCES infra_providers ON DELETE SET NULL,
    cost_minor   bigint      NOT NULL DEFAULT 0,
    currency     char(3)     NOT NULL DEFAULT 'USD',
    billing_day  integer     NOT NULL DEFAULT 1 CHECK (billing_day BETWEEN 1 AND 28),
    next_due_on  date
);

CREATE TABLE infra_payments (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    provider_id  bigint      REFERENCES infra_providers ON DELETE SET NULL,
    node_id      bigint      REFERENCES nodes ON DELETE SET NULL,
    amount_minor bigint      NOT NULL,
    currency     char(3)     NOT NULL DEFAULT 'USD',
    paid_on      date        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);


-- ═══════════════════════════════════════════════════════════════════
--  11. КОММУНИКАЦИИ
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE broadcasts (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    title         text        NOT NULL,
    body          text        NOT NULL,
    button_text   text,
    button_url    text,
    media_url     text,
    segment       jsonb       NOT NULL DEFAULT '{}',  -- условия выборки
    status        broadcast_status NOT NULL DEFAULT 'draft',
    scheduled_at  timestamptz,
    started_at    timestamptz,
    finished_at   timestamptz,
    total_count   integer     NOT NULL DEFAULT 0,
    sent_count    integer     NOT NULL DEFAULT 0,
    failed_count  integer     NOT NULL DEFAULT 0,
    created_by    bigint      REFERENCES admins ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE broadcast_deliveries (
    broadcast_id bigint      NOT NULL REFERENCES broadcasts ON DELETE CASCADE,
    client_id    bigint      NOT NULL REFERENCES clients ON DELETE CASCADE,
    sent_at      timestamptz,
    error        text,
    PRIMARY KEY (broadcast_id, client_id)
);

CREATE TABLE tickets (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id  bigint          NOT NULL REFERENCES clients ON DELETE CASCADE,
    subject    text            NOT NULL,
    status     ticket_status   NOT NULL DEFAULT 'open',
    priority   ticket_priority NOT NULL DEFAULT 'normal',
    assigned_to bigint         REFERENCES admins ON DELETE SET NULL,
    created_at timestamptz     NOT NULL DEFAULT now(),
    updated_at timestamptz     NOT NULL DEFAULT now()
);
CREATE INDEX tickets_status_idx ON tickets (status, updated_at DESC);
CREATE TRIGGER tickets_touch BEFORE UPDATE ON tickets
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE ticket_messages (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ticket_id  bigint      NOT NULL REFERENCES tickets ON DELETE CASCADE,
    author_kind text       NOT NULL,              -- client | admin
    admin_id   bigint      REFERENCES admins ON DELETE SET NULL,
    body       text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ticket_messages_ticket_idx ON ticket_messages (ticket_id, created_at);


-- ═══════════════════════════════════════════════════════════════════
--  12. НАСТРОЙКИ
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE settings (
    key        text        PRIMARY KEY,
    value      jsonb       NOT NULL,
    updated_by bigint      REFERENCES admins ON DELETE SET NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- конфиг веб-страницы подписки (кнопки приложений по платформам)
CREATE TABLE subscription_page_apps (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    platform    text        NOT NULL,             -- ios | android | windows | macos | linux
    sort_order  integer     NOT NULL DEFAULT 0,
    name        text        NOT NULL,
    deeplink    text,
    store_url   text,
    guide       text,
    is_active   boolean     NOT NULL DEFAULT true
);
CREATE INDEX subscription_page_apps_platform_idx ON subscription_page_apps (platform, sort_order);


-- ═══════════════════════════════════════════════════════════════════
--  13. ПРЕДСТАВЛЕНИЯ ДЛЯ ПАНЕЛИ
-- ═══════════════════════════════════════════════════════════════════

-- строка таблицы «Клиенты» одним запросом, без N+1
CREATE VIEW client_overview AS
SELECT
    c.id, c.public_id, c.username, c.status, c.tag, c.short_id,
    c.last_online_at, c.created_at,
    t.code                AS tariff_code,
    s.expires_at,
    s.traffic_used_bytes,
    s.traffic_limit_bytes,
    s.device_limit,
    s.autorenew,
    (SELECT count(*) FROM devices d WHERE d.client_id = c.id)      AS device_count,
    COALESCE((SELECT sum(p.amount_minor) FROM payments p
              WHERE p.client_id = c.id AND p.status = 'success'), 0) AS ltv_minor,
    (SELECT count(*) FROM payments p
      WHERE p.client_id = c.id AND p.status = 'success')            AS payment_count
FROM clients c
LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
LEFT JOIN tariffs t       ON t.id = s.tariff_id
WHERE c.deleted_at IS NULL;

-- ═══════════════════════════════════════════════════════════════════
--  Демо-данные для стенда. НЕ применять на боевой установке.
--  Пароль администратора задаётся отдельно, через утилиту создания админа.
-- ═══════════════════════════════════════════════════════════════════

BEGIN;

-- ── профили конфигураций и инбаунды ──
INSERT INTO config_profiles (name, engine, config) VALUES
('main-reality', 'xray', '{"log":{"loglevel":"warning"},"inbounds":[{"tag":"vless-reality","port":443,"protocol":"vless"},{"tag":"vless-xhttp","port":8080,"protocol":"vless"}],"outbounds":[{"protocol":"freedom","tag":"direct"}]}'),
('ws-fallback',  'xray', '{"log":{"loglevel":"warning"},"inbounds":[{"tag":"vless-ws","port":443,"protocol":"vless"}],"outbounds":[{"protocol":"freedom","tag":"direct"}]}');

-- Ссылаемся по именам, а НЕ по числовым id: последовательности в Postgres
-- не откатываются после ошибки, поэтому зашитые id ломают повторный прогон.
INSERT INTO inbounds (profile_id, tag, protocol, network, security, port)
SELECT p.id, v.tag, v.protocol, v.network, v.security::host_security, v.port
  FROM (VALUES
    ('main-reality','vless-reality','vless','tcp',  'reality', 443),
    ('main-reality','vless-xhttp',  'vless','xhttp','reality', 8080),
    ('ws-fallback', 'vless-ws',     'vless','ws',   'tls',     443)
  ) AS v(profile, tag, protocol, network, security, port)
  JOIN config_profiles p ON p.name = v.profile;

-- ── ноды ──
INSERT INTO nodes (name, country_code, address, profile_id, status, agent_secret_hash,
                   engine_version, agent_version, last_seen_at, traffic_multiplier)
SELECT v.name, v.cc, v.addr, p.id, v.status::node_status, '\x00',
       v.engine, '0.1.0', now() - (v.seen_ago || ' minutes')::interval, v.mult
  FROM (VALUES
    ('ams-edge-01','NL','45.90.12.7',  'main-reality','online', '25.7.26', 0, 1.00),
    ('fra-edge-02','DE','88.99.140.21','main-reality','online', '25.7.26', 0, 1.00),
    ('ist-edge-01','TR','185.65.204.9','ws-fallback', 'online', '25.6.8',  0, 1.50),
    ('ala-edge-01','KZ','91.147.92.30','ws-fallback', 'offline','25.7.26', 21, 1.00)
  ) AS v(name, cc, addr, profile, status, engine, seen_ago, mult)
  JOIN config_profiles p ON p.name = v.profile;

INSERT INTO node_inbounds (node_id, inbound_id)
SELECT n.id, i.id
  FROM (VALUES
    ('ams-edge-01','vless-reality'), ('ams-edge-01','vless-xhttp'),
    ('fra-edge-02','vless-reality'), ('fra-edge-02','vless-xhttp'),
    ('ist-edge-01','vless-ws'),      ('ala-edge-01','vless-ws')
  ) AS v(node, inbound)
  JOIN nodes n    ON n.name = v.node
  JOIN inbounds i ON i.tag  = v.inbound;

-- ── хосты (строки локаций в подписке) ──
INSERT INTO hosts (sort_order, remark, address, port, inbound_id, security, sni,
                   fingerprint, alpn, path, public_key, short_id, is_enabled)
SELECT v.ord, v.remark, v.addr, v.port, i.id, v.security::host_security, v.sni,
       v.fp, v.alpn, v.path, v.pbk, v.sid, true
  FROM (VALUES
    (1,'🇳🇱 Амстердам · Reality','ams.example.net',443, 'vless-reality','reality','yahoo.com',       'chrome', NULL,         NULL, 'PBK_AMS_DEMO','a1b2'),
    (2,'🇩🇪 Франкфурт · Reality','fra.example.net',443, 'vless-reality','reality','yahoo.com',       'chrome', NULL,         NULL, 'PBK_FRA_DEMO','c3d4'),
    (3,'🇳🇱 Амстердам · xHTTP',  'ams.example.net',8080,'vless-xhttp',  'reality','cdn.jsdelivr.net','firefox',NULL,         NULL, 'PBK_AMS_DEMO','e5f6'),
    (4,'🇹🇷 Стамбул · WS',       'ist.example.net',443, 'vless-ws',     'tls',    'ist.example.net', NULL,     'h2,http/1.1','/ws',NULL,          NULL)
  ) AS v(ord, remark, addr, port, inbound, security, sni, fp, alpn, path, pbk, sid)
  JOIN inbounds i ON i.tag = v.inbound;

-- ── сквады ──
INSERT INTO squads (name, description) VALUES
('Стандарт', 'Базовые локации для всех тарифов'),
('Премиум',  'Приоритетные ноды для PRO');

INSERT INTO squad_inbounds (squad_id, inbound_id)
SELECT s.id, i.id
  FROM (VALUES
    ('Стандарт','vless-reality'), ('Стандарт','vless-ws'),
    ('Премиум','vless-reality'),  ('Премиум','vless-xhttp'), ('Премиум','vless-ws')
  ) AS v(squad, inbound)
  JOIN squads s   ON s.name = v.squad
  JOIN inbounds i ON i.tag  = v.inbound;

-- ── тарифы ──
-- Описание — слова для человека, а не повтор лимитов: устройства и трафик
-- бот и кабинет подставляют сами из полей ниже, и написанное второй раз
-- выглядит в карточке дублем.
INSERT INTO tariffs (code, title, description, device_limit, traffic_limit_bytes, reset_strategy, sort_order, is_trial, badge) VALUES
('TRIAL', 'Триал', 'Попробовать без оплаты',              2, 10737418240,  'day',   1, true,  NULL),
('START', 'Start', 'Для одного телефона',                 1, 107374182400, 'month', 2, false, NULL),
('PRO',   'Pro',   'Все локации, для семьи и ноутбука',   3, 536870912000, 'month', 3, false, '⭐ Популярный');

INSERT INTO tariff_prices (tariff_id, period_days, currency, amount_minor)
SELECT t.id, v.days, 'USD', v.minor
  FROM (VALUES
    ('TRIAL',3,0),
    ('START',30,349), ('START',90,899), ('START',365,2999),
    ('PRO',30,599), ('PRO',90,1499), ('PRO',180,2690), ('PRO',365,4999)
  ) AS v(code, days, minor)
  JOIN tariffs t ON t.code = v.code;

INSERT INTO tariff_squads (tariff_id, squad_id)
SELECT t.id, s.id
  FROM (VALUES ('TRIAL','Стандарт'),('START','Стандарт'),('PRO','Стандарт'),('PRO','Премиум'))
       AS v(code, squad)
  JOIN tariffs t ON t.code = v.code
  JOIN squads  s ON s.name = v.squad;

-- ── промокоды ──
INSERT INTO promo_codes (code, kind, value, max_uses, used_count, valid_until) VALUES
('PROMO15',  'percent', 15, 500, 214, now() + interval '28 days'),
('WELCOME7', 'days',     7, NULL, 1130, NULL);

-- ── клиенты ──
-- short_id фиксированные, чтобы ссылки подписок были предсказуемы на стенде.
INSERT INTO clients (username, short_id, status, tag, note, last_online_at, created_at) VALUES
('marat_k',        'fQ8yKk2c', 'active',   'VIP',  'Пришёл с рекламы',        now() - interval '9 minutes',  now() - interval '260 days'),
('irina_s',        'zR4mNp7a', 'active',   NULL,   '',                        now() - interval '2 minutes',  now() - interval '150 days'),
('lena_travel',    'kV9sLm3d', 'active',   'TRIAL','',                        now() - interval '23 minutes', now() - interval '1 day'),
('dmitry_off',     'pH2cJn8f', 'limited',  NULL,   'Упёрся в лимит',          now() - interval '15 hours',   now() - interval '80 days'),
('olga_m',         'mW5dRk4g', 'expired',  NULL,   'Ушла после подорожания',  now() - interval '13 days',    now() - interval '210 days'),
('alex_gamer',     'qN7fTb2h', 'disabled', 'ABUSE','Отключён: торренты',      now() - interval '4 days',     now() - interval '6 days'),
('nick_freelance', 'sD3gVc6j', 'active',   'VIP',  '',                        now() - interval '1 minute',   now() - interval '420 days');

INSERT INTO client_identities (client_id, kind, value, is_verified)
SELECT c.id, v.kind::identity_kind, v.value, true
  FROM (VALUES
    ('marat_k','telegram','144001820'), ('marat_k','email','marat.k@example.com'),
    ('irina_s','telegram','98230471'),
    ('lena_travel','telegram','201455802'),
    ('dmitry_off','telegram','77284016'),
    ('olga_m','email','olga.m@example.com'),
    ('alex_gamer','telegram','88123900'),
    ('nick_freelance','telegram','66091238'), ('nick_freelance','email','nick.f@example.com')
  ) AS v(username, kind, value)
  JOIN clients c ON c.username = v.username;

INSERT INTO subscriptions (client_id, tariff_id, expires_at, device_limit,
                           traffic_limit_bytes, traffic_used_bytes, reset_strategy, autorenew)
SELECT c.id, t.id, now() + (v.days || ' days')::interval, v.devices,
       v.limit_bytes, v.used_bytes, v.strategy::reset_strategy, v.autorenew
  FROM (VALUES
    ('marat_k',       'PRO',   88,  3, 536870912000::bigint,  197779132416::bigint, 'month',    true),
    ('irina_s',       'START', 22,  1, 107374182400,           44875550720,         'month',    true),
    ('lena_travel',   'TRIAL',  3,  2,  10737418240,            2576980378,         'day',      false),
    ('dmitry_off',    'START', 16,  1, 107374182400,          107374182400,         'month',    true),
    ('olga_m',        'PRO',  -12,  3, 536870912000,                     0,         'month',    false),
    ('alex_gamer',    'START', 28,  1, 107374182400,           13636866048,         'month',    false),
    ('nick_freelance','PRO',  312,  5, NULL,                 1298443436032,         'no_reset', true)
  ) AS v(username, code, days, devices, limit_bytes, used_bytes, strategy, autorenew)
  JOIN clients c ON c.username = v.username
  JOIN tariffs t ON t.code     = v.code;

-- доступы: PRO получает оба сквада, остальные — базовый
-- Доступы наследуются от тарифа: тот же принцип, что при создании клиента в API.
INSERT INTO client_squads (client_id, squad_id)
SELECT DISTINCT s.client_id, ts.squad_id
  FROM subscriptions s
  JOIN tariff_squads ts ON ts.tariff_id = s.tariff_id
 WHERE s.is_current;

-- ── устройства ──
INSERT INTO devices (client_id, hwid, platform, model, app_version, first_seen_at, last_seen_at)
SELECT c.id, v.hwid, v.platform, v.model, v.app,
       now() - (v.first_ago || ' days')::interval,
       now() - (v.last_ago  || ' hours')::interval
  FROM (VALUES
    ('marat_k',       'A1B2-C3D4-E5F6','iOS 18.5',  'iPhone 15 Pro', 'Happ 3.5.0',   90, 1),
    ('marat_k',       'F6E5-D4C3-B2A1','macOS 15.4','MacBook Pro M3','Client 2.0.6', 50, 4),
    ('irina_s',       '11AA-22BB-33CC','Android 15','Pixel 8',       'Happ 3.4.1',  120, 1),
    ('nick_freelance','44DD-55EE-66FF','Windows 11','PC',            'Client 2.0.6',400, 1),
    ('nick_freelance','77GG-88HH-99II','iOS 18.5',  'iPad Air',      'Happ 3.5.0',  130, 72)
  ) AS v(username, hwid, platform, model, app, first_ago, last_ago)
  JOIN clients c ON c.username = v.username;

-- ── платежи ──
INSERT INTO payments (client_id, tariff_id, kind, status, amount_minor, currency,
                      period_days, provider, provider_txid, paid_at, created_at)
SELECT c.id, t.id, v.kind::payment_kind, v.status::payment_status, v.minor, 'USD',
       v.days, v.provider, v.txid,
       CASE WHEN v.status = 'success' THEN now() - (v.ago || ' hours')::interval END,
       now() - (v.ago || ' hours')::interval
  FROM (VALUES
    ('marat_k',       'PRO',  'purchase',    'success',1499, 90, 'platega',   'plg_9f83ha72',   2),
    ('irina_s',       'START','autorenewal', 'success', 349, 30, 'platega',   'plg_2k38dm11',   5),
    ('dmitry_off',    'START','purchase',    'success', 349, 30, 'crypto_ton','ton_88ka02mz',   9),
    ('nick_freelance','PRO',  'renewal',     'success',2690,180, 'platega',   'plg_1m99xc45',  24),
    ('marat_k',       'PRO',  'purchase',    'success', 599, 30, 'platega',   'plg_aa11bb22',  96),
    ('nick_freelance','PRO',  'renewal',     'success',4999,365, 'platega',   'plg_cc33dd44', 480),
    ('irina_s',       'START','autorenewal', 'success', 349, 30, 'platega',   'plg_ee55ff66', 768),
    ('olga_m',        'PRO',  'autorenewal', 'failed',  599, 30, 'platega',   'plg_0x11vv93', 312)
  ) AS v(username, code, kind, status, minor, days, provider, txid, ago)
  JOIN clients c ON c.username = v.username
  JOIN tariffs t ON t.code     = v.code;

UPDATE payments SET error_message = 'Недостаточно средств (код 51)' WHERE status = 'failed';

-- ── трафик за последние 14 суток ──
-- Раскидываем по клиентам и нодам, чтобы графики и «Статистика нод» были живыми.
INSERT INTO traffic_usage (client_id, node_id, day, upload_bytes, download_bytes)
SELECT c.id,
       n.id,
       d::date,
       (random() * 500000000)::bigint,
       (random() * 4000000000)::bigint
  FROM generate_series(current_date - 13, current_date, '1 day') d
 CROSS JOIN (SELECT id FROM clients WHERE status = 'active') c
 CROSS JOIN (SELECT id FROM nodes WHERE status = 'online') n
ON CONFLICT DO NOTHING;

-- ── метрики нод ──
INSERT INTO node_metrics (node_id, at, cpu_percent, ram_percent, online_count, uplink_bps, downlink_bps)
SELECT n.id, now(), v.cpu, v.ram, v.online, v.up, v.down
  FROM (VALUES
    ('ams-edge-01',34,41,4214,420000000::bigint,2100000000::bigint),
    ('fra-edge-02',48,55,3892,380000000,1900000000),
    ('ist-edge-01',77,69,1655,210000000, 980000000),
    ('ala-edge-01', 0, 0,   0,        0,          0)
  ) AS v(node, cpu, ram, online, up, down)
  JOIN nodes n ON n.name = v.node;

-- ── инфра-биллинг ──
INSERT INTO infra_providers (name, url) VALUES ('Hetzner','hetzner.com'), ('Aeza','aeza.net'), ('PS.kz','ps.kz');
INSERT INTO node_billing (node_id, provider_id, cost_minor, currency, billing_day, next_due_on)
SELECT n.id, p.id, v.minor, 'USD', v.day, current_date + v.due
  FROM (VALUES
    ('ams-edge-01','Aeza',   2800, 5,  3),
    ('fra-edge-02','Hetzner',3400, 11, 9),
    ('ist-edge-01','Hetzner',2200, 1, 28),
    ('ala-edge-01','PS.kz',  1800, 25,23)
  ) AS v(node, provider, minor, day, due)
  JOIN nodes n           ON n.name = v.node
  JOIN infra_providers p ON p.name = v.provider;

-- ── шаблоны и правила ответов ──
INSERT INTO subscription_templates (code, title, body) VALUES
('xray_json', 'Xray JSON', '{"remarks":"{{TITLE}}","outbounds":[]}'),
('clash',     'Clash / mihomo', 'proxies: []'),
('singbox',   'sing-box', '{"outbounds":[]}');

INSERT INTO response_rules (sort_order, name, ua_pattern, template_id, action)
SELECT v.ord, v.name, v.pattern, t.id, v.action
  FROM (VALUES
    (1,'Happ / Streisand',  'happ|streisand|v2box','xray_json','template'),
    (2,'Clash-ядра',        'clash|mihomo|stash',  'clash',    'template'),
    (3,'sing-box / Hiddify','sing-box|hiddify',    'singbox',  'template'),
    (4,'Браузеры',          'mozilla',             NULL,       'web_page')
  ) AS v(ord, name, pattern, tpl, action)
  LEFT JOIN subscription_templates t ON t.code = v.tpl;

-- Приложения страницы подписки устанавливаются миграцией 048 и не требуют demo seed.

-- ── настройки ──
INSERT INTO settings (key, value) VALUES
('brand.name',           '"STEALTHNET"'),
('brand.accent',         '"#9FE870"'),
('subscription.title',   '"STEALTHNET VPN"'),
('subscription.update_interval_hours', '12'),
('support.url',          '"https://t.me/example_support"');

COMMIT;

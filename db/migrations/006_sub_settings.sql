-- ═══════════════════════════════════════════════════════════════════
--  Настройки страницы подписки.
--  Ссылка на поддержку/бота задаётся администратором: у кого-то это
--  Telegram-бот, у кого-то чат поддержки, у кого-то сайт.
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO settings (key, value) VALUES
    ('subscription.title',       '"Подключение"'),
    ('subscription.support_url', '""'),
    ('subscription.support_text','"Написать в поддержку"'),
    ('subscription.bot_url',     '""'),
    ('subscription.footer_note', '""')
ON CONFLICT (key) DO NOTHING;

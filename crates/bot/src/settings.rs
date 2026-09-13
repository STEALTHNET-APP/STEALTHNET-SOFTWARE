//! Настройки бота из панели.
//!
//! Всё, что видит человек в боте, задаётся администратором: тексты,
//! картинки, ссылка на поддержку, обязательный канал. Зашивать это в код
//! означало бы правку и пересборку ради смены одной фразы.
//!
//! Один снимок читается из БД на каждый апдейт. После сохранения в панели
//! следующая команда сразу видит новую версию, без окна устаревшего кэша.

use std::collections::HashMap;
use serde_json::Value;
use sn_core::{Pool, Result};

/// Экраны бота. У каждого может быть своя картинка.
///
/// Отдельный тип, а не строка, чтобы опечатка в имени экрана не
/// превращалась молча в «картинки нет».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Buy,
    Sub,
    Docs,
    Tickets,
}

impl Screen {
    fn key(self) -> &'static str {
        match self {
            Screen::Menu => "bot.image.menu",
            Screen::Buy => "bot.image.buy",
            Screen::Sub => "bot.image.sub",
            Screen::Docs => "bot.image.docs",
            Screen::Tickets => "bot.image.tickets",
        }
    }

    /// Все экраны — панель перечисляет их в настройках.
    pub const ALL: [Screen; 5] =
        [Screen::Menu, Screen::Buy, Screen::Sub, Screen::Docs, Screen::Tickets];
}

#[derive(Clone, Default)]
pub struct BotSettings;

impl BotSettings {
    /// Согласованный снимок для одного апдейта, без кэширования между командами.
    pub async fn load(&self, pool: &Pool) -> Result<View> {
        // URL Mini App выводится только из работающего установленного кабинета.
        let rows: Vec<(String, Value)> = sqlx::query_as(
            "SELECT key, value FROM settings
              WHERE key LIKE 'bot.%' OR key = 'panel.public_url'",
        )
        .fetch_all(pool)
        .await?;

        let mut map: std::collections::HashMap<String,Value> = rows.into_iter().collect();
        if let Some(url)=sn_core::cabinet::miniapp_url(pool).await? {map.insert("_cabinet.miniapp_url".into(),Value::String(url));}
        Ok(View { map })
    }
}

/// Ссылка на документ: подпись кнопки и адрес.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocLink {
    pub label: String,
    pub url: String,
}

/// Снимок настроек на время обработки одного апдейта.
pub struct View {
    map: HashMap<String, Value>,
}

impl View {
    /// Снимок из пар — для проверок, которым не нужна база.
    #[cfg(test)]
    pub fn from_pairs(pairs: &[(&str, Value)]) -> Self {
        Self { map: pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    }
}

impl View {
    pub fn menu_buttons(&self) -> Vec<sn_core::bot_config::MenuButton> {
        sn_core::bot_config::menu_buttons(self.map.get("bot.menu_layout"))
    }
    pub fn custom(&self, key: &str, fallback: &str) -> String { self.text(key).unwrap_or_else(|| fallback.to_string()) }
    fn text(&self, key: &str) -> Option<String> {
        self.map
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    fn flag(&self, key: &str, default: bool) -> bool {
        self.map.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    /// Картинка для экрана: своя, иначе общая, иначе никакой.
    ///
    /// Так администратор может обойтись одной картинкой на весь бот и не
    /// заполнять пять полей ради одного и того же адреса.
    pub fn image(&self, screen: Screen) -> Option<String> {
        self.text(screen.key()).or_else(|| self.text("bot.image.default"))
    }

    /// Приветствие на /start. Пусто — показываем сразу меню.
    pub fn welcome(&self) -> Option<String> {
        self.text("bot.welcome_text")
    }

    /// Показывать приветствие каждый раз или только при первом заходе.
    pub fn welcome_always(&self) -> bool {
        self.flag("bot.welcome_always", false)
    }

    /// Ссылка на поддержку. Задана — в меню появляется кнопка.
    pub fn support_url(&self) -> Option<String> {
        self.text("bot.support_url")
    }

    /// Подпись кнопки поддержки.
    pub fn support_label(&self) -> String {
        self.text("bot.support_label")
            .unwrap_or_else(|| "💬 Связь с поддержкой".into())
    }

    /// Обращения из бота. Выключены — кнопки нет.
    pub fn tickets_enabled(&self) -> bool {
        self.flag("bot.tickets_enabled", true)
    }

    /// Канал, на который нужно подписаться. Пусто — проверки нет вовсе.
    ///
    /// Хранится как `@name` или числовой id. Бот должен состоять в канале
    /// администратором, иначе Telegram не даст проверить участника.
    pub fn required_channel(&self) -> Option<String> {
        self.text("bot.require_channel")
    }

    /// Ссылка на канал для кнопки. Из `@name` собирается сама.
    pub fn channel_url(&self) -> Option<String> {
        if let Some(u) = self.text("bot.channel_url") {
            return Some(u);
        }
        let ch = self.required_channel()?;
        ch.strip_prefix('@').map(|n| format!("https://t.me/{n}"))
    }

    pub fn channel_text(&self) -> String {
        self.text("bot.channel_text").unwrap_or_else(|| {
            "Чтобы пользоваться ботом, подпишитесь на наш канал.".into()
        })
    }

    /// Текст над списком документов.
    pub fn docs_text(&self) -> String {
        self.text("bot.docs_text")
            .unwrap_or_else(|| "<b>Документы</b>\n\nУсловия использования сервиса.".into())
    }

    /// Ссылки на документы: оферта, договор, что угодно ещё.
    ///
    /// Хранятся одним списком, а не набором отдельных ключей: заранее
    /// неизвестно, сколько их понадобится, и добавление третьего не
    /// должно требовать правки кода.
    ///
    /// Записи без подписи или без адреса пропускаем: кнопка без адреса
    /// не откроется, а Telegram откажется принять всю клавиатуру целиком.
    pub fn docs_links(&self) -> Vec<DocLink> {
        self.map
            .get("bot.docs_links")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        let label = d["label"].as_str()?.trim();
                        let url = d["url"].as_str()?.trim();
                        if label.is_empty() || !url.starts_with("http") {
                            return None;
                        }
                        Some(DocLink { label: label.to_string(), url: url.to_string() })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Текст под кнопками главного меню — объявление, акция, что угодно.
    pub fn menu_note(&self) -> Option<String> {
        self.text("bot.menu_note")
    }

    /// «Пригласить друга». Выключено — кнопки нет.
    ///
    /// По умолчанию выключено: программа означает выплаты, и включать её
    /// молча за администратора нельзя.
    pub fn referral_enabled(&self) -> bool {
        self.flag("bot.referral_enabled", false)
    }

    /// Сколько процентов с оплат приведённого получает пригласивший.
    ///
    /// Ставка общая для всех, кто завёл ссылку из бота. Отдельные
    /// договорённости задаются в панели и отсюда не перетираются.
    pub fn referral_percent(&self) -> f64 {
        self.map
            .get("bot.referral_percent")
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
            .filter(|p| (0.0..=100.0).contains(p))
            .unwrap_or(10.0)
    }

    /// Мини-приложение в меню. Выключено — кнопки нет вовсе.
    ///
    /// По умолчанию выключено: страница подписки должна быть на https и
    /// открываться без входа, а на свежей установке это ещё не так.
    pub fn miniapp_enabled(&self) -> bool {
        self.flag("bot.miniapp_enabled", false)
    }

    /// Подпись кнопки мини-приложения.
    pub fn miniapp_label(&self) -> String {
        self.text("bot.miniapp_label").unwrap_or_else(|| "🚀 Открыть приложение".into())
    }

    /// Публичный адрес панели.
    ///
    /// Не путать с `PANEL_URL` из окружения: там внутренний адрес, по
    /// которому бот ходит в API, — обычно `127.0.0.1`. Кнопку в Telegram
    /// из него не собрать: по такому адресу клиент никуда не попадёт, да
    /// и https там нет.
    pub fn panel_public_url(&self) -> Option<String> {
        self.text("panel.public_url")
    }

    /// URL установленного кабинета, вычисленный при загрузке настроек.
    pub fn miniapp_url(&self) -> Option<String> { self.text("_cabinet.miniapp_url") }

    /// Кнопка «Подключиться»: открывает страницу подписки прямо в
    /// Telegram. Это не кабинет — это готовые ссылки и инструкции.
    pub fn connect_inline(&self) -> bool {
        self.flag("bot.connect_inline", true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(pairs: &[(&str, Value)]) -> View {
        View::from_pairs(pairs)
    }

    #[test]
    fn картинка_экрана_падает_на_общую() {
        let v = view(&[("bot.image.default", json!("общая.jpg"))]);
        assert_eq!(v.image(Screen::Menu).as_deref(), Some("общая.jpg"));

        let v = view(&[
            ("bot.image.default", json!("общая.jpg")),
            ("bot.image.menu", json!("меню.jpg")),
        ]);
        assert_eq!(v.image(Screen::Menu).as_deref(), Some("меню.jpg"));
        assert_eq!(v.image(Screen::Buy).as_deref(), Some("общая.jpg"));
    }

    #[test]
    fn пустая_строка_это_не_значение() {
        // Иначе очищенное поле в панели давало бы попытку отправить
        // картинку с пустым адресом — и сообщение не уходило бы вовсе.
        let v = view(&[("bot.image.default", json!("   "))]);
        assert_eq!(v.image(Screen::Menu), None);
        let v = view(&[("bot.support_url", json!(""))]);
        assert_eq!(v.support_url(), None);
    }

    #[test]
    fn ссылка_на_канал_выводится_из_имени() {
        let v = view(&[("bot.require_channel", json!("@stealthnet"))]);
        assert_eq!(v.channel_url().as_deref(), Some("https://t.me/stealthnet"));

        // Числовой id в ссылку не превратить — админ задаёт её сам.
        let v = view(&[("bot.require_channel", json!("-1001234567890"))]);
        assert_eq!(v.channel_url(), None);

        // Явная ссылка сильнее выведенной.
        let v = view(&[
            ("bot.require_channel", json!("@stealthnet")),
            ("bot.channel_url", json!("https://t.me/+abcdef")),
        ]);
        assert_eq!(v.channel_url().as_deref(), Some("https://t.me/+abcdef"));
    }

    #[test]
    fn panel_and_legacy_urls_never_enable_miniapp() {
        let v=view(&[("panel.public_url",json!("https://panel.example.com")),("bot.miniapp_url",json!("https://old.example.com/app/"))]);
        assert_eq!(v.miniapp_url(),None);
        let v=view(&[("_cabinet.miniapp_url",json!("https://customer.example.com/app/"))]);
        assert_eq!(v.miniapp_url().as_deref(),Some("https://customer.example.com/app/"));
    }

    #[test]
    fn умолчания_разумны() {
        let v = view(&[]);
        assert!(v.tickets_enabled(), "обращения включены, пока не выключили");
        assert!(!v.welcome_always(), "приветствие не навязываем на каждый /start");
        assert_eq!(v.required_channel(), None, "без канала проверки нет");
        assert!(v.support_label().contains("поддержк"));
    }
}

#[cfg(test)]
mod docs_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ссылки_читаются_списком() {
        let v = View::from_pairs(&[("bot.docs_links", json!([
            { "label": "Оферта",  "url": "https://example.com/offer" },
            { "label": "Договор", "url": "https://example.com/agreement" },
        ]))]);
        let links = v.docs_links();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].label, "Оферта");
        assert_eq!(links[1].url, "https://example.com/agreement");
    }

    #[test]
    fn негодные_записи_отбрасываются() {
        // Кнопка без адреса не откроется, а Telegram откажется принять
        // всю клавиатуру целиком — и экран не покажется вовсе.
        let v = View::from_pairs(&[("bot.docs_links", json!([
            { "label": "",        "url": "https://example.com/a" },
            { "label": "Пустая",  "url": "" },
            { "label": "Не адрес","url": "example.com/b" },
            { "label": "Годная",  "url": "https://example.com/c" },
        ]))]);
        let links = v.docs_links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].label, "Годная");
    }

    #[test]
    fn без_настройки_список_пуст() {
        assert!(View::from_pairs(&[]).docs_links().is_empty());
        // Мусор вместо списка не должен ронять экран.
        let v = View::from_pairs(&[("bot.docs_links", json!("строка"))]);
        assert!(v.docs_links().is_empty());
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    #[ignore = "requires isolated SN_BOT_SETTINGS_TEST_DB"]
    async fn next_update_sees_saved_menu_and_emoji_without_waiting() {
        let url = std::env::var("SN_BOT_SETTINGS_TEST_DB").expect("isolated database URL");
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
        assert!(database.starts_with("sn_audit_"), "use an isolated audit database");
        // Temp table shadows public settings on the single test connection.
        sqlx::query("CREATE TEMP TABLE settings (key text PRIMARY KEY, value jsonb)").execute(&pool).await.unwrap();
        let before = json!([{"id":"purchase"}]);
        sqlx::query("INSERT INTO settings VALUES ('bot.menu_layout',$1)").bind(before).execute(&pool).await.unwrap();
        let settings = BotSettings::default();
        let old = settings.load(&pool).await.unwrap();
        assert_eq!(old.menu_buttons()[0].id, "purchase");
        let after = json!([{"id":"link_channel","label":"Канал","url":"https://t.me/example","icon_custom_emoji_id":"[5215361191051798408]"}]);
        sqlx::query("UPDATE settings SET value=$1 WHERE key='bot.menu_layout'").bind(after).execute(&pool).await.unwrap();
        let fresh = settings.load(&pool).await.unwrap();
        let entries = fresh.menu_buttons();
        assert_eq!(entries[0].id, "link_channel");
        assert_eq!(entries[0].icon_custom_emoji_id.as_deref(), Some("5215361191051798408"));
        // The in-flight update retains its own snapshot, while the next sees edits.
        assert_eq!(old.menu_buttons()[0].id, "purchase");
        sqlx::query("UPDATE settings SET value='[]'::jsonb WHERE key='bot.menu_layout'").execute(&pool).await.unwrap();
        assert_eq!(settings.load(&pool).await.unwrap().menu_buttons()[0].id, "miniapp");
        pool.close().await;
    }
}

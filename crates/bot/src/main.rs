//! Telegram-бот: витрина тарифов, покупка, продление, своя подписка.
//!
//! Бот — тонкий клиент к тем же таблицам, что и панель. Он не знает,
//! как устроены платёжные системы: спрашивает у реестра модулей, чем
//! можно заплатить в нужной валюте, и показывает эти способы кнопками.

mod apps;
mod promo;
mod referral;
mod settings;
mod addons;
mod tg;
mod tickets;

use serde_json::Value;
use sn_core::{money, Config, Error, Pool, Result};
use sn_payments::Registry;
use sqlx::Row;
use tg::{keyboard, Btn, Tg};

const OFFSET_KEY: &str = "updates_offset";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        // sn_core здесь обязателен: именно там логируются ошибки БД.
        // Без него «внутренняя ошибка» в ответе не имеет следа в журнале.
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sn_bot=info,sn_core=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let token = std::env::var("BOT_TOKEN")
        .map_err(|_| Error::Internal("не задан BOT_TOKEN".into()))?;

    let pool = sn_core::db::connect(&config.database_url).await?;
    let registry = Registry::from_env();
    registry.sync_to_db(&pool).await?;

    // Настройки бота живут в панели: тексты, картинки, поддержка, канал.
    let bot_settings = settings::BotSettings::default();

    let tg = Tg::new(token);
    let me = tg.get_me().await?;
    let bot_username = me["username"].as_str().unwrap_or_default().to_string();
    tracing::info!("бот @{bot_username} запущен");

    // Имя бота кладём в настройки: кабинет собирает из него ссылку
    // приглашения, а спрашивать Telegram на каждый запрос кабинета —
    // лишний поход наружу ради значения, которое почти не меняется.
    if !bot_username.is_empty() {
        let _ = sqlx::query(
            "INSERT INTO settings (key, value, updated_at)
             VALUES ('bot.username', to_jsonb($1::text), now())
             ON CONFLICT (key) DO UPDATE
                SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(&bot_username)
        .execute(&pool)
        .await;
    }

    tg.set_commands(&[
        ("start", "Главное меню"),
        ("buy", "Купить или продлить"),
        ("sub", "Моя подписка"),
        ("docs", "Документы"),
        ("support", "Поддержка"),
        ("paysupport", "Помощь с оплатой"),
        ("cancel", "Отменить ввод"),
    ])
    .await
    .ok();

    // Смещение храним в БД: после перезапуска бот не переобрабатывает
    // старые апдейты и не шлёт людям дубликаты сообщений.
    let mut offset: i64 = sqlx::query_scalar("SELECT value FROM bot_state WHERE key = $1")
        .bind(OFFSET_KEY)
        .fetch_optional(&pool)
        .await?
        .unwrap_or(0);

    loop {
        match tg.get_updates(offset).await {
            Ok(updates) => {
                sqlx::query("INSERT INTO bot_state(key,value) VALUES('poll_heartbeat',$1) ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value").bind(chrono::Utc::now().timestamp()).execute(&pool).await.ok();
                for u in updates {
                    let update_id = u["update_id"].as_i64().unwrap_or(0);
                    offset = update_id + 1;

                    if let Err(e) = handle(&tg, &pool, &registry, &config, &bot_settings, &u).await {
                        tracing::warn!(update = update_id, error = %e, "апдейт не обработан");
                    }

                    let _ = sqlx::query(
                        "INSERT INTO bot_state (key, value) VALUES ($1, $2)
                         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
                    )
                    .bind(OFFSET_KEY)
                    .bind(offset)
                    .execute(&pool)
                    .await;
                }
            }
            Err(e) => {
                // Сеть моргнула — не выходим, ждём и пробуем снова.
                tracing::warn!(error = %e, "getUpdates не удался");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

/// Всё, что нужно обработчику экрана.
///
/// Раньше по цепочке передавали tg, pool, reg и cfg по отдельности, и
/// добавление настроек означало правку сигнатуры каждой функции. Теперь
/// это один пакет, и настройки читаются ровно один раз на апдейт.
pub struct Ctx<'a> {
    pub tg: &'a Tg,
    pub pool: &'a Pool,
    pub reg: &'a Registry,
    pub cfg: &'a Config,
    pub s: settings::View,
}

async fn handle(
    tg: &Tg,
    pool: &Pool,
    reg: &Registry,
    cfg: &Config,
    bot: &settings::BotSettings,
    u: &Value,
) -> Result<()> {
    // Подтверждение перед списанием Stars: ответить надо за 10 секунд.
    if let Some(pcq) = u.get("pre_checkout_query").filter(|v| !v.is_null()) {
        let id = pcq["id"].as_str().unwrap_or_default();
        let accepted = sn_payments::providers::stars::validate_checkout(pool, pcq).await.unwrap_or(false);
        tg.answer_pre_checkout(id, accepted, if accepted {None} else {Some("Этот счёт недействителен или уже оплачен. Создайте новый счёт в боте.")}).await?;
        return Ok(());
    }

    let ctx = Ctx { tg, pool, reg, cfg, s: bot.load(pool).await? };

    if let Some(msg) = u.get("message").filter(|v| !v.is_null()) {
        if matches!(msg["chat"]["type"].as_str(),Some("group"|"supergroup")) && msg["from"]["is_bot"]!=true {
            let command=msg["text"].as_str().unwrap_or("").split_whitespace().next().unwrap_or("");
            if command.split('@').next()==Some("/chatid") {
                tg.send_chat_id(msg["chat"]["id"].as_i64().unwrap_or(0),msg["message_thread_id"].as_i64()).await?;
            }
            return Ok(());
        }
        if msg["chat"]["type"] != "private" || msg["from"]["is_bot"] == true { return Ok(()); }
        let chat = msg["chat"]["id"].as_i64().unwrap_or(0);
        let user = msg["from"]["id"].as_i64().unwrap_or(0);
        // Успешную оплату пропускаем мимо проверки канала: деньги уже
        // списаны, и не выдать за них доступ было бы хуже всего.
        if msg["successful_payment"].is_null() && gate_channel(&ctx, chat, user, None).await? {
            return Ok(());
        }
        return on_message(&ctx, msg).await;
    }
    if let Some(cq) = u.get("callback_query").filter(|v| !v.is_null()) {
        if cq["message"]["chat"]["type"] != "private" {return Ok(());}
        tg.answer_callback(cq["id"].as_str().unwrap_or_default(),None).await.ok();
        let chat = cq["message"]["chat"]["id"].as_i64().unwrap_or(0);
        let mid = cq["message"]["message_id"].as_i64().unwrap_or(0);
        let user = cq["from"]["id"].as_i64().unwrap_or(0);
        // «Я подписался» — единственная кнопка, доступная за стеной.
        let recheck = cq["data"].as_str() == Some("subchk");
        if gate_channel(&ctx, chat, user, Some(mid)).await? {
            if recheck {
                tg.answer_callback(cq["id"].as_str().unwrap_or_default(),
                                   Some("Подписка на канал не найдена")).await.ok();
            }
            return Ok(());
        }
        if recheck {
            tg.answer_callback(cq["id"].as_str().unwrap_or_default(), Some("Спасибо!")).await.ok();
            let client_id = ensure_client(pool, &cq["from"]).await?;
    sqlx::query("DELETE FROM bot_await WHERE client_id=$1").bind(client_id).execute(pool).await?;
            return show_menu(&ctx, chat, client_id, Some(mid)).await;
        }
        return on_callback(&ctx, cq).await;
    }
    Ok(())
}

/// Стена подписки на канал.
///
/// Возвращает `true`, если человека дальше пускать нельзя — тогда ему уже
/// показан экран с кнопками на канал и «Я подписался».
///
/// Канал не задан — проверки нет вовсе: это должно быть выключено по
/// умолчанию, иначе первая же установка встретит клиентов стеной.
///
/// Отказ Telegram (бота убрали из администраторов, канал переименовали)
/// трактуем как «пускать»: сломанная настройка не должна оставлять
/// работающий сервис без единого клиента. Причина уходит в журнал.
async fn gate_channel(c: &Ctx<'_>, chat: i64, user_id: i64, mid: Option<i64>) -> Result<bool> {
    let Some(channel) = c.s.required_channel() else {
        return Ok(false);
    };
    if user_id == 0 {
        return Ok(false);
    }

    match c.tg.is_member(&channel, user_id).await {
        Ok(true) => Ok(false),
        Ok(false) => {
            let mut rows = Vec::new();
            if let Some(url) = c.s.channel_url() {
                rows.push(vec![Btn::Url("📢 Открыть канал".into(), url)]);
            }
            rows.push(vec![Btn::Data("✅ Я подписался".into(), "subchk".into())]);
            c.tg.screen(chat, mid, &c.s.channel_text(),
                        c.s.image(settings::Screen::Menu).as_deref(), Some(keyboard(rows)))
                .await?;
            Ok(true)
        }
        Err(e) => {
            tracing::warn!(error = %e, channel,
                "не проверил подписку на канал — пускаем, чтобы не отрезать всех");
            Ok(false)
        }
    }
}

// ─────────────────────────── клиенты ───────────────────────────

/// Находит клиента по Telegram ID или заводит нового.
/// Логин делаем из username, но он может отсутствовать — тогда берём id.
async fn ensure_client(pool: &Pool, from: &Value) -> Result<i64> {
    sn_core::telegram_client::ensure(pool,from["id"].as_i64().unwrap_or(0),from["username"].as_str()).await
}

#[cfg(test)]
fn short_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyzACDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    (0..8)
        .map(|_| {
            n = n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            A[(n >> 33) as usize % A.len()] as char
        })
        .collect()
}

// ─────────────────────────── экраны ───────────────────────────

struct SubInfo {
    status: String,
    expires: Option<chrono::DateTime<chrono::Utc>>,
    used: i64,
    limit: Option<i64>,
    short_id: String,
    /// Продлевать ли автоматически. Клиент управляет этим сам: списание
    /// без ведома человека — верный способ получить возврат и жалобу.
    autorenew: bool,
    tariff: Option<String>,
}

async fn sub_info(pool: &Pool, client_id: i64) -> Result<SubInfo> {
    let r = sqlx::query(
        "SELECT c.status::text AS status, c.short_id, COALESCE(s.autorenew, false) AS autorenew,
                s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes,
                t.title AS tariff
           FROM clients c
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
           LEFT JOIN tariffs t ON t.id = s.tariff_id
          WHERE c.id = $1",
    )
    .bind(client_id)
    .fetch_one(pool)
    .await?;

    Ok(SubInfo {
        status: r.get("status"),
        expires: r.try_get("expires_at").unwrap_or(None),
        used: r.try_get::<Option<i64>, _>("traffic_used_bytes").unwrap_or(None).unwrap_or(0),
        limit: r.try_get::<Option<i64>, _>("traffic_limit_bytes").unwrap_or(None),
        short_id: r.get("short_id"),
        autorenew: r.get("autorenew"),
        tariff: r.try_get("tariff").unwrap_or(None),
    })
}

fn main_menu(
    brand: &str,
    info: &SubInfo,
    st: &settings::View,
    sub_base: &str,
    _panel_base: Option<&str>,
) -> (String, Value) {
    let brand=html_escape(&st.custom("bot.brand_name",brand));
    let active = info.status == "active";
    let head = if active {
        let until = info
            .expires
            .map(|e| e.format("%d.%m.%Y").to_string())
            .unwrap_or_else(|| "бессрочно".into());
        let limit = info
            .limit
            .map(money::format_bytes)
            .unwrap_or_else(|| "безлимит".into());
        format!(
            "<b>{brand}</b>\n\n✅ Подписка активна\nТариф: <b>{}</b>\nДействует до: <b>{until}</b>\nТрафик: {} из {limit}",
            html_escape(&info.tariff.clone().unwrap_or_else(|| "—".into())),
            money::format_bytes(info.used),
        )
    } else {
        format!("<b>{brand}</b>\n\nУ вас нет активной подписки.\nВыберите тариф — займёт минуту.")
    };

    // Объявление под меню: акция, предупреждение о работах, что угодно.
    let head = match st.menu_note() {
        Some(note) => format!("{head}\n\n{note}"),
        None => head,
    };

    // Две разные кнопки, и путать их не надо.
    //
    // Кабинет — это состояние подписки, витрина и оплата: он полезен и
    // тому, кто ещё ничего не купил. «Подключиться» — готовая ссылка для
    // приложения VPN, и до покупки её попросту нет.
    let mut rows: Vec<Vec<Btn>> = Vec::new();
    for entry in st.menu_buttons().into_iter().filter(|e| e.enabled) {
        let button = match entry.id.as_str() {
            "miniapp" if st.miniapp_enabled() => st.miniapp_url().filter(|url| sn_core::bot_config::web_url(url, true))
              .map(|url| Btn::WebApp(st.miniapp_label(), url)),
            "connect" if active && st.connect_inline() => {
                let url = format!("{}/{}", sub_base.trim_end_matches('/'), info.short_id);
                sn_core::bot_config::web_url(&url, true)
                    .then(|| Btn::WebApp(st.custom("bot.button.connect", "🔌 Подключиться"), url))
            },
            "purchase" => Some(Btn::Data(
                if active { st.custom("bot.button.renew", "🔄 Продлить") } else { st.custom("bot.button.buy", "🚀 Купить доступ") }, "buy".into())),
            "addons" => Some(Btn::Data(st.custom("bot.button.addons", "Докупки трафика и устройств"), "addons".into())),
            "subscription" if active => Some(Btn::Data(st.custom("bot.button.subscription", "🔑 Моя подписка"), "sub".into())),
            "referral" if st.referral_enabled() => Some(Btn::Data(st.custom("bot.button.referral", "🎁 Пригласить друга"), "ref".into())),
            "tickets" if st.tickets_enabled() => Some(Btn::Data(st.custom("bot.button.tickets", "✉️ Мои обращения"), "tickets".into())),
            "docs" => Some(Btn::Data(st.custom("bot.button.docs", "📄 Документы"), "docs".into())),
            "support" => st.support_url().map(|url| Btn::Url(st.support_label(), url)),
            id if id.starts_with("link_") => entry.label.zip(entry.url).map(|(label, url)| Btn::Url(label.trim().into(), url)),
            _ => None,
        };
        if let Some(button) = button { rows.push(vec![button.with_icon(entry.icon_custom_emoji_id.as_deref())]); }
    }

    (head, keyboard(rows))
}

async fn show_menu(c: &Ctx<'_>, chat: i64, client_id: i64, edit: Option<i64>) -> Result<()> {
    // Распаковка контекста: тела экранов писались до его появления
    // и обращаются к этим четырём по именам.
    let (tg, pool, cfg, reg) = (c.tg, c.pool, c.cfg, c.reg);
    let _ = (cfg, reg);

    sn_core::addons::refresh(pool,client_id).await?;
    let info = sub_info(pool, client_id).await?;
    let (text, mut kb) = main_menu(&cfg.brand_name, &info, &c.s, &cfg.sub_public_url,
                               Some(cfg.panel_url.as_str()));
    let catalog=sn_core::addons::catalog(pool,client_id).await?;
    if catalog["items"].as_array().is_none_or(|a|a.is_empty()) && catalog["grants"].as_array().is_none_or(|a|a.is_empty()) {
        if let Some(rows)=kb["inline_keyboard"].as_array_mut(){rows.retain(|row|!row.as_array().is_some_and(|buttons|buttons.iter().any(|b|b["callback_data"]=="addons")));}
    }
    tg.screen(chat, edit, &text, c.s.image(settings::Screen::Menu).as_deref(), Some(kb))
        .await?;
    Ok(())
}

/// Витрина: тарифы с ценами. Показываем только те, что продаются.
async fn show_tariffs(c: &Ctx<'_>, chat: i64, client_id: i64, mid: Option<i64>) -> Result<()> {
    // Распаковка контекста: тела экранов писались до его появления
    // и обращаются к этим четырём по именам.
    let (tg, pool, cfg, reg) = (c.tg, c.pool, c.cfg, c.reg);
    let _ = (cfg, reg);

    let rows = sqlx::query(
        // is_trial показываем меткой: пробный тариф среди платных
        // выглядит подозрительно дешёвым, и его пропускают мимо глаз.
        // Уже использованный пробный не предлагаем — он одноразовый.
        "SELECT t.id, t.title, t.description, t.device_limit, t.traffic_limit_bytes,
                t.is_trial
           FROM tariffs t
          WHERE t.is_active AND t.is_visible
            AND (NOT t.is_trial OR NOT EXISTS (
                  SELECT 1 FROM payments p
                   WHERE p.client_id = $1 AND p.tariff_id = t.id AND p.status='success'))
          ORDER BY t.is_trial DESC, t.sort_order, t.id",
    )
    .bind(client_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        let text = "Тарифы пока не настроены. Загляните позже.";
        match mid {
            Some(m) => tg.edit(chat, m, text, None).await?,
            None => { tg.send(chat, text, None).await?; }
        }
        return Ok(());
    }

    let currency = money::service_currency(pool).await;
    let mut buttons = Vec::new();
    let mut text = String::from("<b>Выберите тариф</b>\n");
    for r in &rows {
        let id: i64 = r.get("id");
        let title: String = r.get("title");
        let devices: i32 = r.get("device_limit");
        let limit: Option<i64> = r.get("traffic_limit_bytes");
        // Минимум только среди цен в валюте сервиса. Без фильтра сюда
        // попадала цена в Stars — она в своих единицах и почти всегда
        // меньше, поэтому на кнопке оказывалось «PRO от 1.29» вместо
        // настоящей месячной цены.
        let min: Option<i64> = sqlx::query_scalar(
            "SELECT min(amount_minor) FROM tariff_prices
              WHERE tariff_id = $1 AND is_active AND currency = $2",
        )
        .bind(id)
        .bind(&currency)
        .fetch_one(pool)
        .await?;

        if min.is_none() { continue; }
        // Пробный помечаем и в тексте, и на кнопке: среди платных он
        // выглядит подозрительно дешёвым, и его пропускают мимо глаз.
        let trial: bool = r.get("is_trial");
        let mark = if trial { " 🎁" } else { "" };
        text.push_str(&format!(
            "\n<b>{title}</b>{mark} — {} устр., {}\n",
            devices,
            limit.map(money::format_bytes).unwrap_or_else(|| "безлимит".into())
        ));
        if trial {
            text.push_str("<i>Пробный — один раз на аккаунт.</i>\n");
        }
        if let Some(desc) = r.get::<Option<String>, _>("description") {
            if !desc.is_empty() {
                text.push_str(&format!("<i>{desc}</i>\n"));
            }
        }
        // Валюта была зашита строкой: цены давно меняли в панели, а на
        // кнопке всё равно стоял доллар.
        let from = min
            .map(|m| format!(" от {}", money::format_minor(m, &currency)))
            .unwrap_or_default();
        buttons.push(vec![Btn::Data(format!("{mark}{title}{from}"), format!("t:{id}"))]);
    }
    if buttons.is_empty() {
        text = "Тарифы пока не опубликованы. Напишите в поддержку.".into();
    }
    buttons.push(vec![Btn::Data("← Назад".into(), "menu".into())]);

    let kb = keyboard(buttons);
    tg.screen(chat, mid, &text, c.s.image(settings::Screen::Buy).as_deref(), Some(kb))
        .await?;
    Ok(())
}

/// Периоды выбранного тарифа с выгодой относительно месячной цены.
async fn show_periods(c: &Ctx<'_>, chat: i64, mid: i64, tariff_id: i64) -> Result<()> {
    // Распаковка контекста: тела экранов писались до его появления
    // и обращаются к этим четырём по именам.
    let (tg, pool, cfg, reg) = (c.tg, c.pool, c.cfg, c.reg);
    let _ = (cfg, reg);

    // Только валюта сервиса. Раньше брали все строки подряд, и тариф с
    // ценами в двух валютах показывал «30 дн» дважды — а после смены
    // валюты в панели клиент по-прежнему видел старые суммы, потому что
    // ничего его от них не отсекало. Stars сюда не попадают намеренно:
    // это способ оплаты, он появляется на следующем экране.
    let currency = money::service_currency(pool).await;
    let rows = sqlx::query(
        "SELECT period_days, currency, amount_minor FROM tariff_prices
          WHERE tariff_id = $1 AND is_active AND currency = $2
          ORDER BY period_days",
    )
    .bind(tariff_id)
    .bind(&currency)
    .fetch_all(pool)
    .await?;

    let title: String = sqlx::query_scalar("SELECT title FROM tariffs WHERE id = $1")
        .bind(tariff_id)
        .fetch_one(pool)
        .await?;

    if rows.is_empty() {
        // Тупик без кнопки означал бы, что из тарифа с незаданной ценой
        // человек может выйти только перезапуском бота.
        tg.edit(
            chat,
            mid,
            "У этого тарифа пока нет цены — напишите в поддержку, поможем.",
            Some(keyboard(vec![vec![Btn::Data("← Назад".into(), "buy".into())]])),
        )
        .await?;
        return Ok(());
    }

    // База для расчёта выгоды — самый короткий период.
    let base = rows
        .first()
        .map(|r| {
            let d: i32 = r.get("period_days");
            let a: i64 = r.get("amount_minor");
            a as f64 / d as f64
        })
        .unwrap_or(0.0);

    let mut buttons = Vec::new();
    for r in &rows {
        let days: i32 = r.get("period_days");
        let minor: i64 = r.get("amount_minor");
        let cur: String = r.get("currency");
        let per_day = minor as f64 / days as f64;
        let save = if base > 0.0 && per_day < base {
            format!(" · −{:.0}%", (1.0 - per_day / base) * 100.0)
        } else {
            String::new()
        };
        let label = if minor == 0 {
            format!("{days} дн · бесплатно")
        } else {
            format!("{days} дн · {}{save}", money::format_minor(minor, &cur))
        };
        buttons.push(vec![Btn::Data(label, format!("p:{tariff_id}:{days}"))]);
    }
    buttons.push(vec![Btn::Data("← Назад".into(), "buy".into())]);

    tg.edit(
        chat,
        mid,
        &format!("<b>{title}</b>\n\nНа какой срок?"),
        Some(keyboard(buttons)),
    )
    .await?;
    Ok(())
}

/// Способы оплаты для выбранного периода — из реестра модулей.
async fn show_methods(
    tg: &Tg,
    pool: &Pool,
    reg: &Registry,
    chat: i64,
    mid: i64,
    client_id: i64,
    tariff_id: i64,
    days: i32,
) -> Result<()> {
    let currency = money::service_currency(pool).await;
    let prices = sqlx::query(
        "SELECT currency, amount_minor FROM tariff_prices
          WHERE tariff_id = $1 AND period_days = $2 AND is_active
            AND (currency = $3 OR currency = 'XTR')",
    )
    .bind(tariff_id)
    .bind(days)
    .bind(&currency)
    .fetch_all(pool)
    .await?;

    if !prices.iter().any(|r| r.get::<String, _>("currency") == currency) {
        tg.edit(chat, mid, "Цена не найдена.", None).await?;
        return Ok(());
    }

    // Бесплатный тариф (триал) выдаём сразу, без платёжной части.
    let free = prices.iter().find(|r| r.get::<String, _>("currency") == currency && r.get::<i64, _>("amount_minor") == 0);
    if let Some(_) = free {
        sn_core::billing::activate_free(pool, client_id, tariff_id, days).await?;
        sn_core::addons::refresh(pool,client_id).await?;
    let info = sub_info(pool, client_id).await?;
        let until = info.expires.map(|e| e.format("%d.%m.%Y").to_string()).unwrap_or_default();
        tg.edit(
            chat,
            mid,
            &format!("✅ Готово! Доступ активен до <b>{until}</b>."),
            Some(keyboard(vec![vec![Btn::Data("🔑 Моя подписка".into(), "sub".into())]])),
        )
        .await?;
        return Ok(());
    }

    // Промокод, если он уже введён и всё ещё годится. Проверяем заново:
    // между вводом и этим экраном код могли исчерпать.
    let held = promo::held(pool, client_id, tariff_id).await.unwrap_or(None);

    // Для каждой валюты спрашиваем реестр, кто её принимает.
    let mut buttons: Vec<Vec<Btn>> = Vec::new();
    let mut text = String::from("<b>Способ оплаты</b>\n");
    if let Some(v) = &held {
        text.push_str(&format!(
            "\nПромокод <b>{}</b> — {}\n",
            v.code,
            v.discount.describe()
        ));
    }
    for r in &prices {
        let cur: String = r.get("currency");
        let minor: i64 = r.get("amount_minor");
        // Цену показываем уже со скидкой: увидеть одну сумму на кнопке
        // и другую в счёте — верный повод передумать.
        let pay = held.as_ref().filter(|v| !matches!(v.discount,promo::Discount::Fixed(_)) || v.currency.as_deref()==Some(cur.as_str())).map(|v| v.discount.apply(minor)).unwrap_or(minor);
        for p in reg.enabled_for_currency(pool, &cur).await {
            if cur != currency && p.id() != "stars" { continue; }
            let label = if pay == minor {
                format!("{} · {}", p.title(), money::format_minor(minor, &cur))
            } else {
                format!(
                    "{} · {} (вместо {})",
                    p.title(),
                    money::format_minor(pay, &cur),
                    money::format_minor(minor, &cur)
                )
            };
            buttons.push(vec![Btn::Data(label, format!("m:{tariff_id}:{days}:{}:{cur}", p.id()))]);
        }
    }

    if buttons.is_empty() {
        text = "Способы оплаты не настроены.\nНапишите в поддержку — поможем вручную.".into();
    } else {
        buttons.push(vec![Btn::Data(
            if held.is_some() { "🎟 Изменить промокод".into() } else { "🎟 У меня есть промокод".into() },
            format!("promo:{tariff_id}:{days}"),
        )]);
    }
    buttons.push(vec![Btn::Data("← Назад".into(), format!("t:{tariff_id}"))]);

    tg.edit(chat, mid, &text, Some(keyboard(buttons))).await?;
    Ok(())
}

/// Бесплатный период: активируем без платежа, но только один раз.
async fn show_subscription(c: &Ctx<'_>, chat: i64, client_id: i64, mid: Option<i64>) -> Result<()> {
    // Распаковка контекста: тела экранов писались до его появления
    // и обращаются к этим четырём по именам.
    let (tg, pool, cfg, reg) = (c.tg, c.pool, c.cfg, c.reg);
    let _ = (cfg, reg);

    sn_core::addons::refresh(pool,client_id).await?;
    let info = sub_info(pool, client_id).await?;
    let url = format!("{}/{}", cfg.sub_public_url.trim_end_matches('/'), info.short_id);

    let text = if info.status == "active" {
        format!(
            "<b>Ваша подписка</b>\n\nСсылка для приложения:\n<code>{url}</code>\n\n\
             Скопируйте её и добавьте как подписку в вашем VPN-приложении.\n\
             Или откройте ссылку в браузере — там есть инструкция."
        )
    } else {
        format!("Подписка неактивна.\n\nСсылка появится сразу после оплаты.")
    };

    let kb = keyboard(vec![
        vec![Btn::Data("📱 Как подключить".into(), "apps".into())],
        vec![Btn::Data(
            if info.autorenew { "🔁 Автопродление: включено".into() }
            else { "🔁 Автопродление: выключено".into() },
            "renew".into(),
        )],
        vec![Btn::Url("🌐 Открыть страницу".into(), url)],
        vec![Btn::Data("← Назад".into(), "menu".into())],
    ]);
    tg.screen(chat, mid, &text, c.s.image(settings::Screen::Sub).as_deref(), Some(kb))
        .await?;
    Ok(())
}

// ─────────────────────────── обработка ───────────────────────────

async fn on_message(c: &Ctx<'_>, msg: &Value) -> Result<()> {
    // Распаковка контекста: тела экранов писались до его появления
    // и обращаются к этим четырём по именам.
    let (tg, pool, cfg, reg) = (c.tg, c.pool, c.cfg, c.reg);
    let _ = (cfg, reg);

    let chat = msg["chat"]["id"].as_i64().unwrap_or(0);
    let from = &msg["from"];

    // Успешная оплата Stars приходит обычным сообщением.
    if !msg["successful_payment"].is_null() {
        let provider = reg
            .get("stars")
            .ok_or_else(|| Error::Internal("модуль stars не собран".into()))?;
        let body = serde_json::to_vec(msg).unwrap_or_default();
        let outcome = provider.handle_webhook(&Default::default(), &body).await?;
        reg.apply_outcome(pool, "stars", &outcome).await?;

        let client_id = ensure_client(pool, from).await?;
        sn_core::addons::refresh(pool,client_id).await?;
        let info = sub_info(pool, client_id).await?;
        let until = info.expires.map(|e| e.format("%d.%m.%Y").to_string()).unwrap_or_default();
        let receipt = addon_receipt(pool, outcome.payment_id.unwrap_or(0), client_id).await?
            .unwrap_or_else(||format!("✅ Оплата получена!\n\nПодписка активна до <b>{until}</b>."));
        tg.send(
            chat,
            &receipt,
            Some(keyboard(vec![vec![Btn::Data("🔑 Моя подписка".into(), "sub".into())]])),
        )
        .await?;
        return Ok(());
    }

    let text = msg["text"].as_str().unwrap_or("");
    let client_id = ensure_client(pool, from).await?;

    // Ждём ли мы от этого человека текст обращения. Проверяем до разбора
    // команд: иначе сообщение ушло бы в общий обработчик и открыло меню.
    if !text.starts_with('/') {
        if let Some((kind, tid)) = tickets::pending(pool, client_id).await? {
            // Промокод пришёл в ожидании покупки — проверяем и
            // возвращаем человека туда, откуда он ушёл вводить.
            if let Some(rest) = kind.strip_prefix("promo:") {
                let mut it = rest.split(':');
                let t: i64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let d: i32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                match promo::check(pool, text, client_id, t).await? {
                    Ok(v) => {
                        let what = v.discount.describe();
                        promo::hold(pool, client_id, &v).await?;
                        tg.send(chat, &format!("Промокод принят: <b>{what}</b>"), None).await?;
                    }
                    Err(why) => {
                        tg.send(chat, why.message(), None).await?;
                    }
                }
                let mid = tg.send(chat, "Загружаем…", None).await?;
                return show_methods(tg, pool, reg, chat, mid, client_id, t, d).await;
            }
            return tickets::accept_text(c, chat, client_id, &kind, tid, text).await;
        }
    }

    let command=text.split_whitespace().next().unwrap_or("").split('@').next().unwrap_or("");
    if command.starts_with('/') {sqlx::query("DELETE FROM bot_await WHERE client_id=$1").bind(client_id).execute(pool).await?;}
    match command {
        "/addons" => addons::show(c,chat,client_id,None,None).await,
        "/start" => {
            if let Some(token)=text.split_whitespace().nth(1).and_then(|s|s.strip_prefix("cab_")) {
                match sn_core::cabinet::link_info(pool,token).await {
                    Ok((site,account))=>{tg.send(chat,&format!("Привязать Telegram к аккаунту <b>{}</b> на {}?\n\nПодтверждайте только запрос, который вы только что создали на сайте.",html_escape(&account),html_escape(&site)),Some(keyboard(vec![vec![Btn::Data("Подтвердить привязку".into(),format!("cablink:{token}"))],vec![Btn::Data("Отмена".into(),"menu".into())]]))).await?;},
                    Err(e)=>{tg.send(chat,&html_escape(&e.to_string()),None).await?;}
                }
                return Ok(());
            }
            // Пришёл по ссылке приглашения: /start r_<slug>. Привязку
            // делаем до всего остального — иначе первая же покупка
            // пройдёт мимо того, кто человека привёл.
            if let Some(arg) = text.split_whitespace().nth(1).and_then(|a| a.strip_prefix("r_")) {
                match referral::attach(pool, client_id, arg).await {
                    Ok(true) => tracing::info!(client_id, slug = arg, "клиент пришёл по приглашению"),
                    Ok(false) => {}
                    Err(e) => tracing::warn!(error = %e, "не привязал приглашение"),
                }
            }

            // Приветствие показываем отдельным сообщением перед меню:
            // приклеивать его к меню значит показывать при каждом
            // возврате на главную, а это быстро надоедает.
            if let Some(text) = c.s.welcome() {
                let first = is_first_visit(pool, client_id).await?;
                if first || c.s.welcome_always() {
                    tg.screen(chat, None, &text,
                              c.s.image(settings::Screen::Menu).as_deref(), None).await?;
                }
            }
            show_menu(c, chat, client_id, None).await
        }
        "/cancel" => show_menu(c,chat,client_id,None).await,
        "/support" | "/paysupport" => {
            if c.s.tickets_enabled() { tickets::show_list(c,chat,client_id,None).await }
            else { show_docs(c,chat,None).await }
        }
        "/buy" => show_tariffs(c, chat, client_id, None).await,
        "/sub" => show_subscription(c, chat, client_id, None).await,
        "/docs" => show_docs(c, chat, None).await,
        // Старая команда: у людей она уже в истории переписки.
        "/help" => show_docs(c, chat, None).await,
        _ => show_menu(c, chat, client_id, None).await,
    }
}

/// Первый ли это заход. По нему решаем, показывать ли приветствие.
///
/// Отметку держим в bot_state рядом со смещением апдейтов: заводить ради
/// одного флага таблицу незачем.
async fn is_first_visit(pool: &Pool, client_id: i64) -> Result<bool> {
    let key = format!("greeted:{client_id}");
    let seen: Option<i64> = sqlx::query_scalar("SELECT value FROM bot_state WHERE key = $1")
        .bind(&key)
        .fetch_optional(pool)
        .await?;
    if seen.is_some() {
        return Ok(false);
    }
    sqlx::query("INSERT INTO bot_state (key, value) VALUES ($1, 1) ON CONFLICT DO NOTHING")
        .bind(&key)
        .execute(pool)
        .await?;
    Ok(true)
}

/// Экран документов: оферта, договор и что администратор добавит сам.
///
/// Заменил экран помощи: рядом с ним в меню жила кнопка поддержки, и две
/// кнопки об одном и том же только путали. Поддержка осталась в меню,
/// а здесь — то, чего нигде больше не прочитать.
async fn show_docs(c: &Ctx<'_>, chat: i64, mid: Option<i64>) -> Result<()> {
    let links = c.s.docs_links();
    let mut body = c.s.docs_text();
    if links.is_empty() {
        // Пустой экран с одной кнопкой «назад» выглядит поломкой.
        body.push_str("\n\nСписок пока пуст.");
    }

    // По кнопке на строку: подписи вроде «Публичная оферта» в паре
    // не помещаются и обрезаются.
    let mut rows: Vec<Vec<Btn>> =
        links.into_iter().map(|d| vec![Btn::Url(d.label, d.url)]).collect();
    if let Some(url)=c.s.support_url() {rows.push(vec![Btn::Url(c.s.support_label(),url)]);}
    rows.push(vec![Btn::Data("← Назад".into(), "menu".into())]);

    c.tg.screen(chat, mid, &body, c.s.image(settings::Screen::Docs).as_deref(),
                Some(keyboard(rows)))
        .await?;
    Ok(())
}

async fn on_callback(c: &Ctx<'_>, cq: &Value) -> Result<()> {
    // Распаковка контекста: тела экранов писались до его появления
    // и обращаются к этим четырём по именам.
    let (tg, pool, cfg, reg) = (c.tg, c.pool, c.cfg, c.reg);
    let _ = (cfg, reg);

    let id = cq["id"].as_str().unwrap_or_default();
    let chat = cq["message"]["chat"]["id"].as_i64().unwrap_or(0);
    let mid = cq["message"]["message_id"].as_i64().unwrap_or(0);
    let data = cq["data"].as_str().unwrap_or("");
    let client_id = ensure_client(pool, &cq["from"]).await?;
    sqlx::query("DELETE FROM bot_await WHERE client_id=$1").bind(client_id).execute(pool).await?;

    tg.answer_callback(id, None).await.ok();

    let parts: Vec<&str> = data.split(':').collect();
    if let Some(token)=data.strip_prefix("cablink:") {
        match sn_core::cabinet::confirm_link(pool,client_id,token).await {
            Ok(_)=>{tg.send(chat,"Telegram привязан. Вернитесь на сайт и обновите кабинет. Подписка и покупки теперь общие.",None).await?;},
            Err(e)=>{tg.send(chat,&html_escape(&e.to_string()),None).await?;}
        }
        return Ok(());
    }
    if tickets::on_callback(c, chat, mid, client_id, parts.as_slice()).await? {
        return Ok(());
    }
    match parts.as_slice() {
        ["menu"] => show_menu(c, chat, client_id, Some(mid)).await,
        ["buy"] => show_tariffs(c, chat, client_id, Some(mid)).await,
        ["addons"] => addons::show(c,chat,client_id,Some(mid),None).await,
        ["addon",id] => addons::show(c,chat,client_id,Some(mid),Some(id.parse().map_err(|_|Error::bad("пакет не найден"))?)).await,
        ["am",id,provider,currency] => addons::pay(c,chat,client_id,mid,id.parse().map_err(|_|Error::bad("пакет не найден"))?,provider,currency).await,
        ["sub"] => show_subscription(c, chat, client_id, Some(mid)).await,
        ["docs"] | ["help"] => show_docs(c, chat, Some(mid)).await,
        ["renew"] => {
            // Переключаем и сразу показываем карточку заново: без этого
            // человек не видит, сработало ли нажатие.
            let now: bool = sqlx::query_scalar(
                "UPDATE subscriptions SET autorenew = NOT autorenew
                  WHERE client_id = $1 AND is_current RETURNING autorenew",
            )
            .bind(client_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or(false);
            tg.answer_callback(
                id,
                Some(if now { "Автопродление включено" } else { "Автопродление выключено" }),
            )
            .await
            .ok();
            show_subscription(c, chat, client_id, Some(mid)).await
        }
        ["ref"] => {
            let me = tg.get_me().await.ok();
            let name = me
                .as_ref()
                .and_then(|m| m["username"].as_str())
                .unwrap_or_default()
                .to_string();
            referral::show(c, chat, client_id, &name, Some(mid)).await
        }
        ["promo", tid, d] => {
            let (tid, d) = (tid.parse::<i64>().unwrap_or(0), d.parse::<i32>().unwrap_or(0));
            tickets::expect(pool, client_id, &format!("promo:{tid}:{d}"), None).await?;
            tg.screen(
                chat, Some(mid),
                "Пришлите промокод одним сообщением.",
                c.s.image(settings::Screen::Buy).as_deref(),
                Some(keyboard(vec![vec![Btn::Data(
                    "← Отмена".into(), format!("p:{tid}:{d}"))]])),
            ).await?;
            Ok(())
        }
        ["apps"] => apps::show_platforms(c, chat, Some(mid)).await,
        ["app", platform] => {
            sn_core::addons::refresh(pool,client_id).await?;
    let info = sub_info(pool, client_id).await?;
            let url = format!("{}/{}", cfg.sub_public_url.trim_end_matches('/'), info.short_id);
            apps::show_apps(c, chat, platform, &url, Some(mid)).await
        }
        ["t", tid] => {
            let tid: i64 = tid.parse().map_err(|_| Error::bad("плохой тариф"))?;
            show_periods(c, chat, mid, tid).await
        }
        ["p", tid, days] => {
            let tid: i64 = tid.parse().map_err(|_| Error::bad("плохой тариф"))?;
            let days: i32 = days.parse().map_err(|_| Error::bad("плохой срок"))?;
            match show_methods(tg, pool, reg, chat, mid, client_id, tid, days).await {
                Ok(()) => Ok(()),
                Err(e) => {
                    tg.edit(chat, mid, &format!("⚠️ {e}"),
                        Some(keyboard(vec![vec![Btn::Data("← Назад".into(), "buy".into())]]))).await
                }
            }
        }
        ["c", pid] => {
            let pid: i64 = pid.parse().map_err(|_| Error::bad("плохой платёж"))?;
            check_payment(tg, pool, reg, cfg, chat, mid, client_id, pid).await
        }
        ["m", tid, days, provider, currency] => {
            let tid: i64 = tid.parse().map_err(|_| Error::bad("плохой тариф"))?;
            let days: i32 = days.parse().map_err(|_| Error::bad("плохой срок"))?;
            create_and_show_invoice(tg, pool, reg, chat, mid, client_id, tid, days, provider, currency).await
        }
        _ => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_and_show_invoice(
    tg: &Tg,
    pool: &Pool,
    reg: &Registry,
    chat: i64,
    mid: i64,
    client_id: i64,
    tariff_id: i64,
    days: i32,
    provider: &str,
    currency: &str,
) -> Result<()> {
    money::check_purchase_currency(pool, currency, provider).await?;
    let row = sqlx::query(
        "SELECT tp.amount_minor, t.title
           FROM tariff_prices tp JOIN tariffs t ON t.id = tp.tariff_id
          WHERE tp.tariff_id = $1 AND tp.period_days = $2 AND tp.currency = $3 AND tp.is_active",
    )
    .bind(tariff_id)
    .bind(days)
    .bind(currency)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::bad("цена не найдена"))?;

    let full: i64 = row.get("amount_minor");
    let title: String = row.get("title");

    // Скидка применяется здесь, а не раньше: между экраном способов и
    // нажатием кнопки код мог исчерпаться, и списать со скидкой то, что
    // уже не действует, нельзя.
    let held = promo::held(pool, client_id, tariff_id).await.unwrap_or(None)
        .filter(|v| !matches!(v.discount,promo::Discount::Fixed(_)) || v.currency.as_deref()==Some(currency));
    let amount = held.as_ref().map(|v| v.discount.apply(full)).unwrap_or(full);
    let bonus = held.as_ref().map(|v| v.discount.bonus_days()).unwrap_or(0);
    let days = days + bonus;

    let description = match &held {
        Some(v) => format!("{title} · {days} дн · промокод {}", v.code),
        None => format!("{title} · {days} дн"),
    };

    let telegram_id = sqlx::query_scalar::<_, String>(
        "SELECT value FROM client_identities WHERE client_id = $1 AND kind = 'telegram'",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?
    .and_then(|v| v.parse::<i64>().ok());

    match reg
        .create_payment(pool, provider, client_id, tariff_id, days-bonus, full, currency, &description, telegram_id, held.as_ref().map(|v|v.id))
        .await
    {
        Ok((payment_id, invoice)) => {
            if invoice.payload["free"]==true {
                return tg.edit(chat,mid,"✅ Промокод применён. Доступ включён без оплаты.",Some(keyboard(vec![vec![Btn::Data("🔑 Моя подписка".into(),"sub".into())]]))).await;
            }
            let mut rows = Vec::new();
            if !invoice.pay_url.is_empty() {
                rows.push(vec![Btn::Url("💳 Оплатить".into(), invoice.pay_url.clone())]);
            }
            rows.push(vec![Btn::Data("🔄 Проверить оплату".into(), format!("c:{payment_id}"))]);
            rows.push(vec![Btn::Data("← Отмена".into(), "buy".into())]);

            let instructions = invoice.payload["instructions"].as_str().unwrap_or("");
            let body = if instructions.is_empty() {
                format!(
                    "<b>{description}</b>\nК оплате: <b>{}</b>\n\nНажмите «Оплатить». \
                     Доступ включится автоматически.",
                    money::format_minor(amount, currency)
                )
            } else {
                format!(
                    "<b>{description}</b>\nК оплате: <b>{}</b>\n\n{instructions}",
                    money::format_minor(amount, currency)
                )
            };
            tg.edit(chat, mid, &body, Some(keyboard(rows))).await
        }
        Err(e) => {
            tg.edit(
                chat,
                mid,
                &format!("⚠️ Не удалось создать счёт.\n<i>{e}</i>"),
                Some(keyboard(vec![vec![Btn::Data("← Назад".into(), "buy".into())]])),
            )
            .await
        }
    }
}

/// Ручная проверка оплаты: клиент нажал «Проверить».
///
/// Нужна, когда вебхук не дошёл — сеть, блокировка, упавший процесс.
/// Без неё человек, который уже заплатил, остаётся без доступа и идёт в поддержку.
async fn addon_receipt(pool:&Pool,payment:i64,client:i64)->Result<Option<String>> {
    let addon:Option<(String,bool)>=sqlx::query_as("SELECT p.addon_snapshot->>'title',a.activated_at IS NULL FROM payments p JOIN subscription_addons a ON a.payment_id=p.id WHERE p.id=$1 AND p.client_id=$2")
        .bind(payment).bind(client).fetch_optional(pool).await?;
    Ok(addon.map(|(title,pending)|format!("Оплата подтверждена: <b>{}</b>.\n\n{}",html_escape(&title),if pending{"Пакет сохранён и включится после активации подходящей подписки."}else{"Лимиты подписки увеличены. Обновите подписку в VPN-приложении."})))
}

async fn check_payment(
    tg: &Tg, pool: &Pool, reg: &Registry, cfg: &Config,
    chat: i64, mid: i64, client_id: i64, payment_id: i64,
) -> Result<()> {
    let row = sqlx::query(
        "SELECT status::text AS status, provider, provider_txid
           FROM payments WHERE id = $1 AND client_id = $2",
    )
    .bind(payment_id)
    .bind(client_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::bad("платёж не найден"))?;

    let _=row;
    reg.check_payment(pool,payment_id).await?;

    sn_core::addons::refresh(pool,client_id).await?;
    let info = sub_info(pool, client_id).await?;
    let paid: bool = sqlx::query_scalar("SELECT status='success' FROM payments WHERE id=$1 AND client_id=$2")
        .bind(payment_id).bind(client_id).fetch_one(pool).await?;
    if paid {
        let until = info.expires.map(|e| e.format("%d.%m.%Y").to_string()).unwrap_or_default();
        let receipt=addon_receipt(pool,payment_id,client_id).await?
            .unwrap_or_else(||format!("✅ Оплата подтверждена!\n\nПодписка активна до <b>{until}</b>."));
        tg.edit(chat, mid,
            &receipt,
            Some(keyboard(vec![vec![Btn::Data("🔑 Моя подписка".into(), "sub".into())]]))).await?;
    } else {
        tg.edit(chat, mid,
            "⏳ Оплата пока не поступила.\n\nЕсли вы уже заплатили — подождите минуту и нажмите ещё раз.",
            Some(keyboard(vec![
                vec![Btn::Data("🔄 Проверить ещё раз".into(), format!("c:{payment_id}"))],
                vec![Btn::Data("← Назад".into(), "buy".into())],
            ]))).await?;
    }
    let _ = cfg;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info(active: bool) -> SubInfo {
        SubInfo {
            status: if active { "active".into() } else { "expired".into() },
            expires: None,
            used: 0,
            limit: None,
            tariff: Some("PRO".into()),
            short_id: "abcd1234".into(),
            autorenew: false,
        }
    }

    /// Собирает подписи кнопок построчно — так проверять содержимое
    /// клавиатуры проще, чем разбирать вложенный JSON в каждом тесте.
    fn labels(kb: &Value) -> Vec<String> {
        kb["inline_keyboard"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|row| row.as_array().unwrap())
            .map(|b| b["text"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn custom_menu_reorders_real_actions_and_inserts_exact_links() {
        let st=settings::View::from_pairs(&[("bot.menu_layout",json!([
            {"id":"docs","icon_custom_emoji_id":"[5215361191051798408]"}, {"id":"link_channel","label":"Наш канал","url":"https://t.me/example?start=hello","icon_custom_emoji_id":"5215361191051798408"},
            {"id":"purchase"}, {"id":"tickets","enabled":false}
        ]))]);
        let (_,kb)=main_menu("VPN",&info(true),&st,"https://sub.example.com",Some("https://panel.example.com"));
        let rows=kb["inline_keyboard"].as_array().unwrap();
        assert_eq!(rows[0][0]["icon_custom_emoji_id"],"5215361191051798408");assert_eq!(rows[1][0]["icon_custom_emoji_id"],"5215361191051798408");
        assert_eq!(rows[0][0]["callback_data"],"docs");assert_eq!(rows[1][0]["text"],"Наш канал");
        assert_eq!(rows[1][0]["url"],"https://t.me/example?start=hello");assert!(rows[1][0].get("callback_data").is_none());
        assert_eq!(rows[2][0]["callback_data"],"buy");assert!(rows[2][0]["text"].as_str().unwrap().contains("Продлить"));
        assert!(rows.iter().all(|r|r[0]["callback_data"]!="tickets"));
    }
    #[test]
    fn custom_order_keeps_subscription_and_feature_conditions() {
        let st=settings::View::from_pairs(&[("bot.menu_layout",json!([
            {"id":"connect"},{"id":"subscription"},{"id":"referral"},{"id":"miniapp"},
            {"id":"link_channel","label":"Канал","url":"tg://resolve?domain=example"},{"id":"docs","enabled":false}
        ]))]);
        let (_,kb)=main_menu("VPN",&info(false),&st,"https://sub.example.com",Some("https://panel.example.com"));
        let rows=kb["inline_keyboard"].as_array().unwrap();assert_eq!(rows[0][0]["url"],"tg://resolve?domain=example");
        assert!(rows.iter().all(|r|r[0].get("web_app").is_none() && !["sub","ref","docs"].contains(&r[0]["callback_data"].as_str().unwrap_or(""))));
        assert!(labels(&kb).iter().any(|label|label.contains("Купить")));
    }

    #[test]
    fn кнопка_поддержки_только_со_ссылкой() {
        // Кнопка без адреса вела бы в никуда — это хуже, чем её отсутствие.
        let st = settings::View::from_pairs(&[]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(!labels(&kb).iter().any(|l| l.contains("поддержк")));

        let st = settings::View::from_pairs(&[
            ("bot.support_url", json!("https://t.me/support")),
        ]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(labels(&kb).iter().any(|l| l.contains("поддержк")));
    }

    #[test]
    fn подпись_кнопки_поддержки_меняется() {
        let st = settings::View::from_pairs(&[
            ("bot.support_url", json!("https://t.me/support")),
            ("bot.support_label", json!("Написать нам")),
        ]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(labels(&kb).contains(&"Написать нам".to_string()));
    }

    #[test]
    fn обращения_выключаются_настройкой() {
        let st = settings::View::from_pairs(&[]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(labels(&kb).iter().any(|l| l.contains("обращения")),
                "по умолчанию обращения включены");

        let st = settings::View::from_pairs(&[("bot.tickets_enabled", json!(false))]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(!labels(&kb).iter().any(|l| l.contains("обращения")));
    }

    #[test]
    fn объявление_попадает_в_текст_меню() {
        let st = settings::View::from_pairs(&[("bot.menu_note", json!("⚡ Новая локация"))]);
        let (text, _) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(text.contains("⚡ Новая локация"));

        let st = settings::View::from_pairs(&[]);
        let (text, _) = main_menu("Бренд", &info(true), &st, "https://sub.example.com", Some("https://panel.example.com"));
        assert!(!text.contains("⚡"));
    }

    #[test]
    fn без_подписки_предлагаем_купить() {
        let st = settings::View::from_pairs(&[]);
        let (_, kb) = main_menu("Бренд", &info(false), &st, "https://sub.example.com", Some("https://panel.example.com"));
        let l = labels(&kb);
        assert!(l.iter().any(|x| x.contains("Купить")));
        assert!(!l.iter().any(|x| x.contains("Моя подписка")),
                "нечего показывать, пока подписки нет");
    }
}

#[cfg(test)]
mod miniapp_tests {
    use super::*;
    use serde_json::json;

    fn info(active: bool) -> SubInfo {
        SubInfo {
            status: if active { "active".into() } else { "expired".into() },
            expires: None, used: 0, limit: None,
            tariff: Some("PRO".into()), short_id: "abcd1234".into(), autorenew: false,
        }
    }

    const SUB: &str = "https://sub.example.com";
    const PANEL: &str = "https://panel.example.com";

    /// Кнопки клавиатуры с их видом: у мини-приложения свой ключ, и
    /// отличить его от обычной ссылки иначе нельзя.
    fn buttons(kb: &Value) -> Vec<(String, String, String)> {
        kb["inline_keyboard"].as_array().unwrap().iter()
            .flat_map(|r| r.as_array().unwrap())
            .map(|b| {
                let kind = if !b["web_app"].is_null() { "web_app" }
                    else if !b["url"].is_null() { "url" } else { "data" };
                (
                    b["text"].as_str().unwrap_or_default().to_string(),
                    kind.to_string(),
                    b["web_app"]["url"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    fn найти<'a>(b: &'a [(String, String, String)], часть: &str)
        -> Option<&'a (String, String, String)> {
        b.iter().find(|(t, _, _)| t.contains(часть))
    }

    #[test]
    fn кабинет_и_подключение_это_разные_кнопки() {
        // Кабинет — состояние подписки, витрина и оплата. «Подключиться» —
        // готовая ссылка для приложения VPN. Раньше это была одна кнопка,
        // и попасть в витрину из бота было негде.
        let st = settings::View::from_pairs(&[("bot.miniapp_enabled", json!(true)),("_cabinet.miniapp_url",json!("https://customer.example.com/app/"))]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, SUB, Some(PANEL));
        let b = buttons(&kb);

        let кабинет = найти(&b, "приложение").expect("кабинет в меню");
        assert_eq!(кабинет.1, "web_app");
        assert_eq!(кабинет.2,"https://customer.example.com/app/");

        let подкл = найти(&b, "Подключиться").expect("подключение в меню");
        assert_eq!(подкл.1, "web_app");
        assert!(подкл.2.contains("/abcd1234"), "ведёт на свою подписку: {}", подкл.2);
    }

    #[test]
    fn кабинет_нужен_и_без_подписки() {
        // Именно там человек выбирает тариф и платит — прятать его до
        // покупки значит не дать купить.
        let st = settings::View::from_pairs(&[("bot.miniapp_enabled", json!(true)),("_cabinet.miniapp_url",json!("https://customer.example.com/app/"))]);
        let (_, kb) = main_menu("Бренд", &info(false), &st, SUB, Some(PANEL));
        let b = buttons(&kb);
        assert!(найти(&b, "приложение").is_some());
        // А подключаться ещё не к чему.
        assert!(найти(&b, "Подключиться").is_none());
    }

    #[test]
    fn кабинет_выключен_по_умолчанию() {
        // На свежей установке панель может быть ещё не на https.
        let st = settings::View::from_pairs(&[]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, SUB, Some(PANEL));
        assert!(найти(&buttons(&kb), "приложение").is_none());
    }

    #[test]
    fn по_http_кнопок_не_будет() {
        // Telegram молча не покажет их — лучше не показывать самим и
        // написать причину в журнал.
        let st = settings::View::from_pairs(&[("bot.miniapp_enabled", json!(true))]);
        let (_, kb) = main_menu("Бренд", &info(true), &st,
                                "http://sub.example.com", Some("http://panel.example.com"));
        assert!(buttons(&kb).iter().all(|(_, k, _)| k != "web_app"));
    }

    #[test]
    fn свой_адрес_кабинета_сильнее_панели() {
        let st = settings::View::from_pairs(&[
            ("bot.miniapp_enabled", json!(true)),
            ("_cabinet.miniapp_url", json!("https://app.example.com/app/")),
            ("bot.miniapp_label", json!("Личный кабинет")),
        ]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, SUB, Some(PANEL));
        let b = buttons(&kb);
        let к = найти(&b, "Личный кабинет").expect("своя подпись");
        assert_eq!(к.2, "https://app.example.com/app/");
    }

    #[test]
    fn подключение_можно_убрать() {
        let st = settings::View::from_pairs(&[("bot.connect_inline", json!(false))]);
        let (_, kb) = main_menu("Бренд", &info(true), &st, SUB, Some(PANEL));
        assert!(найти(&buttons(&kb), "Подключиться").is_none());
    }
}

fn html_escape(text: &str) -> String { text.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;") }

#[cfg(test)] mod workflow_tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    #[ignore = "requires SN_TEST_DATABASE_URL pointing to an isolated sn_audit database"]
    async fn isolated_bot_commands_callbacks_purchase_and_ticket() {
        let url=std::env::var("SN_TEST_DATABASE_URL").expect("isolated DB required");
        let pool=sn_core::db::connect(&url).await.unwrap();
        let name:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
        assert!(name.starts_with("sn_audit_"),"Never run against a service database");
        let cfg=Config{database_url:url,api_bind:String::new(),sub_bind:String::new(),sub_public_url:"https://sub.example.test".into(),brand_name:"Audit VPN".into(),web_root:String::new(),sub_mode:"db".into(),panel_url:"https://panel.example.test".into(),sub_service_token:None};
        let reg=Registry::from_env();let settings=settings::BotSettings::default();
        let replies=vec![json!({"ok":true,"result":{"message_id":10,"username":"qa_bot"}});100];
        let (tg,server)=tg::transport_tests::mock(replies).await;
        let uid=9000000000+chrono::Utc::now().timestamp();
        let message=|text:&str,kind:&str|json!({"update_id":1,"message":{"chat":{"id":uid,"type":kind},"from":{"id":uid,"username":"qa_bot_workflow"},"text":text}});
        handle(&tg,&pool,&reg,&cfg,&settings,&message("/start","group")).await.unwrap();
        let absent:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM client_identities WHERE kind='telegram' AND value=$1)").bind(uid.to_string()).fetch_one(&pool).await.unwrap();assert!(absent,"Group messages must not create a private account or reveal subscription data");
        for cmd in ["/start","/buy","/sub","/docs","/support","/paysupport","/cancel"] {handle(&tg,&pool,&reg,&cfg,&settings,&message(cmd,"private")).await.unwrap();}
        let client:i64=sqlx::query_scalar("SELECT client_id FROM client_identities WHERE kind='telegram' AND value=$1").bind(uid.to_string()).fetch_one(&pool).await.unwrap();
        let tariff:i64=sqlx::query_scalar("SELECT t.id FROM tariffs t JOIN tariff_prices p ON p.tariff_id=t.id WHERE t.is_active AND t.is_visible AND p.amount_minor>0 AND p.period_days=30 AND p.currency='USD' ORDER BY t.id DESC LIMIT 1").fetch_one(&pool).await.unwrap();
        let callback=|data:String|json!({"update_id":2,"callback_query":{"id":"qa-callback","from":{"id":uid},"message":{"chat":{"id":uid,"type":"private"},"message_id":10},"data":data}});
        for data in ["menu".into(),"buy".into(),format!("t:{tariff}"),format!("p:{tariff}:30"),format!("m:{tariff}:30:manual:USD"),"renew".into(),"apps".into(),"docs".into(),"ref".into(),"tickets".into(),"tk:new".into()] {handle(&tg,&pool,&reg,&cfg,&settings,&callback(data)).await.unwrap();}
        handle(&tg,&pool,&reg,&cfg,&settings,&message("Нужна помощь с подключением","private")).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM tickets WHERE client_id=$1").bind(client).fetch_one(&pool).await.unwrap();assert_eq!(count,1);
        let pid:i64=sqlx::query_scalar("SELECT id FROM payments WHERE client_id=$1 AND status='pending'").bind(client).fetch_one(&pool).await.unwrap();
        handle(&tg,&pool,&reg,&cfg,&settings,&callback(format!("c:{pid}"))).await.unwrap();
        handle(&tg,&pool,&reg,&cfg,&settings,&callback("tk:new".into())).await.unwrap();
        handle(&tg,&pool,&reg,&cfg,&settings,&message("/cancel","private")).await.unwrap();
        assert!(tickets::pending(&pool,client).await.unwrap().is_none());
        server.abort();
    }
}

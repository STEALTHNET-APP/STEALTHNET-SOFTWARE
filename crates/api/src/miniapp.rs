//! Кабинет клиента: мини-приложение Telegram.
//!
//! Отдельная поверхность от админской: сюда ходят клиенты, и путать её с
//! панелью нельзя ни адресом, ни способом входа.
//!
//! Вход без паролей — Telegram подписывает данные о пользователе, и мы
//! проверяем подпись ключом бота. Это единственное, чему здесь можно
//! верить: всё остальное в запросе клиент может подделать, включая свой
//! идентификатор.
//!
//! Что умеет: показать подписку и её состояние, показать тарифы, начать
//! оплату. Ничего, что меняло бы чужие данные, — только своё.

use axum::extract::{State, Query, Path};
use sn_core::bot_config as bc;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::Row;

use crate::state::AppState;
use crate::cabinet::Customer;
use sn_core::{Error, Result};

#[cfg(test)]
#[path = "miniapp_checkout_tests.rs"]
mod checkout_tests;

pub fn miniapp_routes(st:AppState) -> Router<AppState> {
    Router::new()
        .route("/api/app/config", get(app_config))
        .route("/api/app/access", get(access_status).post(access_issue))
        .route("/api/app/access/reveal", axum::routing::post(access_reveal))
        .route("/api/app/me", get(app_me))
        .route("/api/app/payments", get(app_payments))
        .route("/api/app/payments/{id}/check", post(app_payment_check))
        .route("/api/app/devices", get(app_devices))
        .route("/api/app/devices/{hwid}", axum::routing::delete(app_device_remove))
        .route("/api/app/autorenew", post(app_autorenew))
        .route("/api/app/quote", post(app_quote))
        .route("/api/app/tariffs", get(app_tariffs))
        .route("/api/app/pay", post(app_pay))
        .route("/api/app/addons", get(app_addons))
        .route("/api/app/addons/pay", post(app_addon_pay))
        .route("/api/app/referral", get(app_referral))
        .route("/api/app/tickets", get(app_tickets).post(app_ticket_new))
        .route("/api/app/tickets/{id}", get(app_ticket).post(app_ticket_reply))
        .route_layer(axum::middleware::from_fn_with_state(st,crate::cabinet::miniapp_gate))
}

pub fn cabinet_client_routes() -> Router<AppState> {
    Router::new()
        .route("/api/cabinet/me",get(app_me))
        .route("/api/cabinet/tariffs",get(app_tariffs))
        .route("/api/cabinet/quote",post(app_quote))
        .route("/api/cabinet/pay",post(app_pay))
        .route("/api/cabinet/payments",get(app_payments))
        .route("/api/cabinet/payments/{id}/check",post(app_payment_check))
        .route("/api/cabinet/devices",get(app_devices))
        .route("/api/cabinet/devices/{hwid}",axum::routing::delete(app_device_remove))
        .route("/api/cabinet/autorenew",post(app_autorenew))
        .route("/api/cabinet/addons",get(app_addons))
        .route("/api/cabinet/addons/pay",post(app_addon_pay))
        .route("/api/cabinet/referral",get(app_referral))
        .route("/api/cabinet/tickets",get(app_tickets).post(app_ticket_new))
        .route("/api/cabinet/tickets/{id}",get(app_ticket).post(app_ticket_reply))
        .route_layer(axum::Extension(crate::cabinet::CabinetChannel))
}

/// Подпись Telegram живёт сутки, но столько её принимать незачем:
/// перехваченная строка открывала бы чужой кабинет весь день. Часа
/// хватает на любой сеанс, а мини-приложение переоткрывается мгновенно.
const MAX_AGE_SECS: i64 = 3600;

/// Проверка подписи `initData`.
///
/// Telegram считает HMAC по отсортированным парам ключ=значение ключом,
/// выведенным из токена бота. Совпало — данным можно верить; не совпало
/// — это не наш пользователь, и разбираться, чем именно он отличается,
/// не нужно.
///
/// Возвращает идентификатор пользователя Telegram.
fn verify_init_data(init: &str, bot_token: &str) -> Result<i64> {
    if init.len() > 16384 { return Err(Error::Unauthorized); }
    let mut seen = std::collections::HashSet::new();
    let mut hash = String::new();
    let mut pairs: Vec<(String, String)> = Vec::new();

    for part in init.split('&') {
        let (k, v) = part.split_once('=').ok_or_else(|| Error::Unauthorized)?;
        let k = urldecode(k);
        let v = urldecode(v);
        if !seen.insert(k.clone()) { return Err(Error::Unauthorized); }
        if k == "hash" {
            hash = v;
        } else {
            pairs.push((k, v));
        }
    }
    if hash.is_empty() {
        return Err(Error::Unauthorized);
    }

    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let check: String = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");

    // Ключ подписи — HMAC от токена бота со строкой «WebAppData».
    type H = Hmac<Sha256>;
    let mut mac = <H as Mac>::new_from_slice(b"WebAppData").map_err(|_| Error::Unauthorized)?;
    mac.update(bot_token.as_bytes());
    let secret = mac.finalize().into_bytes();

    let mut mac = <H as Mac>::new_from_slice(&secret).map_err(|_| Error::Unauthorized)?;
    mac.update(check.as_bytes());
    let expect = hex::encode(mac.finalize().into_bytes());

    // Сравнение постоянного времени: побайтовое даёт возможность
    // подобрать подпись по времени ответа.
    if !constant_time_eq(expect.as_bytes(), hash.as_bytes()) {
        return Err(Error::Unauthorized);
    }

    // Свежесть: подпись без срока годности перехватывается один раз и
    // работает вечно.
    let auth_date: i64 = pairs
        .iter()
        .find(|(k, _)| k == "auth_date")
        .and_then(|(_, v)| v.parse().ok())
        .ok_or(Error::Unauthorized)?;
    let age = chrono::Utc::now().timestamp() - auth_date;
    if age > MAX_AGE_SECS || age < -60 {
        return Err(Error::Unauthorized);
    }

    let user: Value = pairs
        .iter()
        .find(|(k, _)| k == "user")
        .and_then(|(_, v)| serde_json::from_str(v).ok())
        .ok_or(Error::Unauthorized)?;

    user["id"].as_i64().filter(|id| *id > 0).ok_or(Error::Unauthorized)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn urldecode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hi = (b[i + 1] as char).to_digit(16);
                let lo = (b[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push(((h << 4) | l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Кто пришёл: проверяем подпись и находим клиента.
///
/// Прямой запуск Mini App тоже создаёт аккаунт: общий обработчик с ботом
/// сериализует первый вход по Telegram ID и исключает дубликаты.
pub(crate) async fn who(st: &AppState, headers: &HeaderMap) -> Result<i64> {
    let init = headers
        .get("x-telegram-init-data")
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;

    let token = std::env::var("BOT_TOKEN").map_err(|_| {
        // Без токена подпись не проверить, и пускать всех подряд нельзя.
        Error::Internal("BOT_TOKEN не задан — кабинет работать не может".into())
    })?;

    let tg_id = verify_init_data(init, &token)?;

    let user:Value = init.split('&').filter_map(|p| p.split_once('='))
        .find(|(k,_)| *k=="user").and_then(|(_,v)| serde_json::from_str(&urldecode(v)).ok()).ok_or(Error::Unauthorized)?;
    sn_core::telegram_client::ensure(&st.pool,tg_id,user["username"].as_str()).await
}

/// Своя подписка: состояние, остатки, ссылка.
async fn app_me(State(st): State<AppState>, customer: Customer) -> Result<Json<Value>> {
    let client_id = customer.id;

    sn_core::addons::refresh(&st.pool,client_id).await?;
    let r = sqlx::query(
        "SELECT c.username, c.short_id,
                effective_status(c.status, s.expires_at, s.traffic_used_bytes,
                                 s.traffic_limit_bytes)::text AS status,
                s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes,
                s.device_limit, s.autorenew, s.reset_strategy::text AS reset_strategy,
                s.traffic_reset_at, t.title AS tariff, t.locales AS tariff_locales,
                (SELECT count(*) FROM devices d WHERE d.client_id = c.id) AS devices
           FROM clients c
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
           LEFT JOIN tariffs t ON t.id = s.tariff_id
          WHERE c.id = $1",
    )
    .bind(client_id)
    .fetch_one(&st.pool)
    .await?;

    let sub_url: Option<String> = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'subscription.public_url'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_str().map(str::to_string))
    .filter(|s| !s.is_empty())
    .or_else(|| Some(st.config.sub_public_url.clone()))
    .map(|base| format!("{}/{}", base.trim_end_matches('/'), r.get::<String, _>("short_id")));

    Ok(Json(json!({
        "username":   r.get::<String, _>("username"),
        "status":     r.get::<Option<String>, _>("status"),
        "tariff":     r.get::<Option<String>, _>("tariff"),
        "tariff_locales":r.get::<Option<Value>,_>("tariff_locales"),
        "expires_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at"),
        "used":       r.get::<Option<i64>, _>("traffic_used_bytes"),
        "limit":      r.get::<Option<i64>, _>("traffic_limit_bytes"),
        "devices":    r.get::<i64, _>("devices"),
        "device_limit": r.get::<Option<i32>, _>("device_limit"),
        "autorenew":  r.get::<Option<bool>, _>("autorenew"),
        "reset_strategy": r.get::<Option<String>, _>("reset_strategy"),
        "traffic_reset_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("traffic_reset_at"),
        // Ссылку отдаём только своему: это ключ доступа к VPN.
        "sub_url":    sub_url,
    })))
}

/// Витрина: тарифы с ценами и способами оплаты.
async fn app_tariffs(State(st): State<AppState>, customer: Customer) -> Result<Json<Value>> {
    let paid = purchases_enabled(&st, customer.cabinet).await?;
    tariff_catalog(&st,Some(customer.id),paid).await.map(Json)
}
pub(crate) async fn free_access_available(st:&AppState)->Result<bool> {
    let currency = sn_core::money::service_currency(&st.pool).await;
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tariffs t JOIN tariff_prices p ON p.tariff_id=t.id WHERE t.is_active AND t.is_visible AND p.is_active AND p.currency=$1 AND p.amount_minor=0)")
        .bind(currency).fetch_one(&st.pool).await?)
}
pub(crate) async fn tariff_catalog(st:&AppState, client_id:Option<i64>, paid:bool)->Result<Value> {
    let currency = sn_core::money::service_currency(&st.pool).await;

    // Использованный пробный не показываем: он одноразовый, и увидеть
    // его второй раз значит наткнуться на отказ уже на оплате.
    let rows = sqlx::query(
        "SELECT t.id, t.title, t.description, t.locales, t.badge, t.device_limit,
                t.traffic_limit_bytes, t.is_trial
           FROM tariffs t
          WHERE t.is_active AND t.is_visible
            AND (NOT t.is_trial OR NOT EXISTS (
                  SELECT 1 FROM payments p
                   WHERE p.client_id = $1 AND p.tariff_id = t.id AND p.status='success'))
          ORDER BY t.is_trial DESC, t.sort_order, t.id",
    )
    .bind(client_id)
    .fetch_all(&st.pool)
    .await?;

    let mut items = Vec::new();
    for t in &rows {
        let id: i64 = t.get("id");
        let prices = sqlx::query(
            "SELECT period_days, currency, amount_minor FROM tariff_prices
              WHERE tariff_id = $1 AND is_active AND (currency = $2 OR currency = 'XTR')
                AND ($3 OR (currency = $2 AND amount_minor = 0)) ORDER BY period_days",
        )
        .bind(id)
        .bind(&currency)
        .bind(paid)
        .fetch_all(&st.pool)
        .await?;

        if !prices.iter().any(|p| p.get::<String, _>("currency") == currency) {
            continue;
        }

        items.push(json!({
            "id": id,
            "title": t.get::<String, _>("title"),
            "description": t.get::<Option<String>, _>("description"),
            "locales": t.get::<Value,_>("locales"),
            // Тег из панели: в боте он показывался, в кабинете — нет, и
            // «⭐ Популярный» пропадал ровно там, где витрина длиннее всего.
            "badge": t.get::<Option<String>, _>("badge"),
            "device_limit": t.get::<i32, _>("device_limit"),
            "traffic_limit_bytes": t.get::<Option<i64>, _>("traffic_limit_bytes"),
            "is_trial": t.get::<bool, _>("is_trial"),
            "prices": prices.iter().map(|p| json!({
                "days": p.get::<i32, _>("period_days"),
                "currency": p.get::<String, _>("currency"),
                "amount_minor": p.get::<i64, _>("amount_minor"),
            })).collect::<Vec<_>>(),
        }));
    }

    // Способы оплаты — общие для валюты, спрашиваем реестр.
    let telegram=if let Some(id)=client_id {sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM client_identities WHERE client_id=$1 AND kind='telegram' AND is_verified)").bind(id).fetch_one(&st.pool).await?} else {false};
    let methods: Vec<Value> = if paid { st
        .payments
        .enabled_for_currency(&st.pool, &currency)
        .await
        .iter()
        .filter(|p|p.id()!="stars"||telegram)
        .map(|p| json!({ "id": p.id(), "title": p.title() }))
        .collect() } else { Vec::new() };

    let mut methods_by_currency=serde_json::Map::new();
    methods_by_currency.insert(currency.clone(),json!(methods));
    // Stars — только отдельно включаемый способ оплаты, не валюта витрины.
    if paid && currency != "XTR" && telegram {
        let stars=st.payments.enabled_for_currency(&st.pool,"XTR").await.iter().filter(|p|p.id()=="stars").map(|p|json!({"id":p.id(),"title":p.title()})).collect::<Vec<_>>();
        methods_by_currency.insert("XTR".into(),json!(stars));
    }
    Ok(json!({ "tariffs": items, "methods": methods, "methods_by_currency":methods_by_currency,"currency": currency }))
}

#[derive(Deserialize)]
struct PayBody {
    tariff_id: i64,
    days: i32,
    provider: String,
    currency: String,
    #[serde(default)]
    promo: String,
}

/// Начать оплату: создаём счёт и возвращаем ссылку.
// Checkout keeps an advisory-lock connection while the payment registry uses
// its own transactions. Bound these holders so a burst cannot exhaust the
// shared 16-connection pool with waiters and prevent the lock owner progressing.
static CHECKOUT_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
async fn app_pay(
    State(st): State<AppState>,
    customer: Customer,
    Json(b): Json<PayBody>,
) -> Result<Json<Value>> {
    let client_id = customer.id;
    customer.can_buy()?;
    sn_core::money::check_purchase_currency(&st.pool, &b.currency, &b.provider).await?;
    let _checkout_slot = CHECKOUT_SLOTS.acquire().await
        .map_err(|_| Error::bad("оформление временно недоступно"))?;
    let mut checkout_lock=st.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(-client_id).execute(&mut *checkout_lock).await?;
    let promo_id=promo_id(&st,&b.promo).await?;

    // Цену берём из базы, а не из запроса: сумма, присланная клиентом,
    // — это предложение заплатить сколько ему хочется.
    let row = sqlx::query(
        "SELECT tp.amount_minor, t.title
           FROM tariff_prices tp JOIN tariffs t ON t.id = tp.tariff_id
          WHERE tp.tariff_id = $1 AND tp.period_days = $2 AND tp.currency = $3
            AND tp.is_active AND t.is_active AND t.is_visible",
    )
    .bind(b.tariff_id)
    .bind(b.days)
    .bind(&b.currency)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| Error::bad("цена не найдена"))?;

    let amount: i64 = row.get("amount_minor");
    let title: String = row.get("title");

    // Бесплатный тариф выдаём сразу. Через платёжку ноль не проходит:
    // клиента уносило на страницу оплаты — а оттуда, уже вне Telegram,
    // в кабинет без подписи, то есть в «откройте кабинет из бота».
    if amount == 0 {
        sn_core::billing::activate_free(&st.pool, client_id, b.tariff_id, b.days).await?;
        return Ok(Json(json!({ "free": true })));
    }

    // The purchase switch and payment providers apply only to paid access.
    // Check before reusing an invoice too: disabling purchases must take effect immediately.
    check_shop(&st,&customer).await?;
    let existing:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('payment_id',id,'url',pay_url,'amount_minor',amount_minor,'currency',currency,'instructions',provider_payload->>'instructions') FROM payments WHERE client_id=$1 AND tariff_id=$2 AND provider=$3 AND currency=$4 AND period_days=$5+COALESCE((SELECT CASE WHEN kind='days' THEN value ELSE 0 END FROM promo_codes WHERE id=$6),0) AND promo_code_id IS NOT DISTINCT FROM $6 AND status='pending' AND expires_at>now() AND pay_url IS NOT NULL ORDER BY id DESC LIMIT 1")
        .bind(client_id).bind(b.tariff_id).bind(&b.provider).bind(&b.currency).bind(b.days).bind(promo_id).fetch_optional(&st.pool).await?;
    if let Some(invoice)=existing {return Ok(Json(invoice));}

    let telegram_id: Option<i64> = sqlx::query_scalar::<_, String>(
        "SELECT value FROM client_identities WHERE client_id = $1 AND kind = 'telegram'",
    )
    .bind(client_id)
    .fetch_optional(&st.pool)
    .await?
    .and_then(|v| v.parse().ok());

    let (payment_id, invoice) = st
        .payments
        .create_payment_returning(
            &st.pool,
            &b.provider,
            client_id,
            b.tariff_id,
            b.days,
            amount,
            &b.currency,
            &format!("{title} · {} дн", b.days),
            telegram_id,
            promo_id,
            customer.return_url(&st).await?,
        )
        .await?;

    Ok(Json(json!({
        "payment_id": payment_id,
        "url": invoice.pay_url,
        "amount_minor": sqlx::query_scalar::<_,i64>("SELECT amount_minor FROM payments WHERE id=$1").bind(payment_id).fetch_one(&st.pool).await?,
        "currency": b.currency,
        "free": invoice.payload["free"]==true,
        "instructions": invoice.payload["instructions"],
    })))
}

/// Приглашение друзей: своя ссылка и счётчики.
///
/// Партнёрскую запись заводим при первом заходе — той же, что ведёт
/// панель, чтобы начисления попадали в общие отчёты. Ставку берём из
/// настроек бота: она общая для всех, кто пришёл этим путём.
async fn app_referral(State(st): State<AppState>, customer: Customer) -> Result<Json<Value>> {
    let client_id = customer.id;

    let enabled: bool = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'bot.referral_enabled'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

    if !enabled || crate::cabinet::settings(&st).await?["referral_enabled"]!=true {
        return Ok(Json(json!({ "enabled": false })));
    }

    let percent: f64 = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'bot.referral_percent'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_f64())
    .unwrap_or(10.0).clamp(0.0,100.0);

    let (partner_id, slug, _) = sn_core::partners::ensure(&st.pool, client_id, percent).await?.ok_or(Error::NotFound)?;
    let r = sqlx::query(
        "SELECT (SELECT count(*) FROM clients WHERE referred_by = $1 AND deleted_at IS NULL) AS invited,
                -- Приведение обязательно: sum() над bigint в постгресе даёт
                -- numeric, и чтение как i64 роняет обработчик — даже когда
                -- начислений ноль, потому что COALESCE тоже numeric.
                (SELECT COALESCE(sum(amount_minor), 0)::bigint FROM partner_commissions
                  WHERE partner_id = $1 AND currency=(SELECT currency FROM partners WHERE id=$1)) AS earned,
                (SELECT balance_minor FROM partners WHERE id = $1) AS balance,
                (SELECT share_percent::float8 FROM partners WHERE id = $1) AS pct,
                (SELECT currency FROM partners WHERE id=$1) AS currency,
                (SELECT jsonb_agg(to_jsonb(w)-'partner_id' ORDER BY currency) FROM partner_wallets w WHERE partner_id=$1) AS wallets",
    )
    .bind(partner_id)
    .fetch_one(&st.pool)
    .await?;

    let bot: Option<String> = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'bot.username'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_str().map(str::to_string));

    Ok(Json(json!({
        "enabled": true,
        "percent": r.get::<f64, _>("pct"),
        "invited": r.get::<i64, _>("invited"),
        "earned":  r.get::<i64, _>("earned"),
        "balance": r.get::<i64, _>("balance"),
        "currency": r.get::<String,_>("currency"),
        "wallets": r.get::<Value,_>("wallets"),
        "slug": slug,
        // Имя бота панель узнаёт при проверке связи; пусто — ссылку
        // соберёт сам клиент, но лучше отдать готовую.
        "bot": bot,
    })))
}

/// Свои обращения.
async fn app_tickets(State(st): State<AppState>, customer: Customer, Query(q): Query<PageQuery>) -> Result<Json<Value>> {
    let client_id = customer.id;
    let rows = sqlx::query(
        "SELECT id, subject, status::text AS status, updated_at
           FROM tickets WHERE client_id = $1 ORDER BY updated_at DESC,id DESC LIMIT 20 OFFSET $2",
    )
    .bind(client_id)
    .bind(q.offset.unwrap_or(0).clamp(0,100000))
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!({ "has_more": rows.len()==20, "items": rows.iter().map(|r| json!({
        "id": r.get::<i64, _>("id"),
        "subject": r.get::<String, _>("subject"),
        "status": r.get::<String, _>("status"),
        "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })).collect::<Vec<_>>() })))
}

/// Переписка по обращению.
async fn app_ticket(
    State(st): State<AppState>,
    customer: Customer,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Value>> {
    let client_id = customer.id;

    // Принадлежность проверяем всегда: идентификатор приходит от клиента.
    let head = sqlx::query(
        "SELECT subject, status::text AS status FROM tickets
          WHERE id = $1 AND client_id = $2",
    )
    .bind(id)
    .bind(client_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(Error::NotFound)?;

    let msgs = sqlx::query(
        "SELECT id, author_kind, body, created_at FROM ticket_messages
          WHERE ticket_id = $1 AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT 50",
    )
    .bind(id)
    .bind(q.before)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!({
        "id": id,
        "subject": head.get::<String, _>("subject"),
        "status": head.get::<String, _>("status"),
        "has_more": msgs.len()==50,
        "messages": msgs.iter().rev().map(|m| json!({
            "id": m.get::<i64,_>("id"),
            "who": m.get::<String, _>("author_kind"),
            "body": m.get::<String, _>("body"),
            "at": m.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct TextBody {
    body: String,
}

/// Новое обращение. Тема — первая строка: спрашивать её отдельно значит
/// добавить шаг ради того, что и так видно в первом сообщении.
async fn app_ticket_new(
    State(st): State<AppState>,
    customer: Customer,
    Json(b): Json<TextBody>,
) -> Result<Json<Value>> {
    let client_id = customer.id;
    let body = b.body.trim();
    check_message(&st,&customer,body).await?;
    let subject: String = body.lines().next().unwrap_or(body).chars().take(120).collect();

    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tickets (client_id, subject) VALUES ($1, $2) RETURNING id",
    )
    .bind(client_id)
    .bind(&subject)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO ticket_messages (ticket_id, author_kind, body) VALUES ($1, 'client', $2)",
    )
    .bind(id)
    .bind(body)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "id": id })))
}

/// Ответ в своё обращение.
async fn app_ticket_reply(
    State(st): State<AppState>,
    customer: Customer,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(b): Json<TextBody>,
) -> Result<Json<Value>> {
    let client_id = customer.id;
    let body = b.body.trim();
    check_message(&st,&customer,body).await?;

    let mut tx = st.pool.begin().await?;
    // Закрытое не оживляем ответом: у поддержки оно уже вне работы.
    let ok: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM tickets WHERE id = $1 AND client_id = $2 AND status <> 'closed' FOR UPDATE",
    )
    .bind(id)
    .bind(client_id)
    .fetch_optional(&mut *tx)
    .await?;
    if ok.is_none() {
        return Err(Error::bad("обращение закрыто"));
    }

    sqlx::query(
        "INSERT INTO ticket_messages (ticket_id, author_kind, body) VALUES ($1, 'client', $2)",
    )
    .bind(id)
    .bind(body)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE tickets SET status = 'open', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Подпись Telegram: собираем её тем же способом, что и он сам.
    fn sign(pairs: &[(&str, &str)], token: &str) -> String {
        let mut sorted: Vec<_> = pairs.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let check = sorted
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n");

        type H = Hmac<Sha256>;
        let mut mac = <H as Mac>::new_from_slice(b"WebAppData").unwrap();
        mac.update(token.as_bytes());
        let secret = mac.finalize().into_bytes();
        let mut mac = <H as Mac>::new_from_slice(&secret).unwrap();
        mac.update(check.as_bytes());
        let hash = hex::encode(mac.finalize().into_bytes());

        let mut init: Vec<String> = pairs
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencode(v)))
            .collect();
        init.push(format!("hash={hash}"));
        init.join("&")
    }

    fn urlencode(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    const TOKEN: &str = "123456:TEST-TOKEN";

    fn now() -> String {
        chrono::Utc::now().timestamp().to_string()
    }

    #[test]
    fn настоящая_подпись_принимается() {
        let ts = now();
        let init = sign(
            &[("auth_date", ts.as_str()), ("user", r#"{"id":777,"first_name":"Тест"}"#)],
            TOKEN,
        );
        assert_eq!(verify_init_data(&init, TOKEN).ok(), Some(777));
    }

    #[test]
    fn чужой_токен_не_подходит() {
        // Иначе кабинет открывался бы по подписи любого другого бота.
        let ts = now();
        let init = sign(&[("auth_date", ts.as_str()), ("user", r#"{"id":777}"#)], TOKEN);
        assert!(verify_init_data(&init, "999:OTHER").is_err());
    }

    #[test]
    fn подделанный_идентификатор_ломает_подпись() {
        // Самая опасная попытка: подменить user.id и войти под другим.
        let ts = now();
        let init = sign(&[("auth_date", ts.as_str()), ("user", r#"{"id":777}"#)], TOKEN);
        let fake = init.replace("777", "888");
        assert!(verify_init_data(&fake, TOKEN).is_err());
    }

    #[test]
    fn протухшая_подпись_не_принимается() {
        // Перехваченная строка без срока годности работала бы вечно.
        let old = (chrono::Utc::now().timestamp() - MAX_AGE_SECS - 60).to_string();
        let init = sign(&[("auth_date", old.as_str()), ("user", r#"{"id":777}"#)], TOKEN);
        assert!(verify_init_data(&init, TOKEN).is_err());
    }

    #[test]
    fn подпись_из_будущего_не_принимается() {
        let future = (chrono::Utc::now().timestamp() + 3600).to_string();
        let init = sign(&[("auth_date", future.as_str()), ("user", r#"{"id":777}"#)], TOKEN);
        assert!(verify_init_data(&init, TOKEN).is_err());
    }

    #[test]
    fn без_подписи_и_без_пользователя_отказ() {
        assert!(verify_init_data("", TOKEN).is_err());
        assert!(verify_init_data("auth_date=1&user=%7B%7D", TOKEN).is_err());
        // Подпись есть, но пользователя нет — входить некому.
        let ts = now();
        let init = sign(&[("auth_date", ts.as_str())], TOKEN);
        assert!(verify_init_data(&init, TOKEN).is_err());
    }

    #[test]
    fn duplicate_signed_fields_and_nonpositive_ids_are_rejected() {
        let ts=now();
        for pairs in [vec![("auth_date",ts.as_str()),("user",r#"{"id":7}"#),("user",r#"{"id":8}"#)],vec![("auth_date",ts.as_str()),("user",r#"{"id":0}"#)]] {
            assert!(verify_init_data(&sign(&pairs,TOKEN),TOKEN).is_err());
        }
    }

    #[test]
    fn раскодирование_совпадает_с_кодированием() {
        for s in ["простой", "{\"id\":1}", "a b&c=d", "%%"] {
            assert_eq!(urldecode(&urlencode(s)), s, "туда-обратно: {s}");
        }
    }

    #[test]
    fn сравнение_не_зависит_от_длины_совпадения() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}

#[derive(Deserialize,Default)]
struct PageQuery { offset:Option<i64>, before:Option<i64> }
async fn app_config(State(st):State<AppState>) -> Result<Json<Value>> {
    // Brand and content are shared with the installed customer cabinet.
    let mut config=crate::cabinet::settings(&st).await?;
    let s=bc::load(&st.pool).await?;
    config["shop_enabled"]=json!(config["shop_enabled"]==true&&bc::flag(&s,"bot.miniapp_shop",true));
    config["free_access_available"]=json!(free_access_available(&st).await?);
    config["devices_enabled"]=json!(config["devices_enabled"]==true&&bc::flag(&s,"bot.miniapp_devices",true));
    Ok(Json(config))
}
async fn purchases_enabled(st:&AppState,cabinet:bool)->Result<bool> {
    if crate::cabinet::settings(st).await?["shop_enabled"]!=true {return Ok(false);}
    if cabinet {return Ok(true);}
    let s=bc::load(&st.pool).await?;
    Ok(bc::flag(&s,"bot.miniapp_shop",true))
}
async fn check_shop(st:&AppState,customer:&Customer)->Result<()> {
    customer.can_buy()?;
    if !purchases_enabled(st,customer.cabinet).await? {
        return Err(Error::bad(if customer.cabinet {"Покупки на сайте отключены"} else {"Покупки в приложении отключены. Откройте бота или напишите в поддержку"}));
    }
    Ok(())
}
async fn check_message(st:&AppState, _customer:&Customer, body:&str)->Result<()> {
    if crate::cabinet::settings(st).await?["tickets_enabled"]!=true {return Err(Error::bad("Новые сообщения в обращениях отключены. Используйте ссылку на поддержку"));}
    if body.is_empty() || body.chars().count()>4000 {return Err(Error::bad("Сообщение должно содержать от 1 до 4000 символов"));}
    Ok(())
}
async fn promo_id(st:&AppState, code:&str)->Result<Option<i64>> {
    if code.trim().is_empty() {return Ok(None);}
    let id:Option<i64>=sqlx::query_scalar("SELECT id FROM promo_codes WHERE lower(code)=lower($1)").bind(code.trim()).fetch_optional(&st.pool).await?;
    id.map(Some).ok_or_else(||Error::bad("Промокод не найден"))
}
async fn app_quote(State(st):State<AppState>, customer:Customer, Json(b):Json<PayBody>)->Result<Json<Value>> {
    let client=customer.id; customer.can_buy()?;
    sn_core::money::check_purchase_currency(&st.pool, &b.currency, &b.provider).await?;
    let amount:i64=sqlx::query_scalar("SELECT p.amount_minor FROM tariff_prices p JOIN tariffs t ON t.id=p.tariff_id WHERE p.tariff_id=$1 AND p.period_days=$2 AND p.currency=$3 AND p.is_active AND t.is_active AND t.is_visible")
        .bind(b.tariff_id).bind(b.days).bind(&b.currency).fetch_optional(&st.pool).await?.ok_or_else(||Error::bad("Цена не найдена"))?;
    if amount != 0 {check_shop(&st,&customer).await?;}
    let (charge,days)=if let Some(id)=promo_id(&st,&b.promo).await? {
        let mut tx=st.pool.begin().await?;
        let result=sn_payments::promo::reserve(&mut tx,client,b.tariff_id,id,amount,b.days,&b.currency).await?;
        tx.rollback().await?; result
    } else {(amount,b.days)};
    Ok(Json(json!({"amount_minor":charge,"original_minor":amount,"days":days,"currency":b.currency})))
}
async fn app_payments(State(st):State<AppState>, customer:Customer, Query(q):Query<PageQuery>)->Result<Json<Value>> {
    let client=customer.id;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',p.id,'status',p.status,'amount_minor',p.amount_minor,'currency',p.currency,'created_at',p.created_at,'title',COALESCE(p.addon_snapshot->>'title',t.title),'locales',t.locales,'addon_kind',p.addon_snapshot->>'kind','addon_quantity',p.addon_snapshot->'quantity','days',p.period_days,'provider',p.provider,'url',CASE WHEN p.status='pending' AND p.expires_at>now() THEN p.pay_url END,'instructions',p.provider_payload->>'instructions') FROM payments p LEFT JOIN tariffs t ON t.id=p.tariff_id WHERE p.client_id=$1 ORDER BY p.id DESC LIMIT 20 OFFSET $2")
        .bind(client).bind(q.offset.unwrap_or(0).clamp(0,100000)).fetch_all(&st.pool).await?;
    Ok(Json(json!({"has_more":rows.len()==20,"items":rows})))
}
async fn app_payment_check(State(st):State<AppState>, customer:Customer, Path(id):Path<i64>)->Result<Json<Value>> {
    let client=customer.id;
    let owner:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payments WHERE id=$1 AND client_id=$2)").bind(id).bind(client).fetch_one(&st.pool).await?;
    if !owner{return Err(Error::NotFound);}
    st.payments.check_payment(&st.pool,id).await?;
    let status:String=sqlx::query_scalar("SELECT status::text FROM payments WHERE id=$1 AND client_id=$2").bind(id).bind(client).fetch_one(&st.pool).await?;
    let addon_pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM subscription_addons WHERE payment_id=$1 AND activated_at IS NULL)").bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"id":id,"status":status,"addon_pending":addon_pending})))
}
async fn app_devices(State(st):State<AppState>, customer:Customer)->Result<Json<Value>> {
    let client=customer.id;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('hwid',hwid,'platform',platform,'model',model,'app_version',app_version,'last_seen_at',last_seen_at) FROM devices WHERE client_id=$1 ORDER BY last_seen_at DESC").bind(client).fetch_all(&st.pool).await?;
    Ok(Json(json!({"items":rows})))
}
async fn app_device_remove(State(st):State<AppState>, customer:Customer, Path(hwid):Path<String>)->Result<Json<Value>> {
    let client=customer.id;
    if crate::cabinet::settings(&st).await?["devices_enabled"]!=true || (!customer.cabinet && !bc::flag(&bc::load(&st.pool).await?,"bot.miniapp_devices",true)) {return Err(Error::Forbidden);}
    let res=sqlx::query("DELETE FROM devices WHERE client_id=$1 AND hwid=$2").bind(client).bind(&hwid).execute(&st.pool).await?;
    if res.rows_affected()==0 {return Err(Error::NotFound);}
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)] struct AutorenewBody { enabled:bool }
async fn app_autorenew(State(st):State<AppState>, customer:Customer, Json(b):Json<AutorenewBody>)->Result<Json<Value>> {
    let client=customer.id;
    sqlx::query("UPDATE subscriptions SET autorenew=$2 WHERE client_id=$1 AND is_current").bind(client).bind(b.enabled).execute(&st.pool).await?;
    Ok(Json(json!({"enabled":b.enabled})))
}

async fn app_addons(State(st):State<AppState>,customer:Customer)->Result<Json<Value>> {
    let client=customer.id;
    let mut catalog=sn_core::addons::catalog(&st.pool,client).await?;
    if check_shop(&st,&customer).await.is_err() {catalog["items"]=json!([]);}
    let cur=catalog["currency"].as_str().unwrap_or("USD").to_owned();
    let telegram:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM client_identities WHERE client_id=$1 AND kind='telegram' AND is_verified)").bind(client).fetch_one(&st.pool).await?;
    let mut methods=serde_json::Map::new();
    for currency in [cur.as_str(),"XTR"] {
        methods.insert(currency.into(),json!(st.payments.enabled_for_currency(&st.pool,currency).await.iter().filter(|p|p.id()!="stars"||telegram).map(|p|json!({"id":p.id(),"title":p.title()})).collect::<Vec<_>>()));
    }
    catalog["methods_by_currency"]=json!(methods);
    Ok(Json(catalog))
}
#[derive(Deserialize)]
struct AddonPay { package_id:i64, provider:String, currency:String }
async fn app_addon_pay(State(st):State<AppState>,customer:Customer,Json(b):Json<AddonPay>)->Result<Json<Value>> {
    let client=customer.id;
    check_shop(&st,&customer).await?;
    let (id,invoice)=st.payments.create_addon_payment_returning(&st.pool,client,b.package_id,&b.provider,&b.currency,customer.return_url(&st).await?).await?;
    let mut out:Value=sqlx::query_scalar("SELECT jsonb_build_object('payment_id',id,'amount_minor',amount_minor,'currency',currency) FROM payments WHERE id=$1").bind(id).fetch_one(&st.pool).await?;
    out["url"]=json!(invoice.pay_url);out["instructions"]=invoice.payload["instructions"].clone();
    Ok(Json(out))
}

async fn access_reveal(State(st):State<AppState>,customer:Customer)->Result<Json<Value>> {
    Ok(Json(json!({"code":sn_core::cabinet::reveal_code(&st.pool,customer.id).await?})))
}
async fn access_status(State(st):State<AppState>,customer:Customer)->Result<Json<Value>> {
    let issued:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinet_credentials WHERE client_id=$1)").bind(customer.id).fetch_one(&st.pool).await?;
    let sites:Vec<String>=sqlx::query_scalar("SELECT public_url FROM cabinet_installations WHERE revoked_at IS NULL AND token_hash IS NOT NULL ORDER BY created_at DESC").fetch_all(&st.pool).await?;
    Ok(Json(json!({"issued":issued,"sites":sites})))
}
#[derive(Deserialize)]struct AccessIssue {#[serde(default)] replace:bool}
async fn access_issue(State(st):State<AppState>,customer:Customer,Json(b):Json<AccessIssue>)->Result<Json<Value>> {
    let code=sn_core::cabinet::issue_from_telegram(&st.pool,customer.id,b.replace).await?;
    Ok(Json(json!({"code":code})))
}

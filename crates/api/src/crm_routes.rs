//! Продажи и поддержка: промокоды, партнёры, тикеты, рассылки.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn crm_routes() -> Router<AppState> {
    Router::new()
        .route("/api/promos", get(promos_list).post(promo_create))
        .route("/api/promos/{id}", axum::routing::patch(promo_update).delete(promo_delete))
        .route("/api/partners", get(partners_list).post(partner_create))
        .route("/api/partners/{id}", axum::routing::patch(partner_update))
        .route("/api/partners/{id}/payout", post(partner_payout))
        .route("/api/bot/status", get(bot_status))
        .route("/api/bot/alerts", get(bot_alerts_status))
        .route("/api/bot/alerts/test", post(bot_alerts_test))
        .route("/api/bot/alerts/retry", post(bot_alerts_retry))
        .route("/api/tickets", get(tickets_list))
        .route("/api/clients/{id}/messages", get(client_messages).post(client_message_send))
        .route("/api/tickets/{id}", get(ticket_get))
        .route("/api/tickets/{id}/reply", post(ticket_reply))
        .route("/api/tickets/{id}/close", post(ticket_close))
        .route("/api/broadcasts", get(broadcasts_list).post(broadcast_create))
        .route("/api/broadcasts/worker", get(broadcast_worker))
        .route("/api/broadcasts/{id}", get(broadcast_details).patch(broadcast_update))
        .route("/api/broadcasts/{id}/send", post(broadcast_send))
        .route("/api/broadcasts/{id}/cancel", post(broadcast_cancel))
        .route("/api/sessions", get(sessions_list))
        .route("/api/sessions/activity", get(activity_list))
        .route("/api/srh", get(srh_list))
        .route("/api/devices", get(devices_list))
}

fn dt(row: &sqlx::postgres::PgRow, col: &str) -> Option<String> {
    row.try_get::<Option<DateTime<Utc>>, _>(col)
        .ok()
        .flatten()
        .map(|d| d.to_rfc3339())
}

// ─────────────────────────── промокоды ───────────────────────────

async fn promos_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT p.id, p.code, p.kind::text AS kind, p.value, p.currency, p.max_uses,
                p.used_count, p.per_client_limit, p.valid_from, p.valid_until,
                p.first_purchase_only, p.is_active, t.code AS tariff_code
           FROM promo_codes p LEFT JOIN tariffs t ON t.id = p.tariff_id
          ORDER BY p.is_active DESC, p.id DESC",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "code": r.get::<String, _>("code"),
            "kind": r.get::<String, _>("kind"),
            "value": r.get::<i32, _>("value"),
            "currency": r.get::<Option<String>, _>("currency"),
            "max_uses": r.get::<Option<i32>, _>("max_uses"),
            "used_count": r.get::<i32, _>("used_count"),
            "valid_until": dt(r, "valid_until"),
            "first_purchase_only": r.get::<bool, _>("first_purchase_only"),
            "is_active": r.get::<bool, _>("is_active"),
            "tariff_code": r.get::<Option<String>, _>("tariff_code"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct PromoBody {
    is_active: Option<bool>,
    code: String,
    kind: String,
    value: i32,
    currency: Option<String>,
    max_uses: Option<i32>,
    valid_until: Option<DateTime<Utc>>,
    first_purchase_only: Option<bool>,
}

async fn promo_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<PromoBody>,
) -> Result<Json<Value>> {
    if b.code.trim().is_empty() {
        return Err(Error::bad("код не может быть пустым"));
    }
    if b.value <= 0 {
        return Err(Error::bad("значение должно быть больше нуля"));
    }
    // Скидка больше 100% — почти всегда опечатка, а не щедрость.
    if b.kind == "percent" && b.value > 100 {
        return Err(Error::bad("скидка не может превышать 100%"));
    }
    if b.kind == "fixed" && b.currency.is_none() {
        return Err(Error::bad("для фиксированной скидки нужна валюта"));
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO promo_codes (code, kind, value, currency, max_uses, valid_until, first_purchase_only, is_active)
         VALUES (upper($1), $2::promo_kind, $3, $4, $5, $6, COALESCE($7, false),COALESCE($8,true))
         RETURNING id",
    )
    .bind(b.code.trim())
    .bind(&b.kind)
    .bind(b.value)
    .bind(&b.currency)
    .bind(b.max_uses)
    .bind(b.valid_until)
    .bind(b.first_purchase_only)
    .bind(b.is_active)
    .fetch_one(&st.pool)
    .await?;

    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct PromoPatch {
    is_active: Option<bool>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    max_uses: Option<Option<i32>>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    valid_until: Option<Option<DateTime<Utc>>>,
}

async fn promo_update(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PromoPatch>,
) -> Result<Json<Value>> {
    let res = sqlx::query(
        "UPDATE promo_codes
            SET is_active = COALESCE($2, is_active),
                max_uses = CASE WHEN $5 THEN $3 ELSE max_uses END,
                valid_until = CASE WHEN $6 THEN $4 ELSE valid_until END
          WHERE id = $1",
    )
    .bind(id)
    .bind(b.is_active)
    .bind(b.max_uses.flatten())
    .bind(b.valid_until.flatten())
    .bind(b.max_uses.is_some())
    .bind(b.valid_until.is_some())
    .execute(&st.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn promo_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Промокод с историей применений не удаляем, а выключаем:
    // иначе в платежах останется ссылка в никуда.
    let used: i32 = sqlx::query_scalar("SELECT used_count FROM promo_codes WHERE id = $1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;

    if used > 0 {
        sqlx::query("UPDATE promo_codes SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(&st.pool)
            .await?;
        return Ok(Json(json!({ "ok": true, "deactivated": true, "used_count": used })));
    }

    sqlx::query("DELETE FROM promo_codes WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true, "deleted": true })))
}

// ─────────────────────────── партнёры ───────────────────────────

async fn partners_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT p.id, p.title, p.slug, p.share_percent::float8 AS share_percent,
                p.currency, p.balance_minor, p.is_active,
                (SELECT count(*) FROM clients c WHERE c.referred_by = p.id) AS referred,
                COALESCE((SELECT sum(pc.amount_minor)::bigint FROM partner_commissions pc
                           WHERE pc.partner_id = p.id AND pc.currency=p.currency), 0) AS earned_minor,
                COALESCE((SELECT sum(po.amount_minor)::bigint FROM partner_payouts po
                           WHERE po.partner_id = p.id AND po.currency=p.currency), 0) AS paid_minor,
                COALESCE((SELECT jsonb_agg(to_jsonb(w)-'partner_id' ORDER BY w.currency) FROM partner_wallets w WHERE w.partner_id=p.id),'[]'::jsonb) AS wallets,
                (SELECT value #>> '{}' FROM settings WHERE key='bot.username') AS bot_username
           FROM partners p ORDER BY p.is_active DESC, p.id",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "title": r.get::<String, _>("title"),
            "slug": r.get::<String, _>("slug"),
            "share_percent": r.get::<f64, _>("share_percent"),
            "currency": r.get::<String, _>("currency"),
            "balance_minor": r.get::<i64, _>("balance_minor"),
            "referred": r.get::<i64, _>("referred"),
            "earned_minor": r.get::<i64, _>("earned_minor"),
            "paid_minor": r.get::<i64, _>("paid_minor"),
            "is_active": r.get::<bool, _>("is_active"),
            "wallets": r.get::<Value,_>("wallets"),
            "referral_url": r.get::<Option<String>,_>("bot_username").filter(|s| !s.trim().is_empty())
                .map(|bot| format!("https://t.me/{}?start=r_{}", bot.trim().trim_start_matches('@'), r.get::<String,_>("slug"))),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct PayoutBody {
    amount_minor: i64,
    currency: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize)]
struct PartnerBody {
    /// Кого делаем партнёром: имя существующего клиента.
    username: Option<String>,
    title: Option<String>,
    slug: Option<String>,
    share_percent: Option<f64>,
    is_active: Option<bool>,
}

async fn partner_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<PartnerBody>,
) -> Result<Json<Value>> {
    let slug = b.slug.as_deref().map(str::trim).unwrap_or_default().to_lowercase();
    if slug.is_empty() || slug.len() > 62 {
        return Err(Error::bad("слаг ссылки — от 1 до 62 символов"));
    }
    // Слаг попадает в публичную ссылку — пробелы и слэши в ней сломают адрес.
    if !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(Error::bad("в слаге допустимы латиница, цифры, дефис и подчёркивание"));
    }

    let share = b.share_percent.unwrap_or(20.0);
    if !(0.0..=100.0).contains(&share) {
        return Err(Error::bad("ставка — от 0 до 100 процентов"));
    }

    // Партнёр — это клиент: комиссия начисляется на его же аккаунт.
    let client_id: Option<i64> = match b.username.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => Some(
            sqlx::query_scalar(
                "SELECT id FROM clients WHERE username = $1 AND deleted_at IS NULL",
            )
            .bind(name)
            .fetch_optional(&st.pool)
            .await?
            .ok_or_else(|| Error::bad(format!("клиент «{name}» не найден")))?,
        ),
        None => None,
    };

    let title = b
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| b.username.clone())
        .unwrap_or_else(|| slug.clone());

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO partners (client_id, title, slug, share_percent, currency)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(client_id)
    .bind(&title)
    .bind(&slug)
    .bind(share)
    .bind(sn_core::money::service_currency(&st.pool).await)
    .fetch_one(&st.pool)
    .await
    .map_err(|e| {
        // Уникальный слаг: две одинаковые ссылки сделали бы учёт рефералов
        // неоднозначным.
        if e.to_string().contains("duplicate") {
            Error::bad("такой слаг уже занят")
        } else {
            Error::from(e)
        }
    })?;

    Ok(Json(json!({ "id": id })))
}

async fn partner_update(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PartnerBody>,
) -> Result<Json<Value>> {
    if let Some(share) = b.share_percent {
        if !(0.0..=100.0).contains(&share) {
            return Err(Error::bad("ставка — от 0 до 100 процентов"));
        }
    }

    let res = sqlx::query(
        "UPDATE partners SET
            title         = COALESCE($2, title),
            share_percent = COALESCE($3, share_percent),
            is_active     = COALESCE($4, is_active)
          WHERE id = $1",
    )
    .bind(id)
    .bind(&b.title)
    .bind(b.share_percent)
    .bind(b.is_active)
    .execute(&st.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn partner_payout(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PayoutBody>,
) -> Result<Json<Value>> {
    if b.amount_minor <= 0 {
        return Err(Error::bad("сумма должна быть больше нуля"));
    }

    let mut tx = st.pool.begin().await?;

    // All currencies serialize through the partner row, including refunds.
    let primary: String = sqlx::query_scalar("SELECT currency FROM partners WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let cur = b.currency.as_deref().unwrap_or(&primary).trim().to_uppercase();
    let balance: i64 = sqlx::query_scalar("SELECT balance_minor FROM partner_wallets WHERE partner_id=$1 AND currency=$2")
        .bind(id).bind(&cur).fetch_optional(&mut *tx).await?.unwrap_or(0);
    if b.amount_minor > balance {
        return Err(Error::bad(format!("к выплате доступно {}, запрошено {}",
            sn_core::money::format_minor(balance, &cur), sn_core::money::format_minor(b.amount_minor, &cur))));
    }
    sqlx::query("INSERT INTO partner_payouts (partner_id,amount_minor,currency,note,created_by) VALUES ($1,$2,$3,$4,$5)")
        .bind(id).bind(b.amount_minor).bind(&cur).bind(&b.note).bind(admin.id).execute(&mut *tx).await?;
    if cur == primary {
        sqlx::query("UPDATE partners SET balance_minor=balance_minor-$2 WHERE id=$1")
            .bind(id).bind(b.amount_minor).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id,payload)
        VALUES('admin',$1,'partner.payout','partner',$2,$3)")
        .bind(admin.id).bind(id).bind(json!({"amount_minor":b.amount_minor,"currency":cur,"note":b.note}))
        .execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "currency":cur, "balance_minor": balance - b.amount_minor })))
}

// ─────────────────────────── тикеты ───────────────────────────

/// Жив ли бот и виден ли ему канал.
///
/// «Бот молчит» — самая частая жалоба, и причин у неё три: не тот токен,
/// процесс не запущен, Telegram недоступен. Спрашиваем у самого Telegram
/// и говорим прямо, а не оставляем администратора гадать.
///
/// Заодно проверяем канал: если он задан, а бот в нём не администратор,
/// проверка подписки не сработает — и узнать об этом лучше здесь, чем от
/// клиентов.
async fn bot_status(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let Ok(token) = std::env::var("BOT_TOKEN") else {
        return Ok(Json(json!({ "ok": false, "error": "BOT_TOKEN не задан" })));
    };

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| Error::Internal(e.to_string()))?;

    let me: Value = match http
        .get(format!("https://api.telegram.org/bot{token}/getMe"))
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_else(|_| json!({})),
        Err(e) => return Ok(Json(json!({ "ok": false, "error": e.without_url().to_string() }))),
    };

    if me["ok"] != true {
        let desc = me["description"].as_str().unwrap_or("Telegram отказал").to_string();
        return Ok(Json(json!({ "ok": false, "error": desc })));
    }
    let username = me["result"]["username"].as_str().unwrap_or("").to_string();

    // Канал проверяем только если он задан: без него проверки нет вовсе.
    let channel: Option<String> = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'bot.require_channel'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_str().map(str::to_string))
    .filter(|s| !s.trim().is_empty());

    let heartbeat:Option<i64>=sqlx::query_scalar("SELECT value FROM bot_state WHERE key='poll_heartbeat'").fetch_optional(&st.pool).await?;
    let mut out = json!({ "ok": true, "username": username, "polling":heartbeat.is_some_and(|ts| chrono::Utc::now().timestamp()-ts<90),"last_poll_at":heartbeat });
    if let Some(ch) = channel {
        let res: Value = match http
            .get(format!("https://api.telegram.org/bot{token}/getChatMember"))
            .query(&[("chat_id", ch.clone()),("user_id",me["result"]["id"].to_string())])
            .send()
            .await
        {
            Ok(r) => r.json().await.unwrap_or_else(|_| json!({})),
            Err(e) => json!({ "ok": false, "description": e.without_url().to_string() }),
        };
        let ok = res["ok"] == true && matches!(res["result"]["status"].as_str(),Some("administrator"|"creator"));
        out["channel"] = json!(ch);
        out["channel_ok"] = json!(ok);
        if !ok {
            out["channel_error"] =
                json!(res["description"].as_str().unwrap_or("канал недоступен"));
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct TicketsQuery { status: Option<String>, search: Option<String>, limit: Option<i64>, offset: Option<i64>, paginated: Option<bool> }
async fn tickets_list(_a: CurrentAdmin, State(st): State<AppState>, Query(q): Query<TicketsQuery>) -> Result<Json<Value>> {
    let limit=q.limit.unwrap_or(200).clamp(1,500); let offset=q.offset.unwrap_or(0).max(0);
    let status=q.status.filter(|s| !s.is_empty()); let search=q.search.filter(|s| !s.trim().is_empty());
    let filter="($1::text IS NULL OR ($1='unclosed' AND t.status<>'closed') OR t.status::text=$1)
        AND ($2::text IS NULL OR concat_ws(' ',t.subject,c.username) ILIKE '%'||$2||'%')";
    let rows = sqlx::query(&format!("SELECT t.id,t.subject,t.status::text AS status,t.priority::text AS priority,
        t.updated_at,c.username,c.id AS client_id,
        (SELECT body FROM ticket_messages m WHERE m.ticket_id=t.id ORDER BY m.created_at DESC,m.id DESC LIMIT 1) AS last_message,
        (SELECT count(*) FROM ticket_messages m WHERE m.ticket_id=t.id) AS message_count
        FROM tickets t JOIN clients c ON c.id=t.client_id WHERE {filter}
        ORDER BY CASE t.status WHEN 'open' THEN 0 WHEN 'pending' THEN 1 ELSE 2 END,t.updated_at DESC,t.id DESC LIMIT $3 OFFSET $4"))
        .bind(&status).bind(&search).bind(limit).bind(offset).fetch_all(&st.pool).await?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id":r.get::<i64,_>("id"),"subject":r.get::<String,_>("subject"),"status":r.get::<String,_>("status"),
        "priority":r.get::<String,_>("priority"),"username":r.get::<String,_>("username"),"client_id":r.get::<i64,_>("client_id"),
        "last_message":r.get::<Option<String>,_>("last_message"),"message_count":r.get::<i64,_>("message_count"),"updated_at":dt(r,"updated_at")
    })).collect();
    if !q.paginated.unwrap_or(false) { return Ok(Json(json!(items))) }
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM tickets t JOIN clients c ON c.id=t.client_id WHERE {filter}"))
        .bind(&status).bind(&search).fetch_one(&st.pool).await?;
    Ok(Json(json!({"items":items,"total":total,"limit":limit,"offset":offset})))
}

async fn ticket_get(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let t = sqlx::query(
        "SELECT t.id, t.subject, t.status::text AS status, t.priority::text AS priority,
                c.username, c.id AS client_id
           FROM tickets t JOIN clients c ON c.id = t.client_id WHERE t.id = $1",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(Error::NotFound)?;

    let msgs = sqlx::query(
        "SELECT m.id, m.author_kind, m.body, m.created_at, m.delivered, a.username AS admin_name
           FROM ticket_messages m LEFT JOIN admins a ON a.id = m.admin_id
          WHERE m.ticket_id = $1 ORDER BY m.created_at,m.id",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!({
        "id": t.get::<i64, _>("id"),
        "subject": t.get::<String, _>("subject"),
        "status": t.get::<String, _>("status"),
        "priority": t.get::<String, _>("priority"),
        "username": t.get::<String, _>("username"),
        "client_id": t.get::<i64, _>("client_id"),
        "messages": msgs.iter().map(|m| json!({
            "id":m.get::<i64,_>("id"),
            "delivered":m.get::<Option<bool>,_>("delivered"),
            "author_kind": m.get::<String, _>("author_kind"),
            "admin_name": m.get::<Option<String>, _>("admin_name"),
            "body": m.get::<String, _>("body"),
            "created_at": dt(m, "created_at"),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct ReplyBody {
    body: String,
}

async fn ticket_reply(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ReplyBody>,
) -> Result<Json<Value>> {
    if b.body.trim().is_empty() || b.body.chars().count()>4000 {
        return Err(Error::bad("ответ должен содержать от 1 до 4000 символов"));
    }

    let mut tx = st.pool.begin().await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM tickets WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    if exists.is_none() { return Err(Error::NotFound) }
    let message_id: i64 = sqlx::query_scalar("INSERT INTO ticket_messages(ticket_id,author_kind,admin_id,body) VALUES($1,'admin',$2,$3) RETURNING id")
        .bind(id).bind(admin.id).bind(b.body.trim()).fetch_one(&mut *tx).await?;
    // Ответили — мяч на стороне клиента.
    sqlx::query("UPDATE tickets SET status = 'pending', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    // Пробуем доставить ответ в Telegram. Не вышло — сообщение всё равно
    // сохранено, админ увидит это в переписке.
    let delivered = notify_client(&st, id, &b.body).await;
    sqlx::query("UPDATE ticket_messages SET delivered=$2 WHERE id=$1").bind(message_id).bind(delivered).execute(&st.pool).await?;
    Ok(Json(json!({ "ok": true, "delivered": delivered, "message_id":message_id })))
}

async fn notify_client(st: &AppState, ticket_id: i64, body: &str) -> bool {
    deliver_client_message(st,ticket_id,body).await.is_ok()
}

fn client_message_payload(chat_id:i64,ticket_id:i64,body:&str)->Value {
    json!({"chat_id":chat_id,"text":body,"reply_markup":{"inline_keyboard":[[{"text":"Ответить","callback_data":format!("tk:r:{ticket_id}")}]]}})
}

#[cfg(test)]
mod client_message_tests {
    #[test]
    fn plain_message_and_reply_target_are_preserved() {
        let payload=super::client_message_payload(123,42,"<b>Текст</b>\nСтрока 2");
        assert_eq!(payload["chat_id"],123);
        assert_eq!(payload["text"],"<b>Текст</b>\nСтрока 2");
        assert_eq!(payload["reply_markup"]["inline_keyboard"][0][0]["callback_data"],"tk:r:42");
        assert!(payload.get("parse_mode").is_none());
    }
}

async fn deliver_client_message(st: &AppState, ticket_id: i64, body: &str) -> std::result::Result<(),String> {
    let token = std::env::var("BOT_TOKEN").ok().filter(|t| !t.trim().is_empty()).ok_or("Бот не настроен")?;
    let chat: Option<String> = sqlx::query_scalar("SELECT i.value FROM tickets t JOIN client_identities i ON i.client_id=t.client_id AND i.kind='telegram' WHERE t.id=$1")
        .bind(ticket_id).fetch_optional(&st.pool).await.map_err(|_|"Не удалось получить Telegram клиента")?;
    let chat_id = chat.and_then(|c|c.parse::<i64>().ok()).ok_or("Telegram клиента не привязан")?;
    let response = reqwest::Client::new().post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .timeout(std::time::Duration::from_secs(10))
        .json(&client_message_payload(chat_id,ticket_id,body))
        .send().await.map_err(|_|"Telegram не подтвердил доставку. Проверьте историю перед повторной отправкой")?;
    let result:Value=response.json().await.map_err(|_|"Telegram вернул некорректный ответ")?;
    if result["ok"]==true {Ok(())} else {Err(match result["error_code"].as_i64(){Some(403)=>"Клиент заблокировал бота или запретил сообщения",Some(400)=>"Чат недоступен: клиент должен сначала открыть бота",Some(429)=>"Telegram ограничил частоту отправки. Повторите позже",_=>"Telegram отклонил отправку. Проверьте настройки бота"}.into())}
}

#[derive(Deserialize)]
struct ClientMessageQuery { before: Option<i64> }

async fn client_messages(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>,Query(q):Query<ClientMessageQuery>) -> Result<Json<Value>> {
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clients WHERE id=$1 AND deleted_at IS NULL)").bind(id).fetch_one(&st.pool).await?;
    if !exists{return Err(Error::NotFound);}
    let telegram:Option<String>=sqlx::query_scalar("SELECT value FROM client_identities WHERE client_id=$1 AND kind='telegram'").bind(id).fetch_optional(&st.pool).await?;
    let rows=sqlx::query("SELECT m.id,m.ticket_id,m.body,m.author_kind,m.delivered,m.delivery_error,m.created_at,a.username AS admin_name,t.subject FROM ticket_messages m JOIN tickets t ON t.id=m.ticket_id LEFT JOIN admins a ON a.id=m.admin_id WHERE t.client_id=$1 AND ($2::bigint IS NULL OR m.id<$2) ORDER BY m.id DESC LIMIT 51")
        .bind(id).bind(q.before).fetch_all(&st.pool).await?;
    let more=rows.len()>50;
    let items:Vec<Value>=rows.iter().take(50).rev().map(|r|json!({"id":r.get::<i64,_>("id"),"ticket_id":r.get::<i64,_>("ticket_id"),"subject":r.get::<String,_>("subject"),"body":r.get::<String,_>("body"),"author_kind":r.get::<String,_>("author_kind"),"admin_name":r.get::<Option<String>,_>("admin_name"),"delivered":r.get::<Option<bool>,_>("delivered"),"delivery_error":r.get::<Option<String>,_>("delivery_error"),"created_at":dt(r,"created_at")})).collect();
    let next=if more{items.first().and_then(|m|m["id"].as_i64())}else{None};
    Ok(Json(json!({"items":items,"next_before":next,"telegram_linked":telegram.is_some(),"bot_configured":std::env::var("BOT_TOKEN").ok().is_some_and(|t|!t.trim().is_empty())})))
}

#[derive(Deserialize)]
struct ClientMessageBody { body:String,request_id:uuid::Uuid }

async fn client_message_send(CurrentAdmin(admin):CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>,Json(b):Json<ClientMessageBody>) -> Result<Json<Value>> {
    let body=b.body.trim();if body.is_empty()||body.chars().count()>4000{return Err(Error::bad("сообщение должно содержать от 1 до 4000 символов"));}
    let mut tx=st.pool.begin().await?;
    let exists:Option<i64>=sqlx::query_scalar("SELECT id FROM clients WHERE id=$1 AND deleted_at IS NULL FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    if exists.is_none(){return Err(Error::NotFound);}
    if let Some(row)=sqlx::query("SELECT m.id,m.body,m.delivered,m.delivery_error,t.client_id FROM ticket_messages m JOIN tickets t ON t.id=m.ticket_id WHERE m.request_id=$1").bind(b.request_id).fetch_optional(&mut *tx).await?{
        if row.get::<i64,_>("client_id")!=id||row.get::<String,_>("body")!=body{return Err(Error::Conflict("ключ отправки уже использован другим сообщением".into()));}
        return Ok(Json(json!({"ok":true,"message_id":row.get::<i64,_>("id"),"delivered":row.get::<Option<bool>,_>("delivered"),"error":row.get::<Option<String>,_>("delivery_error"),"replayed":true})));
    }
    let linked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM client_identities WHERE client_id=$1 AND kind='telegram')").bind(id).fetch_one(&mut *tx).await?;
    if !linked{return Err(Error::bad("у клиента не привязан Telegram"));}
    let ticket=sqlx::query_scalar::<_,i64>("SELECT id FROM tickets WHERE client_id=$1 AND subject='Сообщения из карточки клиента' AND status<>'closed' ORDER BY id DESC LIMIT 1").bind(id).fetch_optional(&mut *tx).await?;
    let ticket_id=match ticket{Some(t)=>t,None=>sqlx::query_scalar("INSERT INTO tickets(client_id,subject,status) VALUES($1,'Сообщения из карточки клиента','pending') RETURNING id").bind(id).fetch_one(&mut *tx).await?};
    let message_id:i64=sqlx::query_scalar("INSERT INTO ticket_messages(ticket_id,author_kind,admin_id,body,request_id) VALUES($1,'admin',$2,$3,$4) RETURNING id")
        .bind(ticket_id).bind(admin.id).bind(body).bind(b.request_id).fetch_one(&mut *tx).await?;
    sqlx::query("UPDATE tickets SET updated_at=now() WHERE id=$1").bind(ticket_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id,payload) VALUES('admin',$1,'client.message','client',$2,$3)")
        .bind(admin.id).bind(id).bind(json!({"message_id":message_id,"ticket_id":ticket_id})).execute(&mut *tx).await?;
    tx.commit().await?;
    let outcome=deliver_client_message(&st,ticket_id,body).await;let delivered=outcome.is_ok();let error=outcome.err();
    sqlx::query("UPDATE ticket_messages SET delivered=$2,delivery_error=$3 WHERE id=$1").bind(message_id).bind(delivered).bind(&error).execute(&st.pool).await?;
    Ok(Json(json!({"ok":true,"message_id":message_id,"delivered":delivered,"error":error,"replayed":false})))
}

async fn ticket_close(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("UPDATE tickets SET status = 'closed', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 { return Err(Error::NotFound); }
    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'ticket.close', 'ticket', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&st.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

// ─────────────────────────── рассылки ───────────────────────────

async fn broadcasts_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT id, title, body, status::text AS status, segment, total_count, sent_count,
                failed_count, button_text, button_url, scheduled_at, created_at, last_error, retry_at, photo_id
           FROM broadcasts ORDER BY id DESC LIMIT 100",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "title": r.get::<String, _>("title"),
            "status": r.get::<String, _>("status"),
            // Отдаём плоской строкой: панели нужен ключ сегмента, а не
            // обёртка, в которой он лежит.
            "segment": r.get::<Value, _>("segment")
                .get("kind").and_then(|v| v.as_str()).unwrap_or("all").to_string(),
            "body": r.get::<String, _>("body"),
            "button_text": r.get::<Option<String>, _>("button_text"),
            "button_url": r.get::<Option<String>, _>("button_url"),
            "photo_id": r.get::<Option<uuid::Uuid>, _>("photo_id"),
            "total_count": r.get::<i32, _>("total_count"),
            "sent_count": r.get::<i32, _>("sent_count"),
            "failed_count": r.get::<i32, _>("failed_count"),
            "scheduled_at": dt(r, "scheduled_at"),
            "created_at": dt(r, "created_at"),
            "last_error":r.get::<Option<String>,_>("last_error"),"retry_at":dt(r,"retry_at"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BroadcastBody {
    title: String,
    body: String,
    segment: Option<String>,
    button_text: Option<String>,
    button_url: Option<String>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    photo_id: Option<Option<uuid::Uuid>>,
}

fn validate_broadcast(b: &BroadcastBody) -> Result<()> {
    if b.title.trim().is_empty() || (b.body.trim().is_empty() && b.photo_id.flatten().is_none()) { return Err(Error::bad("нужны название и текст или фото рассылки")); }
    let limit = if b.photo_id.flatten().is_some() { 1024 } else { 4096 };
    if b.body.encode_utf16().count()>limit { return Err(Error::bad(format!("Сообщение слишком длинное: максимум {limit} символов (с фото — 1024)"))); }
    if !["all","active","expired","limited","trial"].contains(&b.segment.as_deref().unwrap_or("all")) { return Err(Error::bad("неизвестный сегмент рассылки")); }
    let label=b.button_text.as_deref().unwrap_or("").trim(); let url=b.button_url.as_deref().unwrap_or("").trim();
    if label.is_empty()!=url.is_empty() { return Err(Error::bad("для кнопки нужны подпись и ссылка")); }
    if !url.is_empty() && !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("tg://")) { return Err(Error::bad("ссылка кнопки должна начинаться с https://, http:// или tg://")); }
    Ok(())
}

async fn validate_photo(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: Option<uuid::Uuid>) -> Result<()> {
    if let Some(id) = id {
        let found = sqlx::query("SELECT id FROM broadcast_media WHERE id=$1 FOR KEY SHARE").bind(id).fetch_optional(&mut **tx).await?;
        if found.is_none() { return Err(Error::bad("Фото не найдено. Прикрепите его ещё раз")); }
    }
    Ok(())
}

async fn broadcast_update(CurrentAdmin(admin): CurrentAdmin, State(st): State<AppState>, Path(id): Path<i64>, Json(mut b): Json<BroadcastBody>) -> Result<Json<Value>> {
    let mut tx=st.pool.begin().await?;
    let existing = sqlx::query("SELECT status::text,photo_id FROM broadcasts WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    if existing.get::<String,_>("status") != "draft" { return Err(Error::Conflict("изменять можно только черновик; создайте копию рассылки".into())); }
    if b.photo_id.is_none() { b.photo_id=Some(existing.get("photo_id")); }
    validate_broadcast(&b)?;
    validate_photo(&mut tx, b.photo_id.flatten()).await?;
    sqlx::query("UPDATE broadcasts SET title=$2,body=$3,segment=$4,button_text=$5,button_url=$6,photo_id=$7 WHERE id=$1")
        .bind(id).bind(b.title.trim()).bind(b.body.trim()).bind(json!({"kind":b.segment.unwrap_or_else(||"all".into())})).bind(b.button_text).bind(b.button_url).bind(b.photo_id.flatten()).execute(&mut *tx).await?;
    sqlx::query("UPDATE broadcasts b SET total_count=(SELECT count(*) FROM client_overview co
        WHERE EXISTS(SELECT 1 FROM client_identities i WHERE i.client_id=co.id AND i.kind='telegram')
        AND ((b.segment->>'kind')='all' OR co.status::text=(b.segment->>'kind') OR
             ((b.segment->>'kind')='trial' AND EXISTS(SELECT 1 FROM subscriptions su JOIN tariffs t ON t.id=su.tariff_id WHERE su.client_id=co.id AND su.is_current AND t.is_trial)))) WHERE b.id=$1")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id) VALUES ('admin',$1,'broadcast.update','broadcast',$2)").bind(admin.id).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

/// Создание рассылки. Всегда как черновик: отправка — отдельным действием,
/// чтобы случайное нажатие не ушло на всю базу.
async fn broadcast_create(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<BroadcastBody>,
) -> Result<Json<Value>> {
    validate_broadcast(&b)?;
    let segment = b.segment.unwrap_or_else(|| "all".into());

    // Считаем ровно тех, кому реально уйдёт сообщение: только с Telegram.
    // Иначе администратор видит одно число, а доходит до другого количества.
    let condition = match segment.as_str() {
        "active" => "co.status = 'active'",
        "expired" => "co.status = 'expired'",
        "limited" => "co.status = 'limited'",
        "trial" => "EXISTS(SELECT 1 FROM subscriptions su JOIN tariffs t ON t.id=su.tariff_id WHERE su.client_id=co.id AND su.is_current AND t.is_trial)",
        _ => "true",
    };
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM client_overview co
          WHERE {condition}
            AND EXISTS (SELECT 1 FROM client_identities i
                         WHERE i.client_id = co.id AND i.kind = 'telegram')"
    ))
    .fetch_one(&st.pool)
    .await?;

    let mut tx = st.pool.begin().await?;
    validate_photo(&mut tx, b.photo_id.flatten()).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO broadcasts (title, body, button_text, button_url, segment,
                                 status, total_count, created_by, photo_id)
         VALUES ($1, $2, $3, $4, $5, 'draft', $6, $7, $8) RETURNING id",
    )
    .bind(b.title.trim())
    .bind(b.body.trim())
    .bind(&b.button_text)
    .bind(&b.button_url)
    .bind(json!({ "kind": segment }))
    .bind(total as i32)
    .bind(admin.id)
    .bind(b.photo_id.flatten())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "id": id, "audience_size": total, "total_count": total })))
}

#[derive(Deserialize)]
struct SendBody {
    /// Отправить только себе — проверить текст перед боевой рассылкой.
    test_to: Option<i64>,
    scheduled_at: Option<DateTime<Utc>>,
}

/// Запуск рассылки. Отправкой занимается воркер: держать её в HTTP-запросе
/// нельзя — она идёт минутами и переживает перезапуск.
async fn broadcast_send(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<SendBody>,
) -> Result<Json<Value>> {
    let row = sqlx::query("SELECT status::text AS status, body, title, photo_id FROM broadcasts WHERE id = $1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;

    let status: String = row.get("status");

    let token=std::env::var("BOT_TOKEN").ok().filter(|s|!s.trim().is_empty()).ok_or_else(||Error::bad("Бот не настроен. Добавьте токен Telegram-бота на сервере"))?;
    // Preview uses the same transport, markup and button as the real worker.
    if let Some(chat)=b.test_to {
        if chat<=0 {return Err(Error::bad("Укажите Telegram ID получателя теста"));}
        let button=sqlx::query("SELECT button_text,button_url FROM broadcasts WHERE id=$1").bind(id).fetch_one(&st.pool).await?;
        let keyboard=match(button.get::<Option<String>,_>("button_text"),button.get::<Option<String>,_>("button_url")) {
            (Some(t),Some(u)) if !t.is_empty() && !u.is_empty()=>Some(json!({"inline_keyboard":[[{"text":t,"url":u}]]})),_=>None};
        let text=sn_core::telegram_send::personalize(row.get("body"),&admin.username,"—","—");
        let http=sn_core::telegram_send::client()?;
        let photo=sn_core::telegram_send::load_photo(&st.pool,row.get("photo_id")).await?;
        return match sn_core::telegram_send::send_with_photo(&http,sn_core::telegram_send::API,&token,chat,&text,&keyboard,photo.as_ref()).await {
            sn_core::telegram_send::Delivery::Sent=>Ok(Json(json!({"ok":true,"test":true}))),
            sn_core::telegram_send::Delivery::Permanent(e)|sn_core::telegram_send::Delivery::Stop(e)=>Err(Error::bad(e)),
            sn_core::telegram_send::Delivery::Retry{error,..}=>Err(Error::bad(error)),
        };
    }
    if status == "sending" || status == "sent" {
        return Err(Error::Conflict(format!("рассылка уже в статусе «{status}»")));
    }
    let alive:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM service_heartbeats WHERE service='broadcasts' AND last_seen_at>now()-interval '3 minutes' AND details->>'bot_configured'='true')").fetch_one(&st.pool).await?;
    if !alive {return Err(Error::bad("Служба рассылок не готова. Проверьте sn-worker и токен бота"));}
    if b.scheduled_at.is_some_and(|at| at<=Utc::now()) { return Err(Error::bad("выберите время отправки в будущем")); }
    let scheduled=sqlx::query(
        "UPDATE broadcasts SET status = 'scheduled', scheduled_at = COALESCE($2, now()), last_error=NULL, retry_at=NULL, finished_at=NULL
          WHERE id = $1 AND status IN ('draft','scheduled','canceled')",
    )
    .bind(id)
    .bind(b.scheduled_at)
    .execute(&st.pool)
    .await?;
    if scheduled.rows_affected()==0 { return Err(Error::Conflict("рассылка уже отправляется".into())); }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'broadcast.send', 'broadcast', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "status": "scheduled" })))
}

/// Остановка рассылки. Уже отправленные сообщения не отзываются —
/// останавливается только очередь.
async fn broadcast_cancel(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let sent: Option<i32> = sqlx::query_scalar(
        "UPDATE broadcasts SET status = 'canceled', finished_at = now()
          WHERE id = $1 AND status IN ('scheduled', 'sending')
      RETURNING sent_count",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;

    match sent {
        Some(n) => Ok(Json(json!({ "ok": true, "already_sent": n }))),
        None => Err(Error::Conflict("рассылку уже нельзя отменить".into())),
    }
}

// ─────────────────────────── инструменты ───────────────────────────

#[derive(Deserialize, Default)]
struct InspectorQuery { client_id: Option<i64>, q: Option<String>, platform: Option<String>, offset: Option<i64>, limit: Option<i64>, paginated: Option<bool> }

async fn srh_list(_a: CurrentAdmin, State(st): State<AppState>, Query(q): Query<InspectorQuery>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT r.requested_at, r.user_agent, r.ip::text AS ip, r.response_code, c.username, c.short_id, c.id AS client_id
           FROM subscription_requests r JOIN clients c ON c.id = r.client_id
          WHERE ($4::bigint IS NULL OR r.client_id=$4) AND ($1::text IS NULL OR concat_ws(' ',c.username,c.short_id,r.user_agent,r.ip::text) ILIKE '%' || $1 || '%')
          ORDER BY r.requested_at DESC, r.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(&q.q).bind(q.limit.unwrap_or(200).clamp(1,500)).bind(q.offset.unwrap_or(0).max(0)).bind(q.client_id)
    .fetch_all(&st.pool)
    .await?;

    let items = json!(rows
        .iter()
        .map(|r| json!({
            "requested_at": dt(r, "requested_at"),
            "user_agent": r.get::<Option<String>, _>("user_agent"),
            "ip": r.get::<Option<String>, _>("ip"),
            "response_code": r.get::<Option<String>, _>("response_code"),
            "username": r.get::<String, _>("username"),
            "short_id": r.get::<String, _>("short_id"),
            "client_id": r.get::<i64, _>("client_id"),
        }))
        .collect::<Vec<_>>());
    if !q.paginated.unwrap_or(false) { return Ok(Json(items)); }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM subscription_requests r JOIN clients c ON c.id=r.client_id WHERE ($2::bigint IS NULL OR r.client_id=$2) AND ($1::text IS NULL OR concat_ws(' ',c.username,c.short_id,r.user_agent,r.ip::text) ILIKE '%' || $1 || '%')")
        .bind(&q.q).bind(q.client_id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"items":items,"total":total})))
}

/// Все устройства платформы. Помечаем те, чей HWID встречается у разных
/// клиентов, — это и есть признак шаринга подписки.
async fn devices_list(_a: CurrentAdmin, State(st): State<AppState>, Query(q): Query<InspectorQuery>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT d.hwid, d.platform, d.model, d.app_version, d.first_seen_at, d.last_seen_at,
                c.username, c.id AS client_id,
                (SELECT count(DISTINCT d2.client_id) FROM devices d2 WHERE d2.hwid = d.hwid) AS shared_by
           FROM devices d JOIN clients c ON c.id = d.client_id
          WHERE c.deleted_at IS NULL
            AND ($1::text IS NULL OR concat_ws(' ',c.username,d.hwid,d.model,d.app_version) ILIKE '%' || $1 || '%')
            AND ($2::text IS NULL OR lower(d.platform)=lower($2))
          ORDER BY d.last_seen_at DESC, d.id DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.q).bind(&q.platform).bind(q.limit.unwrap_or(500).clamp(1,500)).bind(q.offset.unwrap_or(0).max(0))
    .fetch_all(&st.pool)
    .await?;

    let items = json!(rows
        .iter()
        .map(|r| json!({
            "hwid": r.get::<String, _>("hwid"),
            "platform": r.get::<Option<String>, _>("platform"),
            "model": r.get::<Option<String>, _>("model"),
            "app_version": r.get::<Option<String>, _>("app_version"),
            "username": r.get::<String, _>("username"),
            "client_id": r.get::<i64, _>("client_id"),
            "shared_by": r.get::<i64, _>("shared_by"),
            "first_seen_at": dt(r, "first_seen_at"),
            "last_seen_at": dt(r, "last_seen_at"),
        }))
        .collect::<Vec<_>>());
    if !q.paginated.unwrap_or(false) { return Ok(Json(items)); }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM devices d JOIN clients c ON c.id=d.client_id WHERE c.deleted_at IS NULL AND ($1::text IS NULL OR concat_ws(' ',c.username,d.hwid,d.model,d.app_version) ILIKE '%' || $1 || '%') AND ($2::text IS NULL OR lower(d.platform)=lower($2))")
        .bind(&q.q).bind(&q.platform).fetch_one(&st.pool).await?;
    let summary = sqlx::query("SELECT count(*) AS devices, count(DISTINCT client_id) AS clients, (SELECT count(*) FROM (SELECT d.hwid FROM devices d JOIN clients c ON c.id=d.client_id WHERE c.deleted_at IS NULL GROUP BY d.hwid HAVING count(DISTINCT d.client_id)>1) shared) AS shared FROM devices d JOIN clients c ON c.id=d.client_id WHERE c.deleted_at IS NULL")
        .fetch_one(&st.pool).await?;
    Ok(Json(json!({"items":items,"total":total,"summary":{"devices":summary.get::<i64,_>("devices"),"clients":summary.get::<i64,_>("clients"),"shared":summary.get::<i64,_>("shared")}})))
}

/// Онлайн сейчас: последняя метрика каждой ноды.
async fn sessions_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT DISTINCT ON (n.id) n.id, n.name, n.country_code, m.online_count, m.at
           FROM nodes n JOIN node_metrics m ON m.node_id = n.id
          WHERE n.deleted_at IS NULL
          ORDER BY n.id, m.at DESC",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "node_id": r.get::<i64, _>("id"),
            "node": r.get::<String, _>("name"),
            "country_code": r.get::<String, _>("country_code"),
            "online_count": r.get::<Option<i32>, _>("online_count").unwrap_or(0),
            "at": dt(r, "at"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct ActivityQuery { client_id: Option<i64>, node_id: Option<i64>, offset: Option<i64>, limit: Option<i64> }

async fn activity_list(_a: CurrentAdmin, State(st): State<AppState>, Query(q): Query<ActivityQuery>) -> Result<Json<Value>> {
    let rows = sqlx::query("SELECT a.client_id,c.username,a.node_id,n.name,n.country_code,a.last_seen_at,a.upload_bytes,a.download_bytes FROM client_node_activity a JOIN clients c ON c.id=a.client_id JOIN nodes n ON n.id=a.node_id WHERE c.deleted_at IS NULL AND n.deleted_at IS NULL AND a.last_seen_at > now()-interval '5 minutes' AND ($1::bigint IS NULL OR a.client_id=$1) AND ($2::bigint IS NULL OR a.node_id=$2) ORDER BY a.last_seen_at DESC,a.client_id,a.node_id LIMIT $3 OFFSET $4")
        .bind(q.client_id).bind(q.node_id).bind(q.limit.unwrap_or(50).clamp(1,500)).bind(q.offset.unwrap_or(0).max(0)).fetch_all(&st.pool).await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM client_node_activity a JOIN clients c ON c.id=a.client_id JOIN nodes n ON n.id=a.node_id WHERE c.deleted_at IS NULL AND n.deleted_at IS NULL AND a.last_seen_at > now()-interval '5 minutes' AND ($1::bigint IS NULL OR a.client_id=$1) AND ($2::bigint IS NULL OR a.node_id=$2)")
        .bind(q.client_id).bind(q.node_id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"total":total,"window_minutes":5,"items":rows.iter().map(|r|json!({"client_id":r.get::<i64,_>("client_id"),"username":r.get::<String,_>("username"),"node_id":r.get::<i64,_>("node_id"),"node":r.get::<String,_>("name"),"country_code":r.get::<String,_>("country_code"),"at":dt(r,"last_seen_at"),"upload_bytes":r.get::<i64,_>("upload_bytes"),"download_bytes":r.get::<i64,_>("download_bytes")})).collect::<Vec<_>>()})))
}


async fn bot_alerts_status(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>> {
    let value:Value=sqlx::query_scalar("SELECT jsonb_build_object('pending',count(*) FILTER(WHERE status='pending'),'failed',count(*) FILTER(WHERE status='failed' AND created_at>now()-interval '24 hours'),'last_sent',max(sent_at),'last_error',(SELECT error FROM team_notifications WHERE error IS NOT NULL AND status IN ('failed','pending') ORDER BY id DESC LIMIT 1)) FROM team_notifications").fetch_one(&st.pool).await?;
    Ok(Json(value))
}
async fn bot_alerts_test(CurrentAdmin(admin):CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>> {
    let token=std::env::var("BOT_TOKEN").ok().filter(|t|!t.is_empty()).ok_or_else(||Error::bad("Токен бота не настроен"))?;
    sn_core::alerts::send_test(&st.pool,&token,&st.config.brand_name,&st.config.panel_url).await?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type) VALUES('admin',$1,'bot.alerts_test','bot')").bind(admin.id).execute(&st.pool).await?;
    Ok(Json(json!({"ok":true})))
}

async fn bot_alerts_retry(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>> {
    let s=sn_core::bot_config::load(&st.pool).await?;
    let (chat,topic)=sn_core::alerts::destination(&s)?;
    let result=sqlx::query("UPDATE team_notifications SET status='pending',attempts=0,available_at=now(),error=NULL WHERE status='failed' AND created_at>now()-interval '24 hours' AND chat_id=$1 AND thread_id IS NOT DISTINCT FROM $2").bind(chat).bind(topic).execute(&st.pool).await?;
    Ok(Json(json!({"queued":result.rows_affected()})))
}

async fn broadcast_worker(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>> {
    let r=sqlx::query("SELECT last_seen_at,details,last_seen_at>now()-interval '3 minutes' AS alive FROM service_heartbeats WHERE service='broadcasts'").fetch_optional(&st.pool).await?;
    Ok(Json(match r {Some(r)=>json!({"alive":r.get::<bool,_>("alive"),"last_seen_at":r.get::<DateTime<Utc>,_>("last_seen_at"),"bot_configured":r.get::<Value,_>("details")["bot_configured"]}),None=>json!({"alive":false,"last_seen_at":null,"bot_configured":false})}))
}
async fn broadcast_details(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>)->Result<Json<Value>> {
    let r=sqlx::query("SELECT last_error,retry_at,status::text AS status FROM broadcasts WHERE id=$1").bind(id).fetch_optional(&st.pool).await?.ok_or(Error::NotFound)?;
    let errors=sqlx::query("SELECT d.client_id,c.username,d.error,d.attempts FROM broadcast_deliveries d JOIN clients c ON c.id=d.client_id WHERE d.broadcast_id=$1 AND d.error IS NOT NULL ORDER BY d.client_id LIMIT 100").bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!({"last_error":r.get::<Option<String>,_>("last_error"),"retry_at":dt(&r,"retry_at"),"status":r.get::<String,_>("status"),"errors":errors.iter().map(|r|json!({"client_id":r.get::<i64,_>("client_id"),"username":r.get::<String,_>("username"),"error":r.get::<String,_>("error"),"attempts":r.get::<i32,_>("attempts")})).collect::<Vec<_>>()})))
}

#[cfg(test)]
mod broadcast_content_tests {
    use super::*;
    #[test]
    fn photo_caption_and_patch_semantics() {
        let base=json!({"title":"Photo","body":"hello"});
        let omitted: BroadcastBody=serde_json::from_value(base.clone()).unwrap();
        assert!(omitted.photo_id.is_none());
        let mut value=base;
        value["photo_id"]=Value::Null;
        let removed: BroadcastBody=serde_json::from_value(value.clone()).unwrap();
        assert_eq!(removed.photo_id,Some(None));
        value["photo_id"]=json!(uuid::Uuid::new_v4());
        value["body"]=json!("x".repeat(1025));
        assert!(validate_broadcast(&serde_json::from_value(value.clone()).unwrap()).is_err());
        value["body"]=json!("");
        assert!(validate_broadcast(&serde_json::from_value(value.clone()).unwrap()).is_ok());
        value["photo_id"]=Value::Null;
        assert!(validate_broadcast(&serde_json::from_value(value).unwrap()).is_err());
    }
}

//! HTTP-маршруты панели.
//!
//! Запросы пишем через `sqlx::query` без compile-time макросов: так проект
//! собирается без живой БД, что важно для сторонних установок.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub async fn health(State(st): State<AppState>) -> Result<Json<Value>> {
    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&st.pool).await?;
    Ok(Json(json!({
        "status": "ok",
        "db": one == 1,
        "brand": st.config.brand_name,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

// ───────────────────────────── аутентификация ─────────────────────────────

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
    /// Одноразовый код, если у администратора включена двухфакторная.
    code: Option<String>,
}

async fn login(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<LoginBody>,
) -> Result<Json<Value>> {
    if body.username.len() > 254 || body.password.len() > 1024 || body.code.as_ref().is_some_and(|c| c.len() > 16) {
        return Err(Error::Unauthorized);
    }
    crate::security::auth_attempt(&st, &headers, peer.ip(), Some(&body.username)).await?;
    let (admin, token) =
        sn_core::auth::login(&st.pool, &body.username, &body.password, body.code.as_deref()).await?;
    Ok(Json(json!({ "token": token, "admin": admin })))
}

async fn logout(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>> {
    if let Some(token) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        sn_core::auth::logout(&st.pool, token).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn me(CurrentAdmin(admin): CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query("SELECT key, value FROM settings WHERE key = ANY($1)")
        .bind(vec!["billing.currency", "brand.name", "brand.accent", "subscription.public_url", "panel.public_url", "nodes.engine_version"])
        .fetch_all(&st.pool).await?;
    let mut settings: serde_json::Map<String, Value> = rows.iter().map(|r| (r.get("key"), r.get("value"))).collect();
    settings.insert("subscription.public_url".into(), json!(crate::sub_service::public_sub_url(&st).await));
    Ok(Json(json!({ "admin": admin, "settings": settings })))
}

// ───────────────────────────── панель ─────────────────────────────

pub fn panel_routes() -> Router<AppState> {
    Router::new()
        .route("/api/dashboard", get(dashboard))
        .route("/api/dashboard/subscription-health", get(crate::sub_service::public_health))
        .route("/api/clients", get(clients_list).post(client_create))
        .route("/api/clients/bulk", post(clients_bulk))
        .route(
            "/api/clients/{id}",
            get(client_get).patch(client_update).delete(client_delete),
        )
        .route("/api/clients/{id}/reset-traffic", post(client_reset_traffic))
        .route("/api/clients/{id}/revoke", post(client_revoke))
        .route("/api/clients/{id}/devices", get(client_devices))
        .route("/api/clients/{id}/devices/{hwid}", axum::routing::delete(client_device_unbind))
        .route("/api/clients/{id}/grant", post(client_grant))
        .route("/api/clients/{id}/traffic", get(client_traffic))
        .route("/api/clients/{id}/connection-links", get(client_connection_links))
        .route("/api/squads/{id}/traffic", get(squad_traffic))
        .route("/api/clients/{id}/payments", get(client_payments))
        .route("/api/tariffs", get(tariffs_list))
        .route("/api/payments", get(payments_list))
        .route("/api/payments/{id}/refund", post(payment_refund))
        .route("/api/nodes", get(nodes_list))
        .route("/api/hosts", get(hosts_list))
        .route("/api/squads", get(squads_list))
        .route("/api/profiles", get(profiles_list).post(profile_create))
        .route("/api/settings", get(settings_get).patch(settings_set))
        .route("/api/keygen/reality", post(keygen_reality))
        .route("/api/profiles/validate", post(profile_validate))
        .route(
            "/api/profiles/{id}",
            axum::routing::patch(profile_save).delete(profile_delete),
        )
}

fn dt(row: &sqlx::postgres::PgRow, col: &str) -> Option<String> {
    row.try_get::<Option<DateTime<Utc>>, _>(col)
        .ok()
        .flatten()
        .map(|d| d.to_rfc3339())
}

// ── сводка ──

async fn dashboard(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let clients = sqlx::query(
        "SELECT
            count(*)                                    AS total,
            count(*) FILTER (WHERE status = 'active')   AS active,
            count(*) FILTER (WHERE status = 'expired')  AS expired,
            count(*) FILTER (WHERE status = 'limited')  AS limited,
            count(*) FILTER (WHERE status = 'disabled') AS disabled
         FROM client_overview",
    )
    .fetch_one(&st.pool)
    .await?;

    let currency = sn_core::money::service_currency(&st.pool).await;
    // No conversion between Stars and the service currency.
    let revenue = sqlx::query(
        "SELECT
            COALESCE(sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0)) FILTER (WHERE paid_at >= date_trunc('month', now())), 0)::bigint AS month_minor,
            COALESCE(sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0)) FILTER (WHERE paid_at >= (now() AT TIME ZONE 'UTC')::date), 0)::bigint               AS today_minor,
            COALESCE(sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0))::bigint, 0)                                                      AS total_minor,
            count(*) FILTER (WHERE paid_at >= (now() AT TIME ZONE 'UTC')::date)                                     AS today_count
         FROM payments WHERE status = 'success' AND currency=$1",
    )
    .bind(&currency)
    .fetch_one(&st.pool)
    .await?;

    let revenue_by_currency: Vec<Value> = sqlx::query("SELECT currency,
        COALESCE(sum(GREATEST(amount_minor-COALESCE((metadata->>'refunded_minor')::bigint,0),0)),0)::bigint AS amount
        FROM payments WHERE status='success' AND paid_at>=date_trunc('month',now()) GROUP BY currency ORDER BY currency")
        .fetch_all(&st.pool).await?.iter().map(|r| json!({"currency":r.get::<String,_>("currency"),"month_minor":r.get::<i64,_>("amount")})).collect();
    let traffic = sqlx::query(
        "SELECT
            COALESCE(sum(upload_bytes + download_bytes) FILTER (WHERE day = (now() AT TIME ZONE 'UTC')::date), 0)::bigint              AS today,
            COALESCE(sum(upload_bytes + download_bytes) FILTER (WHERE day > (now() AT TIME ZONE 'UTC')::date - 7), 0)::bigint          AS week,
            COALESCE(sum(upload_bytes + download_bytes) FILTER (WHERE day > (now() AT TIME ZONE 'UTC')::date - 30), 0)::bigint         AS month
         FROM traffic_usage",
    )
    .fetch_one(&st.pool)
    .await?;

    let nodes = sqlx::query(
        "SELECT count(*) AS total, count(*) FILTER (WHERE status = 'online') AS online
           FROM nodes WHERE deleted_at IS NULL",
    )
    .fetch_one(&st.pool)
    .await?;

    // График выручки по дням за 30 суток, с нулями в пустых днях.
    let series = sqlx::query(
        "SELECT d::date AS day,
                COALESCE((SELECT sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0))::bigint FROM payments p
                           WHERE p.status = 'success' AND p.currency=$1 AND (p.paid_at AT TIME ZONE 'UTC')::date = d::date), 0) AS minor
           FROM generate_series((now() AT TIME ZONE 'UTC')::date - 29, (now() AT TIME ZONE 'UTC')::date, '1 day') d
          ORDER BY day",
    )
    .bind(&currency)
    .fetch_all(&st.pool)
    .await?;

    let revenue_series: Vec<Value> = series
        .iter()
        .map(|r| {
            json!({
                "day": r.get::<chrono::NaiveDate, _>("day").to_string(),
                "amount_minor": r.get::<i64, _>("minor"),
            })
        })
        .collect();

    // Трафик по дням за те же 30 суток. Пустые дни заполняем нулями:
    // разрыв в ряду график покажет как «данных нет», а ноль — как ноль,
    // и это разные вещи. Ряд идёт рядом с выручкой на одном экране.
    let traffic_rows = sqlx::query(
        "SELECT d::date AS day,
                COALESCE((SELECT sum(upload_bytes + download_bytes)::bigint
                            FROM traffic_usage t WHERE t.day = d::date), 0) AS bytes
           FROM generate_series((now() AT TIME ZONE 'UTC')::date - 29, (now() AT TIME ZONE 'UTC')::date, '1 day') d
          ORDER BY day",
    )
    .fetch_all(&st.pool)
    .await?;

    let traffic_series: Vec<Value> = traffic_rows
        .iter()
        .map(|r| {
            json!({
                "day": r.get::<chrono::NaiveDate, _>("day").to_string(),
                "bytes": r.get::<i64, _>("bytes"),
            })
        })
        .collect();

    Ok(Json(json!({
        "clients": {
            "total":    clients.get::<i64, _>("total"),
            "active":   clients.get::<i64, _>("active"),
            "expired":  clients.get::<i64, _>("expired"),
            "limited":  clients.get::<i64, _>("limited"),
            "disabled": clients.get::<i64, _>("disabled"),
        },
        "revenue_by_currency": revenue_by_currency,
        "revenue": {
            "currency":currency,
            "month_minor": revenue.get::<i64, _>("month_minor"),
            "today_minor": revenue.get::<i64, _>("today_minor"),
            "total_minor": revenue.get::<i64, _>("total_minor"),
            "today_count": revenue.get::<i64, _>("today_count"),
            // Была зашита строкой: на главной панели выручка подписывалась
            // долларом независимо от того, в чём работает сервис.
            "currency": sn_core::money::service_currency(&st.pool).await,
        },
        "traffic": {
            "today_bytes": traffic.get::<i64, _>("today"),
            "week_bytes":  traffic.get::<i64, _>("week"),
            "month_bytes": traffic.get::<i64, _>("month"),
        },
        "nodes": {
            "total":  nodes.get::<i64, _>("total"),
            "online": nodes.get::<i64, _>("online"),
        },
        "revenue_series": revenue_series,
        "traffic_series": traffic_series,
    })))
}

// ── клиенты ──

#[derive(Deserialize)]
struct ClientsQuery {
    partner: Option<String>,
    tariff: Option<String>,
    q: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Колонки витрины клиента. `status` приводим к тексту: это enum в БД,
/// а enum не сравнивается с параметром-строкой и не декодируется в String.
const CLIENT_COLUMNS: &str = "id, public_id, username, status::text AS status, tag, note, short_id, \
     last_online_at, created_at, tariff_code, expires_at, traffic_used_bytes, \
     traffic_limit_bytes, device_limit, autorenew, device_count, ltv_minor, ltv_currency, ltv_by_currency, payment_count, \
     traffic_total_bytes, referrer_title, referrer_slug, \
     reset_strategy::text AS reset_strategy, traffic_reset_at, \
     ARRAY(SELECT s.name FROM squads s \
             JOIN client_squads cs ON cs.squad_id = s.id \
            WHERE cs.client_id = client_overview.id ORDER BY s.name) AS squads";

const CLIENT_FILTER: &str = "($1::text IS NULL OR status::text = lower($1))
 AND ($3::text IS NULL OR tariff_code = $3)
 AND ($4::text IS NULL OR referrer_slug=$4)
 AND ($2::text IS NULL OR username ILIKE '%' || ltrim($2, '@') || '%'
      OR short_id ILIKE '%' || $2 || '%' OR public_id::text ILIKE '%' || $2 || '%'
      OR COALESCE(tag,'') ILIKE '%' || $2 || '%'
      OR EXISTS(SELECT 1 FROM client_identities i WHERE i.client_id=client_overview.id
                AND i.value ILIKE '%' || $2 || '%'))";

fn client_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id":                  r.get::<i64, _>("id"),
        "public_id":           r.get::<uuid::Uuid, _>("public_id").to_string(),
        "username":            r.get::<String, _>("username"),
        "status":              r.get::<String, _>("status"),
        // Стратегия сброса и момент следующего: счётчик обнуляется по
        // расписанию тарифа, и без этих двух полей обнуление выглядит
        // как поломка счётчика.
        "reset_strategy":      r.get::<Option<String>, _>("reset_strategy"),
        "traffic_reset_at":    r.get::<Option<DateTime<Utc>>, _>("traffic_reset_at"),
        "tag":                 r.get::<Option<String>, _>("tag"),
        // Заметка администратора раньше писалась в базу, но обратно не
        // отдавалась: при повторном открытии карточки поле было пустым.
        "note":                r.try_get::<Option<String>, _>("note").ok().flatten(),
        // Трафик за всё время: traffic_used_bytes обнуляется при сбросе
        // периода, поэтому историей он не является.
        "traffic_total_bytes": r.try_get::<i64, _>("traffic_total_bytes").unwrap_or(0),
        // Откуда пришёл клиент. Пусто — пришёл сам, без партнёрской ссылки.
        "referrer_title":      r.try_get::<Option<String>, _>("referrer_title").ok().flatten(),
        "referrer_slug":       r.try_get::<Option<String>, _>("referrer_slug").ok().flatten(),
        "short_id":            r.get::<String, _>("short_id"),
        "tariff_code":         r.get::<Option<String>, _>("tariff_code"),
        "expires_at":          dt(r, "expires_at"),
        "traffic_used_bytes":  r.try_get::<Option<i64>, _>("traffic_used_bytes").ok().flatten(),
        "traffic_limit_bytes": r.try_get::<Option<i64>, _>("traffic_limit_bytes").ok().flatten(),
        "device_limit":        r.try_get::<Option<i32>, _>("device_limit").ok().flatten(),
        "device_count":        r.try_get::<Option<i64>, _>("device_count").ok().flatten(),
        "autorenew":           r.try_get::<Option<bool>, _>("autorenew").ok().flatten(),
        "ltv_currency":        r.get::<String,_>("ltv_currency"),
        "ltv_by_currency":     r.get::<Value,_>("ltv_by_currency"),
        "ltv_minor":           r.try_get::<Option<i64>, _>("ltv_minor").ok().flatten(),
        "payment_count":       r.try_get::<Option<i64>, _>("payment_count").ok().flatten(),
        // Сквады нужны списку, а не только карточке: редактор проставляет
        // по ним галочки, и без них сохранение снимало бы все локации.
        "squads":              r.try_get::<Vec<String>, _>("squads").unwrap_or_default(),
        "last_online_at":      dt(r, "last_online_at"),
        "created_at":          dt(r, "created_at"),
    })
}

async fn clients_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Query(q): Query<ClientsQuery>,
) -> Result<Json<Value>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    let search = q.q.filter(|s| !s.trim().is_empty());

    // Пустая строка статуса = «все», иначе фильтруем.
    let status = q.status.filter(|s| !s.trim().is_empty());

    let sql = format!("SELECT {CLIENT_COLUMNS} FROM client_overview WHERE {CLIENT_FILTER}
          ORDER BY created_at DESC, id DESC LIMIT $5 OFFSET $6");
    let tariff = q.tariff.filter(|s| !s.is_empty());
    let partner=q.partner.filter(|s| !s.is_empty());
    let rows = sqlx::query(&sql).bind(&status).bind(&search).bind(&tariff).bind(&partner)
        .bind(limit).bind(offset).fetch_all(&st.pool).await?;
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM client_overview WHERE {CLIENT_FILTER}"))
        .bind(&status).bind(&search).bind(&tariff).bind(&partner).fetch_one(&st.pool).await?;

    Ok(Json(json!({
        "items": rows.iter().map(client_json).collect::<Vec<_>>(),
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

async fn client_get(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let row = sqlx::query(&format!(
        "SELECT {CLIENT_COLUMNS} FROM client_overview WHERE id = $1"
    ))
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;

    let identities = sqlx::query(
        "SELECT kind::text AS kind, value, is_verified FROM client_identities WHERE client_id = $1",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;

    let squads: Vec<String> = sqlx::query_scalar(
        "SELECT s.name FROM squads s
           JOIN client_squads cs ON cs.squad_id = s.id
          WHERE cs.client_id = $1 ORDER BY s.name",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;

    let vpn_uuid: uuid::Uuid = sqlx::query_scalar("SELECT vpn_uuid FROM clients WHERE id = $1")
        .bind(id)
        .fetch_one(&st.pool)
        .await?;

    let mut out = client_json(&row);
    out["vpn_uuid"] = json!(vpn_uuid.to_string());
    out["squads"] = json!(squads);
    out["external_squad_id"]=json!(sqlx::query_scalar::<_,Option<i64>>("SELECT external_squad_id FROM clients WHERE id=$1").bind(id).fetch_one(&st.pool).await?);
    out["identities"] = json!(identities
        .iter()
        .map(|r| json!({
            "kind": r.get::<String, _>("kind"),
            "value": r.get::<String, _>("value"),
            "is_verified": r.get::<bool, _>("is_verified"),
        }))
        .collect::<Vec<_>>());
    // Адрес сабки берём из настроек: её могли перенести на другой домен,
    // и ссылка, собранная из переменной окружения панели, была бы мёртвой.
    let sub_base = crate::sub_service::public_sub_url(&st).await;
    out["subscription_url"] = json!(format!(
        "{}/s/{}",
        sub_base.trim_end_matches('/'),
        row.get::<String, _>("short_id")
    ));
    Ok(Json(out))
}

#[derive(Deserialize)]
struct CreateClient {
    username: String,
    tariff_id: Option<i64>,
    telegram_id: Option<String>,
    email: Option<String>,
    tag: Option<String>,
    expires_days: Option<i64>,
}

/// Короткий идентификатор для ссылки подписки.
/// Без похожих символов (0/O, 1/l), чтобы диктовать голосом.
fn generate_short_id() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzACDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

async fn client_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(body): Json<CreateClient>,
) -> Result<Json<Value>> {
    if body.username.trim().is_empty() {
        return Err(Error::bad("username не может быть пустым"));
    }

    // Клиент, подписка и идентичности создаются одной транзакцией:
    // клиент без подписки — сломанное состояние.
    let mut tx = st.pool.begin().await?;

    let client_id: i64 = sqlx::query_scalar(
        "INSERT INTO clients (username, short_id, tag) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(body.username.trim())
    .bind(generate_short_id())
    .bind(&body.tag)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(tg) = body.telegram_id.as_ref().filter(|s| !s.trim().is_empty()) {
        sqlx::query(
            "INSERT INTO client_identities (client_id, kind, value) VALUES ($1, 'telegram', $2)",
        )
        .bind(client_id)
        .bind(tg.trim())
        .execute(&mut *tx)
        .await?;
    }
    if let Some(email) = body.email.as_ref().filter(|s| !s.trim().is_empty()) {
        sqlx::query(
            "INSERT INTO client_identities (client_id, kind, value) VALUES ($1, 'email', lower($2))",
        )
        .bind(client_id)
        .bind(email.trim())
        .execute(&mut *tx)
        .await?;
    }

    let days = body.expires_days.unwrap_or(30);
    // LEFT JOIN, а не `FROM tariffs WHERE id = $2`: при создании клиента без
    // тарифа тот запрос не возвращал ни строки, и подписка не создавалась
    // вообще. Такой клиент оставался без срока, лимитов и локаций, а
    // «начислить дни» ему отвечало «не найдено» — чинить это в панели было
    // нечем. Без тарифа берём разумные умолчания: срок задан, лимиты сняты.
    sqlx::query(
        "INSERT INTO subscriptions
            (client_id, tariff_id, expires_at, device_limit, traffic_limit_bytes, reset_strategy)
         SELECT $1, t.id, now() + ($3 || ' days')::interval,
                COALESCE(t.device_limit, 1),
                t.traffic_limit_bytes,
                COALESCE(t.reset_strategy, 'no_reset')
           FROM (SELECT 1) AS one
           LEFT JOIN tariffs t ON t.id = $2",
    )
    .bind(client_id)
    .bind(body.tariff_id)
    .bind(days.to_string())
    .execute(&mut *tx)
    .await?;

    // Доступы наследуем от тарифа.
    sqlx::query(
        "INSERT INTO client_squads (client_id, squad_id)
         SELECT $1, ts.squad_id FROM tariff_squads ts WHERE ts.tariff_id = $2
         ON CONFLICT DO NOTHING",
    )
    .bind(client_id)
    .bind(body.tariff_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "id": client_id })))
}

#[derive(Deserialize)]
struct UpdateClient {
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    external_squad_id: Option<Option<i64>>,
    username: Option<String>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    telegram_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    email: Option<Option<String>>,
    status: Option<String>,
    /// Тариф целиком: меняет лимиты и доступы под новый.
    tariff_id: Option<i64>,
    /// Сквады клиента. Заменяют текущий набор целиком.
    squad_names: Option<Vec<String>>,
    tag: Option<String>,
    note: Option<String>,
    autorenew: Option<bool>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    expires_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    traffic_limit_bytes: Option<Option<i64>>,
    device_limit: Option<i32>,
    /// Как часто обнулять счётчик. В карточке это поле было, но никуда
    /// не отправлялось: администратор менял его и не понимал, почему
    /// расписание осталось прежним.
    reset_strategy: Option<String>,
}

impl UpdateClient {
    fn changes_protected_fields(&self) -> bool {
        self.external_squad_id.is_some() || self.username.is_some()
            || self.telegram_id.is_some() || self.email.is_some()
            || self.tariff_id.is_some() || self.squad_names.is_some()
            || self.autorenew.is_some() || self.expires_at.is_some()
            || self.traffic_limit_bytes.is_some() || self.device_limit.is_some()
            || self.reset_strategy.is_some()
    }
}

#[cfg(test)]
mod client_permission_tests {
    use super::*;

    #[test]
    fn support_cannot_rebind_identity_or_change_subscription_entitlements() {
        for value in [json!({"telegram_id":"123"}), json!({"telegram_id":null}),
            json!({"email":"a@example.test"}), json!({"username":"other"}),
            json!({"external_squad_id":null}), json!({"tariff_id":1}),
            json!({"squad_names":[]}), json!({"autorenew":true}),
            json!({"expires_at":null}), json!({"traffic_limit_bytes":null}),
            json!({"device_limit":999}), json!({"reset_strategy":"no_reset"})] {
            let patch: UpdateClient = serde_json::from_value(value.clone()).unwrap();
            assert!(patch.changes_protected_fields(), "{value}");
        }
        let patch: UpdateClient = serde_json::from_value(json!({"note":"Resolved", "tag":"checked", "status":"disabled"})).unwrap();
        assert!(!patch.changes_protected_fields());
    }
}

async fn client_update(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateClient>,
) -> Result<Json<Value>> {
    // Support may resolve tickets and annotate an account, but cannot rebind a
    // Telegram identity (an indirect login bypass), sell access or change limits.
    if admin.role == "support" && body.changes_protected_fields() {
        return Err(Error::Forbidden);
    }
    if body.username.as_deref().is_some_and(|v| v.trim().is_empty()) { return Err(Error::bad("username не может быть пустым")); }
    if body.device_limit.is_some_and(|v| v < 1) { return Err(Error::bad("лимит устройств должен быть больше нуля")); }
    if body.traffic_limit_bytes.flatten().is_some_and(|v| v <= 0) { return Err(Error::bad("лимит трафика должен быть больше нуля или пустым")); }
    let mut tx = st.pool.begin().await?;
    sn_core::addons::maintain(&mut tx,id,false).await?;
    if let Some(name) = &body.username {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clients WHERE username=$1 AND id<>$2 AND deleted_at IS NULL)")
            .bind(name.trim()).bind(id).fetch_one(&mut *tx).await?;
        if exists { return Err(Error::Conflict("username уже используется".into())); }
    }
    if let Some(tariff) = body.tariff_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tariffs WHERE id=$1)").bind(tariff).fetch_one(&mut *tx).await?;
        if !exists { return Err(Error::bad("тариф больше не существует")); }
    }

    if let Some(squad)=body.external_squad_id {
        if let Some(squad)=squad {let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM external_squads WHERE id=$1 AND is_active)").bind(squad).fetch_one(&mut *tx).await?;if !ok{return Err(Error::bad("Внешний сквад не найден или выключен"))}}
        sqlx::query("UPDATE clients SET external_squad_id=$2 WHERE id=$1 AND deleted_at IS NULL").bind(id).bind(squad).execute(&mut *tx).await?;
    }
    // COALESCE: не переданные поля не затираются.
    let updated = sqlx::query(
        "UPDATE clients
            SET status = COALESCE($2::client_status, status),
                tag    = COALESCE($3, tag),
                note   = COALESCE($4, note),
                username = COALESCE($5, username)
          WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(&body.status)
    .bind(&body.tag)
    .bind(&body.note)
    .bind(body.username.as_deref().map(str::trim))
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    for (kind, incoming) in [("telegram", &body.telegram_id), ("email", &body.email)] {
        if let Some(value) = incoming {
            let value = value.as_deref().unwrap_or("").trim();
            if !value.is_empty() {
                if kind == "telegram" && value.parse::<i64>().ok().filter(|id| *id > 0).is_none() { return Err(Error::bad("Telegram ID должен быть положительным числом")); }
                if kind == "email" && (!value.contains('@') || value.contains(char::is_whitespace)) { return Err(Error::bad("проверьте адрес email")); }
            }
            let normalized = if kind == "email" { value.to_lowercase() } else { value.to_owned() };
            // Unchanged identities retain their verification state.
            sqlx::query("DELETE FROM client_identities WHERE client_id=$1 AND kind=$2::identity_kind AND value<>$3")
                .bind(id).bind(kind).bind(&normalized).execute(&mut *tx).await?;
            if !value.is_empty() {
                let owner: Option<i64> = sqlx::query_scalar("SELECT client_id FROM client_identities WHERE kind=$1::identity_kind AND value=$2")
                    .bind(kind).bind(&normalized).fetch_optional(&mut *tx).await?;
                if owner.is_some_and(|owner| owner != id) { return Err(Error::Conflict("контакт уже привязан к другому клиенту".into())); }
                if owner.is_none() {
                    sqlx::query("INSERT INTO client_identities(client_id,kind,value) VALUES ($1,$2::identity_kind,$3)")
                        .bind(id).bind(kind).bind(&normalized).execute(&mut *tx).await?;
                }
            }
        }
    }

    // Смена тарифа: лимиты подтягиваются из нового, срок не трогаем —
    // оплаченные дни при переводе на другой тариф сгорать не должны.
    if let Some(tariff_id) = body.tariff_id {
        sqlx::query(
            "UPDATE subscriptions s
                SET tariff_id           = t.id,
                    device_limit        = t.device_limit,
                    traffic_limit_bytes = t.traffic_limit_bytes,
                    reset_strategy      = t.reset_strategy
               FROM tariffs t
              WHERE t.id = $2 AND s.client_id = $1 AND s.is_current",
        )
        .bind(id)
        .bind(tariff_id)
        .execute(&mut *tx)
        .await?;

        sn_core::addons::restore_after_plan(&mut tx,id).await?;
        // Доступы нового тарифа добавляем, старые не срываем: клиент мог
        // получить локацию отдельно, и молча её отнимать нельзя.
        sqlx::query(
            "INSERT INTO client_squads (client_id, squad_id)
             SELECT $1, ts.squad_id FROM tariff_squads ts WHERE ts.tariff_id = $2
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(tariff_id)
        .execute(&mut *tx)
        .await?;
    }

    // Явные значения применяем ПОСЛЕ тарифа: администратор мог задать
    // лимит, отличный от тарифного, и смена тарифа не должна его затирать.
    sqlx::query(
        "UPDATE subscriptions
            SET autorenew           = COALESCE($2, autorenew),
                expires_at          = CASE WHEN $7 THEN $3 ELSE expires_at END,
                traffic_limit_bytes = CASE WHEN $8 THEN $4 ELSE traffic_limit_bytes END,
                device_limit        = COALESCE($5, device_limit),
                reset_strategy      = COALESCE($6::reset_strategy, reset_strategy),
                -- Сменили стратегию — пересчитываем и момент следующего
                -- сброса. Иначе счётчик обнулился бы по старому расписанию:
                -- поставили «без сброса», а он всё равно обнулился завтра.
                traffic_reset_at    = CASE
                    WHEN $6::reset_strategy IS NULL THEN traffic_reset_at
                    WHEN $6::reset_strategy = 'no_reset' THEN NULL
                    WHEN $6::reset_strategy = 'day'   THEN now() + interval '1 day'
                    WHEN $6::reset_strategy = 'week'  THEN now() + interval '7 days'
                    WHEN $6::reset_strategy = 'month' THEN now() + interval '1 month'
                    ELSE traffic_reset_at END
          WHERE client_id = $1 AND is_current",
    )
    .bind(id)
    .bind(body.autorenew)
    .bind(body.expires_at.flatten())
    .bind(body.traffic_limit_bytes.flatten())
    .bind(body.device_limit)
    .bind(body.reset_strategy.as_deref())
    .bind(body.expires_at.is_some())
    .bind(body.traffic_limit_bytes.is_some())
    .execute(&mut *tx)
    .await?;

    // Явный список сквадов заменяет набор целиком — иначе снять локацию
    // из панели было бы нечем.
    if let Some(names) = &body.squad_names {
        sqlx::query("DELETE FROM client_squads WHERE client_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for name in names {
            sqlx::query(
                "INSERT INTO client_squads (client_id, squad_id)
                 SELECT $1, id FROM squads WHERE name = $2
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        }
    }

    if body.expires_at.is_some() && body.status.is_none() {
        sqlx::query("UPDATE clients c SET status='active' WHERE c.id=$1 AND c.status IN ('expired','limited')
            AND EXISTS(SELECT 1 FROM subscriptions s WHERE s.client_id=c.id AND s.is_current
                AND (s.expires_at IS NULL OR s.expires_at>now())
                AND (s.traffic_limit_bytes IS NULL OR s.traffic_used_bytes<s.traffic_limit_bytes))")
            .bind(id).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id)
        VALUES ('admin',$1,'client.update','client',$2)")
        .bind(admin.id).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct GrantBody {
    /// Сколько дней добавить. Отрицательное значение отнимает.
    days: Option<i32>,
    /// Сколько гигабайт добавить к лимиту трафика.
    traffic_gb: Option<f64>,
    /// Причина — попадёт в журнал: «подарок за отзыв», «компенсация».
    reason: Option<String>,
}

/// Начисление: продлить срок и/или добавить трафик вручную.
///
/// Отдельная операция, а не правка даты: продлевать нужно **от большей из
/// дат** — истёкшая подписка считается от сегодня, иначе начисленные дни
/// уходят в прошлое и клиент остаётся без доступа.
async fn client_grant(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<GrantBody>,
) -> Result<Json<Value>> {
    let days = b.days.unwrap_or(0);
    let gb = b.traffic_gb.unwrap_or(0.0);
    if days == 0 && gb == 0.0 {
        return Err(Error::bad("нечего начислять: укажите дни или трафик"));
    }

    let mut tx = st.pool.begin().await?;

    let row = sqlx::query(
        "UPDATE subscriptions
            SET expires_at = GREATEST(COALESCE(expires_at, now()), now())
                             + ($2 || ' days')::interval,
                -- Безлимит трогать нельзя: NULL означает «без ограничения»,
                -- и прибавление к нему превратило бы его в лимит.
                traffic_limit_bytes = CASE
                    WHEN traffic_limit_bytes IS NULL THEN NULL
                    ELSE GREATEST(0, traffic_limit_bytes + $3)
                END
          WHERE client_id = $1 AND is_current
      RETURNING expires_at, traffic_limit_bytes",
    )
    .bind(id)
    .bind(days.to_string())
    .bind((gb * 1024.0 * 1024.0 * 1024.0) as i64)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;

    // Начисление возвращает доступ тем, кто отвалился по сроку или трафику.
    sqlx::query(
        "UPDATE clients SET status = 'active'
          WHERE id = $1 AND status IN ('expired', 'limited')",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id, payload)
         VALUES ('admin', $1, 'client.grant', 'client', $2, $3)",
    )
    .bind(admin.id)
    .bind(id)
    .bind(json!({ "days": days, "traffic_gb": gb, "reason": b.reason }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(json!({
        "expires_at": row.get::<Option<DateTime<Utc>>, _>("expires_at").map(|d| d.to_rfc3339()),
        "traffic_limit_bytes": row.get::<Option<i64>, _>("traffic_limit_bytes"),
    })))
}

#[derive(Deserialize)]
struct TrafficQuery {
    days: Option<i32>,
}

/// Расход трафика клиента: по дням и по нодам.
///
/// Это единственные настоящие данные, которые у нас есть о клиенте:
/// счётчики движка, собранные агентом. Графики строятся по ним.
async fn client_connection_links(_a: CurrentAdmin, State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let short: String = sqlx::query_scalar("SELECT short_id FROM clients WHERE id=$1 AND deleted_at IS NULL")
        .bind(id).fetch_optional(&st.pool).await?.ok_or(Error::NotFound)?;
    let data = crate::sub_routes::load_sub_data(&st, &short).await?;
    let mut links = Vec::new();
    if data["status"] == "active" {
        for value in data["hosts"].as_array().into_iter().flatten() {
            let mut value = value.clone();
            value["uuid"] = data["vpn_uuid"].clone();
            let host: sn_sub::formats::HostEntry = serde_json::from_value(value)
                .map_err(|_| Error::Internal("invalid subscription host".into()))?;
            let link = sn_sub::formats::render_plain(std::slice::from_ref(&host));
            if !link.trim().is_empty() {
                links.push(json!({"name":host.remark,"protocol":host.protocol,"link":link.trim()}));
            }
        }
    }
    Ok(Json(json!({"status":data["status"],"links":links})))
}

async fn squad_traffic(_a: CurrentAdmin, State(st): State<AppState>, Path(id): Path<i64>, Query(q): Query<TrafficQuery>) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM squads WHERE id=$1)").bind(id).fetch_one(&st.pool).await?;
    if !exists { return Err(Error::NotFound); }
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM client_squads cs JOIN clients c ON c.id=cs.client_id WHERE cs.squad_id=$1 AND c.deleted_at IS NULL").bind(id).fetch_one(&st.pool).await?;
    let days = q.days.unwrap_or(30).clamp(1,90);
    let rows = sqlx::query("SELECT tu.day, n.id AS node_id, n.name, sum(tu.upload_bytes+tu.download_bytes)::bigint AS bytes FROM traffic_usage tu JOIN client_squads cs ON cs.client_id=tu.client_id JOIN clients c ON c.id=cs.client_id JOIN nodes n ON n.id=tu.node_id WHERE cs.squad_id=$1 AND c.deleted_at IS NULL AND tu.day >= (now() AT TIME ZONE 'UTC')::date - $2::int GROUP BY tu.day,n.id,n.name ORDER BY tu.day,n.id")
        .bind(id).bind(days).fetch_all(&st.pool).await?;
    Ok(Json(json!({"members":members,"items":rows.iter().map(|r|json!({"day":r.get::<chrono::NaiveDate,_>("day").to_string(),"node_id":r.get::<i64,_>("node_id"),"name":r.get::<String,_>("name"),"bytes":r.get::<i64,_>("bytes")})).collect::<Vec<_>>()})))
}

async fn client_traffic(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<TrafficQuery>,
) -> Result<Json<Value>> {
    let days = q.days.unwrap_or(30).clamp(1, 90);

    let by_day = sqlx::query(
        "SELECT day, sum(upload_bytes + download_bytes)::bigint AS bytes
           FROM traffic_usage
          WHERE client_id = $1 AND day >= (now() AT TIME ZONE 'UTC')::date - $2::int
          GROUP BY day ORDER BY day",
    )
    .bind(id)
    .bind(days)
    .fetch_all(&st.pool)
    .await?;

    let by_node = sqlx::query(
        "SELECT n.name, n.country_code,
                sum(tu.upload_bytes + tu.download_bytes)::bigint AS bytes
           FROM traffic_usage tu
           JOIN nodes n ON n.id = tu.node_id
          WHERE tu.client_id = $1 AND tu.day >= (now() AT TIME ZONE 'UTC')::date - $2::int
          GROUP BY n.name, n.country_code ORDER BY bytes DESC",
    )
    .bind(id)
    .bind(days)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!({
        "days": by_day.iter().map(|r| json!({
            "day": r.get::<chrono::NaiveDate, _>("day").to_string(),
            "bytes": r.get::<i64, _>("bytes"),
        })).collect::<Vec<_>>(),
        "nodes": by_node.iter().map(|r| json!({
            "name": r.get::<String, _>("name"),
            "country_code": r.get::<String, _>("country_code"),
            "bytes": r.get::<i64, _>("bytes"),
        })).collect::<Vec<_>>(),
    })))
}

/// Отвязать устройство: клиент сможет войти с нового.
async fn client_device_unbind(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path((id, hwid)): Path<(i64, String)>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM devices WHERE client_id = $1 AND hwid = $2")
        .bind(id)
        .bind(&hwid)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn client_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Мягкое удаление: платежи и история должны пережить клиента.
    let res = sqlx::query("UPDATE clients SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn client_reset_traffic(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>)->Result<Json<Value>> {
    let mut tx=st.pool.begin().await?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clients WHERE id=$1 AND deleted_at IS NULL)").bind(id).fetch_one(&mut *tx).await?;
    if !exists{return Err(Error::NotFound);}
    sn_core::addons::maintain(&mut tx,id,true).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

async fn client_revoke(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Меняем и short_id, и vpn_uuid: старая ссылка и старые конфиги умирают.
    let row = sqlx::query(
        "UPDATE clients SET short_id = $2, vpn_uuid = gen_random_uuid()
          WHERE id = $1 AND deleted_at IS NULL
      RETURNING short_id, vpn_uuid",
    )
    .bind(id)
    .bind(generate_short_id())
    .fetch_optional(&st.pool)
    .await?
    .ok_or(Error::NotFound)?;

    let sub_base = crate::sub_service::public_sub_url(&st).await;
    Ok(Json(json!({
        "short_id": row.get::<String, _>("short_id"),
        "vpn_uuid": row.get::<uuid::Uuid, _>("vpn_uuid").to_string(),
        "subscription_url": format!("{}/s/{}",
            sub_base.trim_end_matches('/'),
            row.get::<String, _>("short_id")),
    })))
}

async fn client_devices(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT hwid, platform, model, app_version, first_seen_at, last_seen_at
           FROM devices WHERE client_id = $1 ORDER BY last_seen_at DESC",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "hwid": r.get::<String, _>("hwid"),
            "platform": r.get::<Option<String>, _>("platform"),
            "model": r.get::<Option<String>, _>("model"),
            "app_version": r.get::<Option<String>, _>("app_version"),
            "first_seen_at": dt(r, "first_seen_at"),
            "last_seen_at": dt(r, "last_seen_at"),
        }))
        .collect::<Vec<_>>())))
}

async fn client_payments(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT p.id, p.client_id, p.status::text AS status, p.kind::text AS kind, p.amount_minor, p.currency,
                p.provider, p.provider_txid, p.period_days, p.error_message, p.paid_at, p.created_at,
                COALESCE((p.metadata->>'refunded_minor')::bigint, 0) AS refunded_minor,
                COALESCE(p.addon_snapshot->>'title',t.code) AS tariff_code
           FROM payments p LEFT JOIN tariffs t ON t.id = p.tariff_id
          WHERE p.client_id = $1 ORDER BY p.created_at DESC",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(rows.iter().map(payment_json).collect::<Vec<_>>())))
}

// ── тарифы ──

async fn tariffs_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT t.id, t.code, t.title, t.description, t.locales, t.badge, t.sort_order,
                t.device_limit, t.traffic_limit_bytes,
                t.reset_strategy::text AS reset_strategy,
                t.is_active, t.is_visible, t.is_trial,
                (SELECT count(*) FROM subscriptions s WHERE s.tariff_id = t.id AND s.is_current) AS active_subs,
                (SELECT count(*) FROM payments p WHERE p.tariff_id=t.id AND p.status='success' AND p.paid_at >= now()-interval '30 days') AS purchases_30d
           FROM tariffs t ORDER BY t.sort_order, t.id",
    )
    .fetch_all(&st.pool)
    .await?;

    let mut out = Vec::new();
    for r in &rows {
        let id: i64 = r.get("id");
        let prices = sqlx::query(
            "SELECT period_days, currency, amount_minor FROM tariff_prices
              WHERE tariff_id = $1 AND is_active ORDER BY period_days",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await?;
        let squads: Vec<String> = sqlx::query_scalar(
            "SELECT s.name FROM squads s JOIN tariff_squads ts ON ts.squad_id = s.id
              WHERE ts.tariff_id = $1",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await?;

        let addons:Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(a) FROM tariff_addons a WHERE tariff_id=$1 AND is_active ORDER BY sort_order,id").bind(id).fetch_all(&st.pool).await?;
        out.push(json!({
            "id": id, "addons": addons,
            "code": r.get::<String, _>("code"),
            "locales": r.get::<Value,_>("locales"),
            "title": r.get::<String, _>("title"),
            "description": r.get::<Option<String>, _>("description"),
            "badge": r.get::<Option<String>, _>("badge"),
            "device_limit": r.get::<i32, _>("device_limit"),
            "traffic_limit_bytes": r.get::<Option<i64>, _>("traffic_limit_bytes"),
            "reset_strategy": r.get::<String, _>("reset_strategy"),
            "is_active": r.get::<bool, _>("is_active"),
            "is_visible": r.get::<bool, _>("is_visible"),
            "is_trial": r.get::<bool, _>("is_trial"),
            "active_subs": r.get::<i64, _>("active_subs"),
            "purchases_30d": r.get::<i64, _>("purchases_30d"),
            "squads": squads,
            "prices": prices.iter().map(|p| json!({
                "period_days": p.get::<i32, _>("period_days"),
                "currency": p.get::<String, _>("currency"),
                "amount_minor": p.get::<i64, _>("amount_minor"),
            })).collect::<Vec<_>>(),
        }));
    }
    Ok(Json(json!(out)))
}

// ── платежи ──

fn payment_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id":            r.get::<i64, _>("id"),
        "client_id":     r.try_get::<Option<i64>, _>("client_id").ok().flatten(),
        "status":        r.get::<String, _>("status"),
        "kind":          r.get::<String, _>("kind"),
        "amount_minor":  r.get::<i64, _>("amount_minor"),
        "currency":      r.get::<String, _>("currency"),
        "refunded_minor": r.get::<i64, _>("refunded_minor"),
        "provider":      r.get::<String, _>("provider"),
        "provider_txid": r.get::<Option<String>, _>("provider_txid"),
        "period_days":   r.try_get::<Option<i32>, _>("period_days").ok().flatten(),
        "error_message": r.get::<Option<String>, _>("error_message"),
        "tariff_code":   r.try_get::<Option<String>, _>("tariff_code").ok().flatten(),
        "username":      r.try_get::<Option<String>, _>("username").ok().flatten(),
        "paid_at":       dt(r, "paid_at"),
        "created_at":    dt(r, "created_at"),
    })
}

#[derive(Deserialize)]
struct PaymentsQuery {
    offset: Option<i64>,
    status: Option<String>,
    limit: Option<i64>,
    provider: Option<String>,
    search: Option<String>,
    paginated: Option<bool>,
}

async fn payments_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Query(q): Query<PaymentsQuery>,
) -> Result<Json<Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let status = q.status.filter(|s| !s.trim().is_empty());
    let provider = q.provider.filter(|s| !s.trim().is_empty());
    let search = q.search.filter(|s| !s.trim().is_empty());
    let filter = "WHERE ($1::text IS NULL OR p.status::text = $1)
      AND ($2::text IS NULL OR p.provider = $2)
      AND ($3::text IS NULL OR c.username ILIKE '%' || ltrim($3, '@') || '%'
           OR p.provider_txid ILIKE '%' || $3 || '%' OR p.id::text = $3
           OR p.amount_minor::text = $3)";
    let rows = sqlx::query(&format!(
        "SELECT p.id, p.client_id, p.status::text AS status, p.kind::text AS kind, p.amount_minor, p.currency,
                p.provider, p.provider_txid, p.period_days, p.error_message, p.paid_at, p.created_at,
                COALESCE((p.metadata->>'refunded_minor')::bigint, 0) AS refunded_minor,
                COALESCE(p.addon_snapshot->>'title',t.code) AS tariff_code, c.username
           FROM payments p
           LEFT JOIN tariffs t ON t.id = p.tariff_id
           JOIN clients c ON c.id = p.client_id
          {filter}
          ORDER BY p.created_at DESC, p.id DESC LIMIT $4 OFFSET $5"
    ))
    .bind(&status)
    .bind(&provider)
    .bind(&search)
    .bind(limit)
    .bind(q.offset.unwrap_or(0).max(0))
    .fetch_all(&st.pool)
    .await?;
    if q.paginated.unwrap_or(false) {
        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM payments p JOIN clients c ON c.id = p.client_id {filter}"
        ))
        .bind(&status).bind(&provider).bind(&search).fetch_one(&st.pool).await?;
        let summary = sqlx::query(
            "SELECT currency,
              COALESCE(sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0)) FILTER (WHERE status = 'success' AND paid_at >= now() - interval '24 hours'), 0)::bigint AS day_amount,
              count(*) FILTER (WHERE status = 'success' AND paid_at >= now() - interval '24 hours') AS day_count,
              COALESCE(sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0)) FILTER (WHERE status = 'success'), 0)::bigint AS month_amount,
              count(*) FILTER (WHERE status = 'success') AS month_count,
              count(*) FILTER (WHERE status = 'failed' AND created_at >= now() - interval '24 hours') AS failed_count
             FROM payments WHERE COALESCE(paid_at, created_at) >= now() - interval '30 days' GROUP BY currency"
        ).fetch_all(&st.pool).await?;
        return Ok(Json(json!({
            "items": rows.iter().map(payment_json).collect::<Vec<_>>(), "total": total,
            "summary": summary.iter().map(|r| json!({
                "currency": r.get::<String, _>("currency"),
                "day_amount": r.get::<i64, _>("day_amount"), "day_count": r.get::<i64, _>("day_count"),
                "month_amount": r.get::<i64, _>("month_amount"), "month_count": r.get::<i64, _>("month_count"),
                "failed_count": r.get::<i64, _>("failed_count"),
            })).collect::<Vec<_>>()
        })));
    }
    Ok(Json(json!(rows.iter().map(payment_json).collect::<Vec<_>>())))
}

// ── инфраструктура ──

async fn nodes_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    // numeric приводим к float8: иначе тянуть BigDecimal ради множителя.
    let rows = sqlx::query(
        "SELECT n.id, n.name, n.country_code, n.address, n.api_port, n.status::text AS status,
                n.engine_version, n.agent_version, n.safe_engine_update, n.last_seen_at,
                n.engine_ok, n.engine_error,
                n.traffic_multiplier::float8 AS traffic_multiplier,
                n.count_traffic, n.notify, n.profile_id, p.name AS profile_name,
                n.cpu_model, n.cpu_cores, n.kernel, n.mem_total_bytes,
                n.mem_used_bytes, n.uptime_seconds, n.iface,
                n.rx_total_bytes, n.tx_total_bytes,
                n.infra_provider_id, n.monthly_cost_minor, n.bill_day,
                ip.name AS infra_provider_name, ip.logo_url AS infra_provider_logo,
                COALESCE((SELECT sum(upload_bytes + download_bytes)::bigint FROM traffic_usage tu
                           WHERE tu.node_id = n.id AND tu.day = (now() AT TIME ZONE 'UTC')::date), 0) AS today_bytes,
                -- Последний замер метрик: панель показывает текущее
                -- состояние, а не среднее за сутки.
                m.online_count, m.cpu_percent, m.ram_percent,
                m.uplink_bps, m.downlink_bps, m.la1, m.la5, m.la15,
                -- Ряд для искры в списке: последние сутки по часам.
                -- Одним запросом на все ноды, а не по запросу на каждую:
                -- на парке в полсотни серверов это полсотни походов в базу
                -- ради одной строки списка.
                COALESCE(s.spark, '{}') AS spark
           FROM nodes n
           LEFT JOIN config_profiles p ON p.id = n.profile_id
           LEFT JOIN infra_providers ip ON ip.id = n.infra_provider_id
           LEFT JOIN LATERAL (
               SELECT online_count, cpu_percent, ram_percent,
                      uplink_bps, downlink_bps, la1, la5, la15
                 FROM node_metrics
                WHERE node_id = n.id ORDER BY at DESC LIMIT 1
           ) m ON true
           LEFT JOIN LATERAL (
               -- Суммарная скорость по часам: она есть в метриках и не
               -- требует обхода партиций трафика.
               SELECT array_agg(v ORDER BY час) AS spark FROM (
                   SELECT date_trunc('hour', at) AS час,
                          avg(COALESCE(uplink_bps,0) + COALESCE(downlink_bps,0))::bigint AS v
                     FROM node_metrics
                    WHERE node_id = n.id AND at > now() - interval '24 hours'
                    GROUP BY 1
               ) q
           ) s ON true
          WHERE n.deleted_at IS NULL ORDER BY n.name",
    )
    .fetch_all(&st.pool)
    .await?;

    let mut out = Vec::new();
    for r in &rows {
        let id: i64 = r.get("id");
        let inbounds: Vec<String> = sqlx::query_scalar(
            "SELECT i.tag FROM inbounds i JOIN node_inbounds ni ON ni.inbound_id = i.id
              WHERE ni.node_id = $1 ORDER BY i.tag",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await?;

        out.push(json!({
            "id": id,
            "name": r.get::<String, _>("name"),
            "country_code": r.get::<String, _>("country_code"),
            "address": r.get::<String, _>("address"),
            "api_port": r.get::<i32, _>("api_port"),
            "status": r.get::<String, _>("status"),
            // Движок отдельно от агента: нода отвечает панели, но может
            // не обслуживать ни одного клиента. В списке это авария.
            "engine_ok": r.try_get::<bool, _>("engine_ok").unwrap_or(true),
            "engine_error": r.try_get::<Option<String>, _>("engine_error").ok().flatten(),
            "engine_version": r.get::<Option<String>, _>("engine_version"),
            "agent_version": r.get::<Option<String>, _>("agent_version"),
            "safe_engine_update": r.get::<bool, _>("safe_engine_update"),
            "traffic_multiplier": r.get::<f64, _>("traffic_multiplier"),
            "count_traffic": r.get::<bool, _>("count_traffic"),
            "notify": r.get::<bool, _>("notify"),
            "profile_id": r.get::<Option<i64>, _>("profile_id"),
            "profile_name": r.get::<Option<String>, _>("profile_name"),
            "today_bytes": r.get::<i64, _>("today_bytes"),
            "online_count": r.try_get::<Option<i32>, _>("online_count").ok().flatten().unwrap_or(0),
            "cpu_percent":   r.try_get::<Option<f32>, _>("cpu_percent").ok().flatten(),
            "ram_percent":   r.try_get::<Option<f32>, _>("ram_percent").ok().flatten(),
            "uplink_bps":    r.try_get::<Option<i64>, _>("uplink_bps").ok().flatten(),
            "downlink_bps":  r.try_get::<Option<i64>, _>("downlink_bps").ok().flatten(),
            "la": [
                r.try_get::<Option<f32>, _>("la1").ok().flatten(),
                r.try_get::<Option<f32>, _>("la5").ok().flatten(),
                r.try_get::<Option<f32>, _>("la15").ok().flatten(),
            ],
            // Ряд для искры. Пусто у ноды, которая ещё не отчиталась.
            "spark":           r.try_get::<Vec<i64>, _>("spark").unwrap_or_default(),
            "cpu_model":       r.try_get::<Option<String>, _>("cpu_model").ok().flatten(),
            "cpu_cores":       r.try_get::<Option<i32>, _>("cpu_cores").ok().flatten(),
            "kernel":          r.try_get::<Option<String>, _>("kernel").ok().flatten(),
            "mem_total_bytes": r.try_get::<Option<i64>, _>("mem_total_bytes").ok().flatten(),
            "mem_used_bytes":  r.try_get::<Option<i64>, _>("mem_used_bytes").ok().flatten(),
            "uptime_seconds":  r.try_get::<Option<i64>, _>("uptime_seconds").ok().flatten(),
            "iface":           r.try_get::<Option<String>, _>("iface").ok().flatten(),
            "rx_total_bytes":  r.try_get::<Option<i64>, _>("rx_total_bytes").ok().flatten(),
            "tx_total_bytes":  r.try_get::<Option<i64>, _>("tx_total_bytes").ok().flatten(),
            "infra_provider_id":   r.try_get::<Option<i64>, _>("infra_provider_id").ok().flatten(),
            "infra_provider_name": r.try_get::<Option<String>, _>("infra_provider_name").ok().flatten(),
            "infra_provider_logo": r.try_get::<Option<String>, _>("infra_provider_logo").ok().flatten(),
            "monthly_cost_minor":  r.try_get::<Option<i64>, _>("monthly_cost_minor").ok().flatten().unwrap_or(0),
            "bill_day":            r.try_get::<Option<i32>, _>("bill_day").ok().flatten(),
            "last_seen_at": dt(r, "last_seen_at"),
            "inbounds": inbounds,
        }));
    }
    Ok(Json(json!(out)))
}

async fn hosts_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        // public_key и short_id нужны редактору: без них правка хоста
        // затирала бы ключи reality, и локация переставала подключаться.
        "SELECT h.id, h.sort_order, h.remark, h.address, h.port, h.security::text AS security,
                h.sni, h.fingerprint, h.alpn, h.path, h.is_enabled,
                h.public_key, h.short_id, h.options, h.host_header,
                i.profile_id, i.tag AS inbound_tag, i.protocol, i.network, p.name AS profile_name
           FROM hosts h
           -- LEFT, а не JOIN: хост без инбаунда — не ошибка, а состояние
           -- «привязка слетела при замене конфига». С внутренним
           -- соединением он пропадал из списка совсем, и привязать его
           -- заново было негде.
           LEFT JOIN inbounds i ON i.id = h.inbound_id
           LEFT JOIN config_profiles p ON p.id = i.profile_id
          ORDER BY h.sort_order, h.id",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "options":r.get::<Value,_>("options"),
            "host_header":r.get::<Option<String>,_>("host_header"),
            "profile_id":r.get::<Option<i64>,_>("profile_id"),
            "sort_order": r.get::<i32, _>("sort_order"),
            "remark": r.get::<String, _>("remark"),
            "address": r.get::<String, _>("address"),
            "port": r.get::<i32, _>("port"),
            "security": r.get::<String, _>("security"),
            "sni": r.get::<Option<String>, _>("sni"),
            "fingerprint": r.get::<Option<String>, _>("fingerprint"),
            "alpn": r.get::<Option<String>, _>("alpn"),
            "path": r.get::<Option<String>, _>("path"),
            "public_key": r.get::<Option<String>, _>("public_key"),
            "short_id": r.get::<Option<String>, _>("short_id"),
            "is_enabled": r.get::<bool, _>("is_enabled"),
            // Все три могут быть пусты: у отвязанного хоста инбаунда нет.
            // Читать их как String — уронить обработчик на первом же
            // таком хосте, причём весь список сразу.
            "inbound_tag": r.get::<Option<String>, _>("inbound_tag"),
            "protocol": r.get::<Option<String>, _>("protocol"),
            "network": r.get::<Option<String>, _>("network"),
            "profile_name": r.get::<Option<String>, _>("profile_name"),
        }))
        .collect::<Vec<_>>())))
}

async fn squads_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT s.id, s.name, s.description,
                (SELECT count(*) FROM client_squads cs WHERE cs.squad_id = s.id) AS members
           FROM squads s ORDER BY s.name",
    )
    .fetch_all(&st.pool)
    .await?;

    let mut out = Vec::new();
    for r in &rows {
        let id: i64 = r.get("id");
        let inbounds: Vec<String> = sqlx::query_scalar(
            "SELECT i.tag FROM inbounds i JOIN squad_inbounds si ON si.inbound_id = i.id
              WHERE si.squad_id = $1 ORDER BY i.tag",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await?;
        let refs: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('profile_id',i.profile_id,'tag',i.tag)), '[]'::jsonb)
            FROM inbounds i JOIN squad_inbounds si ON si.inbound_id=i.id WHERE si.squad_id=$1")
            .bind(id).fetch_one(&st.pool).await?;
        out.push(json!({
            "id": id,
            "name": r.get::<String, _>("name"),
            "description": r.get::<Option<String>, _>("description"),
            "members": r.get::<i64, _>("members"),
            "inbounds": inbounds,
            "inbound_refs": refs,
        }));
    }
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
pub(crate) struct ProfileCreate {
    pub name: String,
    pub config: Option<Value>,
}

/// Новый профиль конфигурации.
///
/// Проверяем конфиг до сохранения: невалидный заберут агенты и уронят
/// все ноды профиля разом, а причина будет видна только в их логах.
pub(crate) async fn profile_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<ProfileCreate>,
) -> Result<Json<Value>> {
    let name = b.name.trim();
    if name.is_empty() || name.chars().count()>160 {
        return Err(Error::bad("Profile name: 1–160 characters / Название профиля: 1–160 символов"));
    }
    if let Some(c)=&b.config{sn_core::profile_workflow::validate_shape(c).map_err(Error::bad)?;}

    // Секреты из заготовки заполняем сами. Ключи Reality, короткий
    // идентификатор, пароль Shadowsocks и путь веб-сокета — случайные
    // значения, спрашивать их не за чем. Тем более что генератор ключей
    // живёт на странице профиля, которая появляется только после
    // создания: заготовку с Reality нельзя было создать вообще.
    // Порядок важен: сначала секреты, потом служебная часть. Иначе
    // подстановка прошлась бы и по только что дописанному служебному
    // входу — там подставлять нечего, но лишний проход по чужому блоку
    // ни к чему.
    let подставлено = b.config.as_ref().map(|c| {
        let с_ключами = sn_core::profile_workflow::generate_missing(c);
        sn_core::service_parts::ensure_service_parts(&с_ключами).0
    });

    // Конфиг проверяем и при создании, а не только при сохранении.
    // Без этого профиль из заготовки с незаполненными полями попадал на
    // ноду как есть, движок его отвергал и не поднимался вовсе —
    // вместе со всеми остальными инбаундами.
    if let Some(cfg) = подставлено.as_ref() {
        let engine = std::env::var("ENGINE_BIN").unwrap_or_else(|_| "xray".into());
        let check = sn_core::xray_check::check_full(cfg, &engine);
        if !check.valid {
            return Err(Error::BadRequest(check.errors.join("; ")));
        }
    }
    let b = ProfileCreate { name: b.name.clone(), config: подставлено };

    let config = b.config.unwrap_or_else(|| {
        // Пустой профиль без inbounds бесполезен, но и подсовывать
        // втихую чужие ключи нельзя — даём каркас с пустым списком.
        json!({
            "log": { "loglevel": "warning" },
            "api": { "tag": "api", "services": ["StatsService"] },
            "stats": {},
            "policy": {
                "levels": { "0": { "statsUserUplink": true, "statsUserDownlink": true } },
                "system": { "statsInboundUplink": true, "statsInboundDownlink": true }
            },
            "inbounds": [{
                "tag": "api-in", "listen": "127.0.0.1", "port": 10085,
                "protocol": "dokodemo-door", "settings": { "address": "127.0.0.1" }
            }],
            "outbounds": [
                { "protocol": "freedom", "tag": "direct" },
                { "protocol": "blackhole", "tag": "block" }
            ],
            "routing": { "rules": [
                { "type": "field", "inboundTag": ["api-in"], "outboundTag": "api" }
            ]}
        })
    });

    // Та же проверка, что при сохранении профиля: невалидный конфиг
    // заберут агенты и уронят все ноды профиля разом.
    let check = sn_core::xray_check::check_structure(&config);
    if !check.valid {
        return Err(Error::bad(format!(
            "конфиг не прошёл проверку: {}",
            check.errors.join("; ")
        )));
    }

    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO config_profiles (name, config) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(&config)
    .fetch_one(&mut *tx)
    .await?;

    // Инбаунды профиля — то, из чего собираются хосты и сквады.
    // Без них профиль не появится нигде, кроме собственного списка.
    if let Some(inbounds) = config["inbounds"].as_array() {
        for i in inbounds {
            let tag = i["tag"].as_str().unwrap_or_default();
            let protocol = i["protocol"].as_str().unwrap_or_default();
            // Правило то же, что при сохранении: служебные инбаунды в
            // панель не попадают. Держим его в одном месте, чтобы
            // создание и правка не расходились.
            let listen = i["listen"].as_str().unwrap_or("");
            let loopback = listen.starts_with("127.") || listen == "::1" || listen == "localhost";
            let service = protocol == "dokodemo-door"
                || ((protocol == "socks" || protocol == "http") && loopback);
            if tag.is_empty() || service {
                continue;
            }
            sqlx::query(
                "INSERT INTO inbounds (profile_id, tag, protocol, network, security, port,
                                       method, service_name, server_key,
                                       public_key, short_id, sni)
                 VALUES ($1, $2, $3, $4, COALESCE($5,'none')::host_security, $6, $7, $8, $9,
                         $10, $11, $12)
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(tag)
            .bind(protocol)
            // Xray переименовал транспорт `tcp` в `raw`, оба имени приняты
            // движком. Внутри держим одно: сборка ссылок ветвится на «tcp»
            // — от него зависит и `type=` в ссылке, и `flow` у Reality.
            // Записать в базу «raw» значило бы молча выдать клиентам
            // ссылки без flow и с транспортом, которого старые приложения
            // не знают.
            .bind(
                i["streamSettings"]["network"]
                    .as_str()
                    .map(sn_core::xray_check::normalize_network),
            )
            .bind(i["streamSettings"]["security"].as_str())
            .bind(i["port"].as_i64().unwrap_or(443) as i32)
            .bind(i["settings"]["method"].as_str())
            .bind(i["streamSettings"]["grpcSettings"]["serviceName"].as_str())
            .bind(i["settings"]["password"].as_str())
            // Параметры Reality выводим из профиля: клиент обязан назвать
            // ровно те, что стоят на сервере, а копия в строке хоста со
            // временем расходится с профилем — и локация перестаёт работать.
            .bind(
                i["streamSettings"]["realitySettings"]["privateKey"]
                    .as_str()
                    .and_then(sn_core::keygen::reality_public_from_private),
            )
            .bind(i["streamSettings"]["realitySettings"]["shortIds"][0].as_str())
            .bind(i["streamSettings"]["realitySettings"]["serverNames"][0].as_str())
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn profile_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    sqlx::query("SELECT id FROM config_profiles WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let testing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profile_trials WHERE (profile_id=$1 OR candidate_id=$1) AND state='testing')")
        .bind(id).fetch_one(&mut *tx).await?;
    if testing { return Err(Error::bad("Finish the profile trial before deleting / Завершите проверку профиля перед удалением")); }
    let nodes: i64 = sqlx::query_scalar("SELECT count(*) FROM nodes WHERE profile_id=$1 AND deleted_at IS NULL")
        .bind(id).fetch_one(&mut *tx).await?;
    if nodes > 0 { return Err(Error::bad(format!("Profile is used by {nodes} nodes / Профиль используется на {nodes} нодах"))); }
    // Completed candidates can be deleted after their trial has been released.
    sqlx::query("DELETE FROM profile_trials WHERE candidate_id=$1 AND state='finished'")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM config_profiles WHERE id=$1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn profiles_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.engine::text AS engine, p.config, p.version, p.updated_at,
                (SELECT count(*) FROM nodes n WHERE n.profile_id = p.id AND n.deleted_at IS NULL) AS node_count
           FROM config_profiles p ORDER BY p.name",
    )
    .fetch_all(&st.pool)
    .await?;

    let mut out = Vec::new();
    for r in &rows {
        let id: i64 = r.get("id");
        let inbounds: Vec<String> =
            sqlx::query_scalar("SELECT tag FROM inbounds WHERE profile_id = $1 ORDER BY tag")
                .bind(id)
                .fetch_all(&st.pool)
                .await?;
        out.push(json!({
            "id": id,
            "name": r.get::<String, _>("name"),
            "engine": r.get::<String, _>("engine"),
            "config": r.get::<Value, _>("config"),
            "version": r.get::<i32, _>("version"),
            "node_count": r.get::<i64, _>("node_count"),
            "updated_at": dt(r, "updated_at"),
            "inbounds": inbounds,
        }));
    }
    Ok(Json(json!(out)))
}

/// Генерация ключей Reality для нового профиля.
///
/// Приватный ключ отдаём один раз — он должен попасть в конфиг профиля
/// и больше нигде не всплывать.
async fn keygen_reality(_a: CurrentAdmin) -> Result<Json<Value>> {
    Ok(Json(json!(sn_core::keygen::generate_reality_keys())))
}

#[derive(Deserialize)]
struct ConfigBody {
    /// Конфиг. Пусто — значит меняем только название.
    config: Option<Value>,
    /// Новое имя профиля. Раньше переименование в панели показывало
    /// успех, но запроса не отправляло — имя возвращалось прежним.
    name: Option<String>,
    expected_version: Option<i32>,
    #[serde(default)]
    acknowledge_impact: bool,
}

/// Проверка конфига до сохранения.
///
/// Битый конфиг уронит все ноды профиля разом: агент заберёт его и
/// перезапустит движок, который не поднимется.
async fn profile_validate(
    _a: CurrentAdmin,
    Json(b): Json<ConfigBody>,
) -> Result<Json<Value>> {
    let engine = std::env::var("ENGINE_BIN").unwrap_or_else(|_| "xray".into());
    // Проверяем то, что будет сохранено, а не то, что прислали: иначе
    // «Проверить» ругалось бы на отсутствие служебной части, которую
    // сохранение всё равно допишет само.
    let исходный = b.config.clone().unwrap_or_else(|| json!({}));
    sn_core::profile_workflow::validate_shape(&исходный).map_err(Error::bad)?;
    let (config, дописано) = sn_core::service_parts::ensure_service_parts(&исходный);
    let mut ответ = json!(sn_core::xray_check::check_full(&config, &engine));
    ответ["added"] = json!(дописано);
    Ok(Json(ответ))
}

/// Сохранение конфига профиля.
///
/// Инбаунды пересобираем из конфига: они — производное от него, и хранить
/// их отдельно вручную значит однажды разойтись с реальностью.
async fn profile_save(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ConfigBody>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    let previous=sqlx::query("SELECT version,config FROM config_profiles WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    if b.expected_version.is_some_and(|v|v!=previous.get::<i32,_>("version")) {
        return Err(Error::bad("Profile changed; reopen the editor / Профиль изменён, откройте редактор заново"));
    }
    if b.config.is_some() {
        sqlx::query("INSERT INTO profile_revisions(profile_id,version,config) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(id).bind(previous.get::<i32,_>("version")).bind(previous.get::<Value,_>("config")).execute(&mut *tx).await?;
    }
    if b.name.as_ref().is_some_and(|name| name.trim().is_empty()) {
        return Err(Error::bad("название профиля не может быть пустым"));
    }

    // Переименование без правки конфига: отдельный, самый частый случай.
    if let Some(name) = b.name.as_ref().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        sqlx::query("UPDATE config_profiles SET name = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }

    let Some(исходный) = b.config.as_ref() else {
        tx.commit().await?;
        return Ok(Json(json!({ "ok": true })));
    };

    sn_core::profile_workflow::validate_shape(исходный).map_err(Error::bad)?;
    // Служебную часть дописываем и здесь, а не только при создании:
    // конфиг чаще всего именно вставляют в редактор целиком — из другой
    // панели, из статьи, — и в нём этих блоков нет. Без них профиль
    // работает, но не считает трафик, и заметно это не сразу.
    let (config, дописано) = sn_core::service_parts::ensure_service_parts(исходный);
    let config = &config;

    let engine = std::env::var("ENGINE_BIN").unwrap_or_else(|_| "xray".into());
    let check = sn_core::xray_check::check_full(config, &engine);
    if !check.valid {
        return Err(Error::BadRequest(check.errors.join("; ")));
    }

    // Версию поднимаем — по ней ноды понимают, что пора забрать новый конфиг.
    let version: i32 = sqlx::query_scalar(
        "UPDATE config_profiles SET config = $2, version = version + 1, updated_at = now()
          WHERE id = $1 RETURNING version",
    )
    .bind(id)
    .bind(config)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;

    // Служебные инбаунды в панель не попадают: `api-in` слушает петлю и
    // существует ради статистики. Раньше он оседал в таблице и лез во
    // все списки выбора — его предлагали привязать к хосту и скваду,
    // хотя подключиться к нему нельзя.
    let client_inbounds: Vec<_> = check.inbounds.iter().filter(|i| !i.is_service).collect();

    let tags: Vec<String> = client_inbounds.iter().map(|i| i.tag.clone()).collect();

    let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inbounds i WHERE i.profile_id=$1 AND i.tag<>ALL($2) AND (EXISTS(SELECT 1 FROM hosts h WHERE h.inbound_id=i.id) OR EXISTS(SELECT 1 FROM squad_inbounds si WHERE si.inbound_id=i.id) OR EXISTS(SELECT 1 FROM node_inbounds ni WHERE ni.inbound_id=i.id)))")
        .bind(id).bind(&tags).fetch_one(&mut *tx).await?;
    if used && (!b.acknowledge_impact || b.expected_version.is_none()) {
        return Err(Error::bad("Review affected hosts, squads and nodes before removing inbounds / Перед удалением инбаундов подтвердите последствия для хостов, сквадов и нод"));
    }

    // Инбаунд, исчезнувший из конфига, но привязанный к хосту или скваду,
    // просто так не удалить — на него ссылаются, и база отвечала отказом
    // с «внутренней ошибкой». Держать человека из-за этого неправильно:
    // конфиг он уже написал, и переписывать его под старые теги — не то,
    // чего он хотел. Поэтому связи снимаем, а не запрещаем сохранение.
    //
    // Хост при этом остаётся: адрес, порт, название и настройки TLS
    // набирать заново не нужно, он лишь ждёт новой привязки. Выключаем
    // его, чтобы отвязанный хост не выглядел рабочим.
    let отвязано = sqlx::query(
        "WITH пропали AS (
             SELECT id, tag FROM inbounds WHERE profile_id = $1 AND tag <> ALL($2)
         ), сняли_хосты AS (
             UPDATE hosts SET inbound_id = NULL, is_enabled = false, updated_at = now()
              WHERE inbound_id IN (SELECT id FROM пропали)
             RETURNING remark
         ), сняли_сквады AS (
             DELETE FROM squad_inbounds WHERE inbound_id IN (SELECT id FROM пропали)
             RETURNING inbound_id
         ), сняли_ноды AS (
             DELETE FROM node_inbounds WHERE inbound_id IN (SELECT id FROM пропали)
             RETURNING inbound_id
         )
         SELECT (SELECT COALESCE(array_agg(remark), '{}') FROM сняли_хосты) AS хосты,
                (SELECT count(*) FROM сняли_сквады)                         AS сквадов,
                (SELECT COALESCE(array_agg(tag), '{}') FROM пропали)        AS теги",
    )
    .bind(id)
    .bind(&tags)
    .fetch_one(&mut *tx)
    .await?;

    let отвязанные_хосты: Vec<String> = отвязано.get("хосты");
    let снято_сквадов: i64 = отвязано.get("сквадов");
    let пропавшие_теги: Vec<String> = отвязано.get("теги");

    sqlx::query("DELETE FROM inbounds WHERE profile_id = $1 AND tag <> ALL($2)")
        .bind(id)
        .bind(&tags)
        .execute(&mut *tx)
        .await?;

    for ib in &client_inbounds {
        sqlx::query(
            "INSERT INTO inbounds (profile_id, tag, protocol, network, security, port,
                                   method, service_name, server_key, public_key, short_id, sni)
             VALUES ($1, $2, $3, $4, $5::host_security, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (profile_id, tag) DO UPDATE
                SET protocol = EXCLUDED.protocol, network = EXCLUDED.network,
                    security = EXCLUDED.security, port = EXCLUDED.port,
                    method = EXCLUDED.method, service_name = EXCLUDED.service_name,
                    server_key = EXCLUDED.server_key,
                    -- Перевыпустили ключи Reality — клиенты должны получить
                    -- новые, иначе локация тихо перестанет подключаться.
                    public_key = EXCLUDED.public_key, short_id = EXCLUDED.short_id,
                    sni = EXCLUDED.sni",
        )
        .bind(id)
        .bind(&ib.tag)
        .bind(&ib.protocol)
        .bind(&ib.network)
        .bind(&ib.security)
        .bind(ib.port as i32)
        .bind(&ib.method)
        .bind(&ib.service_name)
        .bind(&ib.server_key)
        .bind(&ib.public_key)
        .bind(&ib.short_id)
        .bind(&ib.sni)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id, payload)
         VALUES ('admin', $1, 'profile.save', 'config_profile', $2, $3)",
    )
    .bind(admin.id)
    .bind(id)
    .bind(json!({ "version": version, "inbounds": tags }))
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO profile_revisions(profile_id,version,config) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(id).bind(version).bind(config).execute(&mut *tx).await?;
    tx.commit().await?;

    Ok(Json(json!({
        "ok": true,
        "version": version,
        "inbounds": tags,
        "warnings": check.warnings,
        "engine_checked": check.engine_checked,
        // Что дописали за человека. Молча менять сохранённое нельзя:
        // он откроет конфиг заново и увидит блоки, которых не писал.
        "added": дописано,
        // Что отвязалось. Само по себе сохранение прошло, но хосты
        // выключены и ждут привязки — без этой строки человек узнает
        // об этом, только когда клиенты перестанут получать локацию.
        "unlinked": {
            "tags": пропавшие_теги,
            "hosts": отвязанные_хосты,
            "squad_links": снято_сквадов,
        },
    })))
}

// ── настройки ──

async fn settings_get(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(&st.pool)
        .await?;
    let mut map = serde_json::Map::new();
    for r in &rows {
        map.insert(r.get::<String, _>("key"), r.get::<Value, _>("value"));
    }
    Ok(Json(Value::Object(map)))
}

async fn settings_set(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(body): Json<serde_json::Map<String, Value>>,
) -> Result<Json<Value>> {
    sn_sub::policy::validate_settings(&body).map_err(Error::bad)?;
    sn_core::bot_config::validate(&body)?;
    if let Some(v)=body.get("cabinet.config"){if admin.role!="owner"{return Err(Error::Forbidden);}crate::cabinet::validate_config(v)?;}
    if let Some(value)=body.get("nodes.engine_version") {
        let tag=value.as_str().ok_or_else(||Error::bad("Версия Xray должна быть строкой"))?;
        if !tag.is_empty()&&!crate::xray_releases::valid_tag(tag){return Err(Error::bad("Укажите тег версии Xray вида v26.9.9 или оставьте поле пустым"));}
    }
    if body.keys().any(|k| k=="bot.admin_alerts_enabled" || k.starts_with("bot.alert_")) {
        let mut alerts=sn_core::bot_config::load(&st.pool).await?;
        alerts.extend(body.clone());
        if sn_core::bot_config::flag(&alerts,"bot.admin_alerts_enabled",false) {sn_core::alerts::destination(&alerts)?;}
    }
    let mut tx=st.pool.begin().await?;
    for (key, value) in body {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_by, updated_at)
             VALUES ($1, $2, $3, now())
             ON CONFLICT (key) DO UPDATE
                SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()",
        )
        .bind(&key)
        .bind(&value)
        .bind(admin.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RefundBody {
    /// Сколько вернули. Пусто — вся сумма платежа.
    amount_minor: Option<i64>,
    reason: Option<String>,
    /// Отозвать подписку, выданную этим платежом.
    revoke: Option<bool>,
}

/// Возврат платежа.
///
/// Деньги через провайдера возвращает человек в его личном кабинете:
/// у разных провайдеров возврат делается по-разному, а тихая попытка
/// «вернуть автоматически» в случае неудачи оставила бы платёж помеченным
/// возвращённым при живых деньгах. Панель фиксирует факт возврата,
/// отзывает выданное и пишет это в журнал действий.
async fn payment_refund(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<RefundBody>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;

    let row = sqlx::query(
        "SELECT client_id, amount_minor, currency, status::text AS status, subscription_id, kind::text AS kind,
                COALESCE((metadata->>'refunded_minor')::bigint, 0) AS refunded_minor
           FROM payments WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;

    let status: String = row.get("status");
    if status == "refunded" {
        return Err(Error::Conflict("платёж уже возвращён".into()));
    }
    if status != "success" {
        return Err(Error::bad("вернуть можно только успешный платёж"));
    }

    let paid: i64 = row.get("amount_minor");
    if paid == 0 {
        return Err(Error::bad("платёж на нулевую сумму — возвращать нечего"));
    }
    let already: i64 = row.get("refunded_minor");
    let remaining = paid.saturating_sub(already).max(0);
    if remaining == 0 { return Err(Error::Conflict("платёж уже полностью возвращён".into())); }
    let amount = b.amount_minor.unwrap_or(remaining);
    if amount <= 0 || amount > remaining {
        return Err(Error::bad("сумма возврата превышает оставшуюся невозвращённую сумму"));
    }

    let client_id: i64 = row.get("client_id");
    let full = amount == remaining;

    // Частичный возврат оставляет платёж успешным: он и правда был, просто
    // часть денег вернули. Полный — переводит в «возвращён», иначе выручка
    // за месяц продолжала бы включать возвращённые деньги.
    sqlx::query(
        "UPDATE payments
            SET status = CASE WHEN $2 THEN 'refunded'::payment_status ELSE status END,
                metadata = metadata || jsonb_build_object(
                    'refunded_minor', COALESCE((metadata->>'refunded_minor')::bigint, 0) + $3,
                    'refund_reason', $4::text,
                    'refunded_at', now()),
                updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(full)
    .bind(amount)
    .bind(&b.reason)
    .execute(&mut *tx)
    .await?;

    sn_core::partners::adjust_refund(&mut tx, id).await?;

    // Отзыв подписки — отдельное решение: бывает, что деньги возвращают,
    // а доступ до конца оплаченного срока оставляют.
    if b.revoke.unwrap_or(false) && row.get::<String,_>("kind")=="addon" {
        sqlx::query("UPDATE subscription_addons SET expires_at=now(),ended_at=CASE WHEN activated_at IS NULL THEN now() ELSE ended_at END WHERE payment_id=$1 AND ended_at IS NULL").bind(id).execute(&mut *tx).await?;
        sn_core::addons::maintain(&mut tx,client_id,false).await?;
    } else if b.revoke.unwrap_or(false) {
        sqlx::query(
            "UPDATE subscriptions SET expires_at = now()
              WHERE client_id = $1 AND is_current",
        )
        .bind(client_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE clients SET status = 'expired' WHERE id = $1")
            .bind(client_id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id, payload)
         VALUES ('admin', $1, 'payment.refund', 'payment', $2, $3)",
    )
    .bind(admin.id)
    .bind(id)
    .bind(json!({ "amount_minor": amount, "full": full, "reason": b.reason,
                  "revoked": b.revoke.unwrap_or(false) }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "refunded_minor": amount, "full": full })))
}

#[derive(Deserialize, Default)]
struct BulkFilter {
    tariff: Option<String>,
    status: Option<String>,
    q: Option<String>,
}

#[derive(Deserialize, Default)]
struct BulkSet {
    tariff_id: Option<i64>,
    /// Продлить на N дней (отрицательное — отнять).
    add_days: Option<i32>,
    /// Установить конкретную дату окончания.
    expires_at: Option<String>,
    /// Лимит трафика в байтах. `Some(None)` здесь не выразить, поэтому
    /// снятие лимита передаётся отдельным флагом.
    traffic_limit_bytes: Option<i64>,
    unlimit_traffic: Option<bool>,
    device_limit: Option<i32>,
    reset_strategy: Option<String>,
    tag: Option<String>,
    squads: Option<Vec<i64>>,
}

#[derive(Deserialize)]
struct BulkBody {
    /// Явный список. Пусто — берём всех, кто подходит под фильтр списка.
    ids: Option<Vec<i64>>,
    #[serde(default)]
    filter: BulkFilter,
    /// enable | disable | reset_traffic | revoke | delete | update
    action: String,
    #[serde(default)]
    set: BulkSet,
}

/// Массовые операции над клиентами.
///
/// Работает и по выбранным строкам, и «по фильтру» — второе нужно, когда
/// клиентов тысячи и отмечать их галочками бессмысленно. Список
/// затрагиваемых id вычисляется на сервере тем же условием, что и выдача
/// списка: иначе «выбрать всех» на первой странице означало бы только
/// первые пятьдесят.
async fn clients_bulk(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<BulkBody>,
) -> Result<Json<Value>> {
    // Целевые id. Явный список ограничиваем разумным размером: миллион
    // идентификаторов в теле запроса — это уже не работа с панелью.
    let ids: Vec<i64> = match &b.ids {
        Some(v) if !v.is_empty() => {
            if v.len() > 10_000 {
                return Err(Error::bad("слишком много выбранных записей"));
            }
            v.clone()
        }
        Some(_) => return Err(Error::bad("не выбраны клиенты")),
        None => {
            let status = b.filter.status.clone().filter(|s| !s.trim().is_empty());
            let q = b.filter.q.clone().filter(|s| !s.trim().is_empty());
            sqlx::query_scalar(&format!("SELECT id FROM client_overview WHERE {CLIENT_FILTER}"))
                .bind(&status).bind(&q).bind(&b.filter.tariff)
                .fetch_all(&st.pool).await?
        }
    };

    if ids.is_empty() {
        return Err(Error::bad("под условие не попал ни один клиент"));
    }

    let mut tx = st.pool.begin().await?;
    let mut affected: u64 = 0;

    match b.action.as_str() {
        "enable" => {
            affected = sqlx::query(
                "UPDATE clients SET status = 'active' WHERE id = ANY($1) AND deleted_at IS NULL",
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        "disable" => {
            affected = sqlx::query(
                "UPDATE clients SET status = 'disabled' WHERE id = ANY($1) AND deleted_at IS NULL",
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        "reset_traffic" => {
            let mut ordered=ids.clone();ordered.sort_unstable();ordered.dedup();
            for id in ordered {sn_core::addons::maintain(&mut tx,id,true).await?;affected+=1;}
        }
        "revoke" => {
            // Новый short_id и новый vpn_uuid: старые ссылки и ключи умирают.
            affected = sqlx::query(
                "UPDATE clients
                    SET short_id = substr(replace(gen_random_uuid()::text, '-', ''), 1, 8),
                        vpn_uuid = gen_random_uuid()
                  WHERE id = ANY($1) AND deleted_at IS NULL",
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        "delete" => {
            affected = sqlx::query(
                "UPDATE clients SET deleted_at = now() WHERE id = ANY($1) AND deleted_at IS NULL",
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        "update" => {
            let s = &b.set;

            if let Some(tid) = s.tariff_id {
                let mut ordered=ids.clone();ordered.sort_unstable();ordered.dedup();
                for id in &ordered {sn_core::addons::maintain(&mut tx,*id,false).await?;}
                affected += sqlx::query(
                    "UPDATE subscriptions sub
                        SET tariff_id = t.id, device_limit = t.device_limit,
                            traffic_limit_bytes = t.traffic_limit_bytes,
                            reset_strategy = t.reset_strategy
                       FROM tariffs t
                      WHERE t.id = $2 AND sub.client_id = ANY($1) AND sub.is_current",
                )
                .bind(&ids)
                .bind(tid)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                for id in &ordered {sn_core::addons::restore_after_plan(&mut tx,*id).await?;}
            }

            if let Some(days) = s.add_days {
                affected += sqlx::query(
                    "UPDATE subscriptions
                        SET expires_at = GREATEST(COALESCE(expires_at, now()), now())
                                         + ($2 || ' days')::interval
                      WHERE client_id = ANY($1) AND is_current",
                )
                .bind(&ids)
                .bind(days.to_string())
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }

            if let Some(date) = &s.expires_at {
                affected += sqlx::query(
                    "UPDATE subscriptions SET expires_at = $2::timestamptz
                      WHERE client_id = ANY($1) AND is_current",
                )
                .bind(&ids)
                .bind(date)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }

            if s.unlimit_traffic.unwrap_or(false) {
                affected += sqlx::query(
                    "UPDATE subscriptions SET traffic_limit_bytes = NULL
                      WHERE client_id = ANY($1) AND is_current",
                )
                .bind(&ids)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            } else if let Some(bytes) = s.traffic_limit_bytes {
                affected += sqlx::query(
                    "UPDATE subscriptions SET traffic_limit_bytes = $2
                      WHERE client_id = ANY($1) AND is_current",
                )
                .bind(&ids)
                .bind(bytes)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }

            if let Some(dl) = s.device_limit {
                affected += sqlx::query(
                    "UPDATE subscriptions SET device_limit = $2
                      WHERE client_id = ANY($1) AND is_current",
                )
                .bind(&ids)
                .bind(dl)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }

            if let Some(rs) = &s.reset_strategy {
                affected += sqlx::query(
                    "UPDATE subscriptions SET reset_strategy = $2::reset_strategy
                      WHERE client_id = ANY($1) AND is_current",
                )
                .bind(&ids)
                .bind(rs)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }

            if let Some(tag) = &s.tag {
                affected += sqlx::query("UPDATE clients SET tag = $2 WHERE id = ANY($1)")
                    .bind(&ids)
                    .bind(tag)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
            }

            if let Some(squads) = &s.squads {
                // Замена, а не добавление: иначе «поставить сквад X» со
                // временем превращает набор локаций в свалку.
                sqlx::query("DELETE FROM client_squads WHERE client_id = ANY($1)")
                    .bind(&ids)
                    .execute(&mut *tx)
                    .await?;
                if !squads.is_empty() {
                    affected += sqlx::query(
                        "INSERT INTO client_squads (client_id, squad_id)
                         SELECT c, s FROM unnest($1::bigint[]) c CROSS JOIN unnest($2::bigint[]) s
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&ids)
                    .bind(squads)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                }
            }
        }
        _ => return Err(Error::bad("неизвестное действие")),
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, payload)
         VALUES ('admin', $1, $2, 'client', $3)",
    )
    .bind(admin.id)
    .bind(format!("clients.bulk.{}", b.action))
    .bind(json!({ "count": ids.len(), "affected": affected, "by_filter": b.ids.is_none() }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "clients": ids.len(), "affected": affected })))
}

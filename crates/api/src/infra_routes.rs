//! Расходы на инфраструктуру: провайдеры серверов, стоимость нод, оплаты.
//!
//! Выручка без расходов — не прибыль. Панель показывает и то, и другое
//! в одной валюте, чтобы маржа считалась вычитанием, а не по курсу.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn infra_routes() -> Router<AppState> {
    Router::new()
        .route("/api/infra/providers", get(providers_list).post(provider_create))
        .route(
            "/api/infra/providers/{id}",
            axum::routing::patch(provider_update).delete(provider_delete),
        )
        .route("/api/infra/payments", get(payments_list).post(payment_create))
        .route("/api/infra/payments/{id}", axum::routing::delete(payment_delete))
        .route("/api/infra/summary", get(summary))
        .route("/api/reports/torrents", get(torrents_list))
        .route("/api/reports/domains", get(domains_list))
}

#[derive(Deserialize)]
struct ReportQuery {
    client_id: Option<i64>,
    offset: Option<i64>,
    paginated: Option<bool>,
    days: Option<i32>,
    limit: Option<i64>,
}

/// Кто пытался качать торренты. Блокировка работает всегда — здесь
/// только отчёт о срабатываниях.
async fn torrents_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Query(q): Query<ReportQuery>,
) -> Result<Json<Value>> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let rows = sqlx::query(
        "SELECT c.id AS client_id, c.username, n.name AS node, tr.day, tr.hits, tr.last_target
           FROM torrent_reports tr
           JOIN clients c ON c.id = tr.client_id
           LEFT JOIN nodes n ON n.id = tr.node_id
          WHERE tr.day >= (now() AT TIME ZONE 'UTC')::date - $1::int
          AND ($3::bigint IS NULL OR c.id=$3)
          ORDER BY tr.day DESC, tr.hits DESC, tr.client_id, tr.node_id
          LIMIT $2 OFFSET $4",
    )
    .bind(days)
    .bind(q.limit.unwrap_or(200).clamp(1, 1000)).bind(q.client_id).bind(q.offset.unwrap_or(0).max(0))
    .fetch_all(&st.pool)
    .await?;

    let items = json!(rows
        .iter()
        .map(|r| json!({
            "client_id": r.get::<i64, _>("client_id"),
            "username": r.get::<String, _>("username"),
            "node": r.get::<Option<String>, _>("node"),
            "day": r.get::<chrono::NaiveDate, _>("day").to_string(),
            "hits": r.get::<i32, _>("hits"),
            "last_target": r.get::<Option<String>, _>("last_target"),
        }))
        .collect::<Vec<_>>());
    if !q.paginated.unwrap_or(false) { return Ok(Json(items)); }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM torrent_reports tr JOIN clients c ON c.id=tr.client_id WHERE tr.day >= (now() AT TIME ZONE 'UTC')::date - $1::int AND ($2::bigint IS NULL OR c.id=$2)")
        .bind(days).bind(q.client_id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"items":items,"total":total})))
}

/// Домены, куда ходят клиенты. Пусто, пока сбор не включён.
async fn domains_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Query(q): Query<ReportQuery>,
) -> Result<Json<Value>> {
    let enabled: bool = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'reports.http_enabled'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

    let days = q.days.unwrap_or(7).clamp(1, 90);
    let rows = sqlx::query(
        "SELECT domain, sum(hits)::bigint AS hits
           FROM http_domain_stats
          WHERE day >= (now() AT TIME ZONE 'UTC')::date - $1::int
          GROUP BY domain ORDER BY hits DESC LIMIT $2",
    )
    .bind(days)
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!({
        "enabled": enabled,
        "items": rows
            .iter()
            .map(|r| json!({
                "domain": r.get::<String, _>("domain"),
                "hits": r.get::<i64, _>("hits"),
            }))
            .collect::<Vec<_>>(),
    })))
}

async fn providers_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.url, p.note, p.logo_url,
                (SELECT count(*) FROM nodes n
                  WHERE n.infra_provider_id = p.id AND n.deleted_at IS NULL) AS node_count,
                COALESCE((SELECT sum(n.monthly_cost_minor)::bigint FROM nodes n
                           WHERE n.infra_provider_id = p.id AND n.deleted_at IS NULL), 0) AS monthly_minor
           FROM infra_providers p ORDER BY p.name",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "name": r.get::<String, _>("name"),
            "url": r.get::<Option<String>, _>("url"),
            "note": r.get::<Option<String>, _>("note"),
            "logo_url": r.get::<Option<String>, _>("logo_url"),
            "node_count": r.get::<i64, _>("node_count"),
            "monthly_minor": r.get::<i64, _>("monthly_minor"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct ProviderBody {
    name: Option<String>,
    url: Option<String>,
    note: Option<String>,
    logo_url: Option<String>,
}

async fn provider_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<ProviderBody>,
) -> Result<Json<Value>> {
    let name = b.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::bad("нужно название провайдера"));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO infra_providers (name, url, note, logo_url)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(name)
    .bind(&b.url)
    .bind(&b.note)
    .bind(&b.logo_url)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({ "id": id })))
}

async fn provider_update(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ProviderBody>,
) -> Result<Json<Value>> {
    let res = sqlx::query(
        "UPDATE infra_providers
            SET name = COALESCE($2, name), url = COALESCE($3, url),
                note = COALESCE($4, note), logo_url = COALESCE($5, logo_url)
          WHERE id = $1",
    )
    .bind(id)
    .bind(&b.name)
    .bind(&b.url)
    .bind(&b.note)
    .bind(&b.logo_url)
    .execute(&st.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn provider_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Ноды не трогаем: у них просто пропадёт привязка. Удалять сервер
    // из-за удаления строки о хостере — совсем не то, чего ждут.
    let res = sqlx::query("DELETE FROM infra_providers WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PaymentsQuery {
    limit: Option<i64>,
}

async fn payments_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Query(q): Query<PaymentsQuery>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT ip.id, ip.amount_minor, ip.currency, ip.paid_on,
                p.name AS provider_name, n.name AS node_name
           FROM infra_payments ip
           LEFT JOIN infra_providers p ON p.id = ip.provider_id
           LEFT JOIN nodes n           ON n.id = ip.node_id
          ORDER BY ip.paid_on DESC, ip.id DESC
          LIMIT $1",
    )
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "amount_minor": r.get::<i64, _>("amount_minor"),
            "currency": r.get::<String, _>("currency"),
            "paid_on": r.get::<chrono::NaiveDate, _>("paid_on").to_string(),
            "provider_name": r.get::<Option<String>, _>("provider_name"),
            "node_name": r.get::<Option<String>, _>("node_name"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct PaymentBody {
    provider_id: Option<i64>,
    node_id: Option<i64>,
    amount_minor: i64,
    paid_on: Option<chrono::NaiveDate>,
}

async fn payment_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<PaymentBody>,
) -> Result<Json<Value>> {
    if b.amount_minor <= 0 {
        return Err(Error::bad("сумма должна быть больше нуля"));
    }
    if b.provider_id.is_none() && b.node_id.is_none() {
        return Err(Error::bad("укажите провайдера или ноду — иначе расход не к чему отнести"));
    }

    let currency = crate::catalog_routes::system_currency(&st.pool).await;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO infra_payments (provider_id, node_id, amount_minor, currency, paid_on)
         VALUES ($1, $2, $3, $4, COALESCE($5, (now() AT TIME ZONE 'UTC')::date))
         RETURNING id",
    )
    .bind(b.provider_id)
    .bind(b.node_id)
    .bind(b.amount_minor)
    .bind(&currency)
    .bind(b.paid_on)
    .fetch_one(&st.pool)
    .await?;

    Ok(Json(json!({ "id": id })))
}

async fn payment_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM infra_payments WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

/// Сводка: расход, выручка и маржа за 30 дней.
///
/// Считаем в одной валюте — валюте системы. Расход берём по плановой
/// месячной стоимости нод: фактические оплаты приходят раз в месяц и
/// неровно, по ним нельзя сказать «сколько стоит парк сейчас».
async fn summary(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let currency = crate::catalog_routes::system_currency(&st.pool).await;

    let row = sqlx::query(
        "SELECT
            COALESCE((SELECT sum(monthly_cost_minor)::bigint FROM nodes
                       WHERE deleted_at IS NULL), 0) AS monthly_cost_minor,
            (SELECT count(*) FROM nodes WHERE deleted_at IS NULL) AS node_count,
            COALESCE((SELECT sum(GREATEST(amount_minor - COALESCE((metadata->>'refunded_minor')::bigint, 0), 0))::bigint FROM payments
                       WHERE status = 'success'
                         AND paid_at >= now() - interval '30 days'
                         AND currency = $1), 0) AS revenue_minor,
            COALESCE((SELECT sum(amount_minor)::bigint FROM infra_payments
                       WHERE paid_on >= (now() AT TIME ZONE 'UTC')::date - 30), 0) AS paid_minor",
    )
    .bind(&currency)
    .fetch_one(&st.pool)
    .await?;

    let cost: i64 = row.get("monthly_cost_minor");
    let revenue: i64 = row.get("revenue_minor");
    let nodes: i64 = row.get("node_count");

    Ok(Json(json!({
        "currency": currency,
        "monthly_cost_minor": cost,
        "revenue_minor": revenue,
        "paid_minor": row.get::<i64, _>("paid_minor"),
        "profit_minor": revenue - cost,
        // Маржа без выручки не определена: делить на ноль и показывать
        // «100%» при нулевом доходе — прямой обман.
        "margin_percent": if revenue > 0 {
            Some(((revenue - cost) as f64 / revenue as f64 * 100.0 * 10.0).round() / 10.0)
        } else {
            None
        },
        "node_count": nodes,
        "avg_node_minor": if nodes > 0 { cost / nodes } else { 0 },
    })))
}

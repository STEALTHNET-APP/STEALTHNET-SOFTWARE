//! Платежи: приём вебхуков и управление модулями из панели.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn payment_routes() -> Router<AppState> {
    Router::new()
        // Вебхук намеренно без авторизации: подпись проверяет сам модуль.
        .route("/api/pay/webhook/{provider}", post(webhook).layer(axum::extract::DefaultBodyLimit::max(256 * 1024)))
        .route("/api/pay/providers", get(providers_list))
        .route("/api/pay/providers/{id}", axum::routing::patch(provider_update))
        .route("/api/pay/{id}/confirm", post(confirm_manual))
}

/// Приём уведомления об оплате.
///
/// Успех подтверждаем только после применения: при сбое провайдер должен повторить доставку.
/// Всё, что пришло, сохраняем в журнал — разобраться можно потом.
async fn webhook(
    State(st): State<AppState>,
    Path(provider_id): Path<String>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    body: Bytes,
) -> Result<Json<Value>> {
    crate::security::webhook_attempt(&st, &headers, peer.ip()).await?;
    let hmap: HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.as_str().to_lowercase(), s.to_string())))
        .collect();

    let raw: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

    // Подпись вебхука проверяется тем же ключом, что задан в панели —
    // читаем актуальный, иначе после смены ключа уведомления отклонялись бы.
    st.payments.refresh(&st.pool).await;

    let Some(provider) = st.payments.get(&provider_id) else {
        log_webhook(&st, &provider_id, &raw, false, None, Some("нет такого модуля")).await;
        return Ok(Json(json!({ "ok": false, "error": "неизвестный модуль" })));
    };

    if !provider.accepts_http_webhooks() {
        return Err(Error::bad("этот модуль не принимает публичные HTTP-вебхуки"));
    }
    match provider.handle_webhook(&hmap, &body).await {
        Ok(outcome) => {
            // Дубликат события отсекаем до применения: провайдеры шлют повторы.
            if let Some(ext) = &outcome.external_event_id {
                let dup: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM payment_webhooks
                                    WHERE provider = $1 AND external_id = $2 AND processed)",
                )
                .bind(&provider_id)
                .bind(ext)
                .fetch_one(&st.pool)
                .await
                .unwrap_or(false);
                if dup {
                    tracing::info!(provider = provider_id, event = ext, "повторный вебхук пропущен");
                    return Ok(Json(json!({ "ok": true, "duplicate": true })));
                }
            }

            match st.payments.apply_outcome(&st.pool, &provider_id, &outcome).await {
                Ok(pid) => {
                    log_webhook(&st, &provider_id, &outcome.raw, true, pid, None).await;
                    tracing::info!(provider = provider_id, payment = ?pid, "оплата применена");
                    Ok(Json(json!({ "ok": true })))
                }
                Err(e) => {
                    log_webhook(&st, &provider_id, &outcome.raw, true, None, Some(&e.to_string())).await;
                    tracing::warn!(provider = provider_id, error = %e, "не удалось применить оплату");
                    Err(e)
                }
            }
        }
        Err(e) => {
            // Неверная подпись — самый важный случай для журнала:
            // это либо ошибка настройки, либо попытка подделать оплату.
            log_webhook(&st, &provider_id, &raw, false, None, Some(&e.to_string())).await;
            tracing::warn!(provider = provider_id, error = %e, "вебхук отклонён");
            Err(e)
        }
    }
}

async fn log_webhook(
    st: &AppState,
    provider: &str,
    payload: &Value,
    signature_ok: bool,
    payment_id: Option<i64>,
    error: Option<&str>,
) {
    let _ = sqlx::query(
        "INSERT INTO payment_webhooks (provider, payload, signature_ok, processed, payment_id, error)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(provider)
    .bind(payload)
    .bind(signature_ok)
    .bind(payment_id.is_some())
    .bind(payment_id)
    .bind(error)
    .execute(&st.pool)
    .await;
}

/// Список модулей для панели: что доступно, что настроено, какие валюты.
async fn providers_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    st.payments.refresh(&st.pool).await;

    let rows = sqlx::query(
        "SELECT id, title, currencies, enabled_currencies, note, config,
                is_enabled, is_configured, sort_order
           FROM payment_providers ORDER BY sort_order, id",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<String, _>("id"),
            "title": r.get::<String, _>("title"),
            "currencies": r.get::<Vec<String>, _>("currencies"),
            // null = модуль обслуживает все свои валюты
            "enabled_currencies": r.try_get::<Option<Vec<String>>, _>("enabled_currencies").ok().flatten(),
            "note": r.try_get::<Option<String>, _>("note").ok().flatten(),
            // Схема полей: по ней панель рисует форму настроек модуля.
            // Секреты обратно не отдаём — только признак «значение задано»,
            // иначе ключ платёжной системы утёк бы в браузер и в логи прокси.
            "fields": st.payments.get(&r.get::<String, _>("id"))
                .map(|p| p.settings_schema().iter().map(|f| {
                    let cfg = r.try_get::<Value, _>("config").ok().unwrap_or(Value::Null);
                    let stored = cfg.get(f.key).and_then(|v| v.as_str()).unwrap_or("");
                    json!({
                        "key": f.key, "label": f.label, "hint": f.hint,
                        "secret": f.secret, "required": f.required, "default": f.default,
                        "filled": !stored.trim().is_empty(),
                        "value": if f.secret { Value::Null } else { json!(stored) },
                    })
                }).collect::<Vec<_>>())
                .unwrap_or_default(),
            "is_enabled": r.get::<bool, _>("is_enabled"),
            "is_configured": st.payments.get(&r.get::<String, _>("id")).is_some_and(|p|p.is_configured()),
            "sort_order": r.get::<i32, _>("sort_order"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(serde::Deserialize)]
struct ProviderBody {
    title: Option<String>,
    /// Значения полей из формы. Пустая строка у секрета означает
    /// «не менять» — панель не знает старого значения и не должна его стирать.
    settings: Option<serde_json::Map<String, Value>>,
    is_enabled: Option<bool>,
    sort_order: Option<i32>,
    note: Option<String>,
    /// Какие из валют модуля предлагать. Пустой список = все.
    enabled_currencies: Option<Vec<String>>,
}

/// Настройки модуля, которые задаёт администратор.
///
/// Ключи провайдеров здесь не принимаем: они живут в окружении сервиса и
/// в базу не попадают. Панель показывает лишь, настроены они или нет.
async fn provider_update(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<ProviderBody>,
) -> Result<Json<Value>> {
    // Валюту, которой модуль не умеет, включить нельзя: кнопка в боте
    // вела бы в ошибку выставления счёта.
    let mut known: Vec<String> =
        sqlx::query_scalar("SELECT currencies FROM payment_providers WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await?
            .ok_or(Error::NotFound)?;

    // The generic adapter defines currencies in its settings. Validate the
    // selection against the submitted configuration, not the pre-save cache.
    if id == "http" {
        let current: Value = sqlx::query_scalar("SELECT config FROM payment_providers WHERE id=$1")
            .bind(&id).fetch_one(&st.pool).await?;
        let configured = b.settings.as_ref().and_then(|s| s.get("currencies"))
            .or_else(|| current.get("currencies"))
            .and_then(Value::as_str).unwrap_or("");
        known = configured.split(',').map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty()).collect();
    }

    let change_currencies=b.enabled_currencies.is_some();
    let enabled = match &b.enabled_currencies {
        // Пустой список означает «все» — так же, как NULL в базе. Иначе
        // администратор, сняв все галочки, получил бы модуль без валют,
        // который молча не появляется в боте.
        Some(list) if list.is_empty() => None,
        Some(list) => {
            if let Some(bad) = list.iter().find(|c| !known.contains(c)) {
                return Err(Error::bad(format!("модуль не принимает {bad}")));
            }
            Some(list.clone())
        }
        None => None,
    };

    // Настройки сливаем с сохранёнными: форма присылает только то, что
    // администратор трогал, а секреты приходят пустыми.
    if let Some(incoming) = b.settings {
        let current: Value = sqlx::query_scalar("SELECT config FROM payment_providers WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await?
            .flatten()
            .unwrap_or(Value::Null);

        let mut merged = current.as_object().cloned().unwrap_or_default();
        let secrets: Vec<&'static str> = st
            .payments
            .get(&id)
            .map(|p| {
                p.settings_schema()
                    .iter()
                    .filter(|f| f.secret)
                    .map(|f| f.key)
                    .collect()
            })
            .unwrap_or_default();

        for (k, v) in incoming {
            let val = v.as_str().unwrap_or("").trim().to_string();
            // Пустое значение секрета — «оставить как было». Стереть ключ
            // можно только явным словом: секрет не приходит в браузер, и
            // случайное сохранение формы иначе обнуляло бы платёжку.
            if val.is_empty() && secrets.contains(&k.as_str()) {
                continue;
            }
            merged.insert(k, json!(val));
        }

        if let Some(p)=st.payments.get(&id) {p.validate_settings(&merged)?;}

        sqlx::query("UPDATE payment_providers SET config = $2, updated_at = now() WHERE id = $1")
            .bind(&id)
            .bind(Value::Object(merged))
            .execute(&st.pool)
            .await?;

        // Сбрасываем кэш реестра, чтобы «настроен» пересчитался сразу.
        st.payments.invalidate();
        st.payments.refresh(&st.pool).await;

        // Признак настроенности пересчитывает сам модуль по своим правилам.
        if let Some(p) = st.payments.get(&id) {
            sqlx::query("UPDATE payment_providers SET is_configured = $2, currencies = $3 WHERE id = $1")
                .bind(&id)
                .bind(p.is_configured())
                .bind(p.currencies())
                .execute(&st.pool)
                .await?;
        }
    }

    let res = sqlx::query(
        "UPDATE payment_providers SET
            title = COALESCE($2, title),
            is_enabled = COALESCE($3, is_enabled),
            sort_order = COALESCE($4, sort_order),
            note = COALESCE($5, note),
            enabled_currencies = CASE WHEN $7 THEN $6 ELSE enabled_currencies END,
            updated_at = now()
          WHERE id = $1",
    )
    .bind(&id)
    .bind(&b.title)
    .bind(b.is_enabled)
    .bind(b.sort_order)
    .bind(&b.note)
    .bind(&enabled)
    .bind(change_currencies)
    .execute(&st.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, payload)
         VALUES ('admin', $1, 'payment_provider.update', 'payment_provider', $2)",
    )
    .bind(admin.id)
    .bind(json!({ "provider": id }))
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// Подтверждение оплаты вручную — для переводов мимо платёжных систем.
async fn confirm_manual(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(payment_id): Path<i64>,
) -> Result<Json<Value>> {
    let outcome = sn_payments::WebhookOutcome {
        external_event_id: None,
        payment_id: Some(payment_id),
        provider_txid: Some(format!("manual:{}:{payment_id}", admin.id)),
        status: sn_payments::PaymentStatus::Success,
        amount_minor: None,
        currency: None,
        error: None,
        raw: json!({ "confirmed_by": admin.username }),
    };
    st.payments.apply_outcome(&st.pool, "manual", &outcome).await?;

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'payment.confirm_manual', 'payment', $2)",
    )
    .bind(admin.id)
    .bind(payment_id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

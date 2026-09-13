//! API для сервиса подписок.
//!
//! Нужен, чтобы `sn-sub` можно было вынести на отдельный сервер: там не будет
//! доступа к базе, и все данные он получает отсюда. Это правильная схема и
//! с точки зрения живучести — сабку обычно держат на отдельном домене и
//! отдельной машине, чтобы блокировка панели не отрезала клиентов от конфигов.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::AppState;
use sn_core::{Error, Result};

pub fn sub_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sub/{short_id}", get(sub_data))
        .route("/api/sub/{short_id}/request", post(log_request))
        .route("/api/sub/{short_id}/device", post(register_device))
        .route("/api/sub/apps", get(apps_list))
        .route("/api/sub/settings", get(settings_for_sub))
        .route("/api/sub/rules-effective", get(rules_for_sub))
        .route("/api/sub/templates-effective", get(templates_for_sub))
}

/// Сервисный токен сабки. Отдельный от админского: у него нет прав на панель,
/// он умеет только читать данные подписок.
///
/// Ищем токен в базе — их выпускают из панели, и сменить токен можно не
/// заходя на сервер. Переменную окружения оставляем как запасной путь: у тех,
/// кто настроил сабку до появления этой страницы, всё продолжает работать.
async fn check_service_token(st: &AppState, headers: &HeaderMap) -> Result<()> {
    let got = headers
        .get("x-service-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;

    let hash = sn_core::auth::token_hash(got);
    let row = sqlx::query(
        "SELECT id, last_used_at FROM service_tokens
          WHERE kind = 'sub' AND token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(&hash)
    .fetch_optional(&st.pool)
    .await?;

    if let Some(row) = row {
        // Отметку «последнего использования» обновляем не чаще раза в минуту:
        // иначе каждый запрос подписки превращается в запись в базу.
        let id: i64 = row.get("id");
        let stale = row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at")
            .ok()
            .flatten()
            .map(|t| chrono::Utc::now() - t > chrono::Duration::minutes(1))
            .unwrap_or(true);
        if stale {
            let _ = sqlx::query("UPDATE service_tokens SET last_used_at = now() WHERE id = $1")
                .bind(id)
                .execute(&st.pool)
                .await;
        }
        return Ok(());
    }

    // После первого выпуска через панель старый токен окружения больше
    // не действует: иначе кнопка перевыпуска не отзывала бы все старые ключи.
    let managed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM service_tokens WHERE kind = 'sub')")
        .fetch_one(&st.pool)
        .await?;
    if managed {
        return Err(Error::Unauthorized);
    }
    // Совместимость со старыми установками до первого выпуска через панель.
    let Some(expected) = st.config.sub_service_token.as_ref() else {
        // Токена нет ни в базе, ни в окружении — доступ снаружи закрыт.
        return Err(Error::Forbidden);
    };

    // Сравнение постоянного времени: длина токена не должна утекать по таймингу.
    if got.len() != expected.len()
        || got
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |a, (x, y)| a | (x ^ y))
            != 0
    {
        return Err(Error::Unauthorized);
    }
    Ok(())
}

/// Всё, что нужно сервису подписок для одного клиента: статус, лимиты и хосты.
async fn sub_data(
    State(st): State<AppState>,
    Path(short_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    check_service_token(&st, &headers).await?;

    load_sub_data(&st, &short_id).await.map(Json)
}

pub(crate) async fn load_sub_data(st: &AppState, short_id: &str) -> Result<Value> {
    let row = sqlx::query(
        "SELECT c.id, c.username, c.vpn_uuid,
                effective_status(c.status, s.expires_at, s.traffic_used_bytes,
                                 s.traffic_limit_bytes)::text AS status,
                s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes, s.device_limit,
                t.title AS tariff
           FROM clients c
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
           LEFT JOIN tariffs t       ON t.id = s.tariff_id
          WHERE c.short_id = $1 AND c.deleted_at IS NULL",
    )
    .bind(&short_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(Error::NotFound)?;

    let client_id: i64 = row.get("id");
    let vpn_uuid: uuid::Uuid = row.get("vpn_uuid");

    // Параметры Reality — только из профиля.
    //
    // Раньше они копировались в строку хоста при его создании. Стоило
    // перевыпустить ключи в профиле — и значения расходились: сервер ждал
    // один shortId, клиент присылал другой, локация выглядела нерабочей
    // при полностью исправной ноде. Копии больше нет, расходиться нечему.
    // Имя сервера у Reality тоже из профиля: клиент обязан назвать то,
    // что перечислено в serverNames. У остальных протоколов SNI задаётся
    // на хосте — это его собственный домен.
    let hosts = sqlx::query(
        "SELECT DISTINCT h.id, h.sort_order, h.remark, h.address, h.port,
                CASE WHEN h.options->>'security'='inherit' THEN COALESCE(i.security::text,h.security::text) ELSE h.security::text END AS security,
                COALESCE(i.sni, h.sni) AS sni,
                h.fingerprint, h.alpn, h.path,
                i.public_key, i.short_id,
                h.host_header, h.options, ht.body AS host_template,
                i.protocol, COALESCE(i.network, 'tcp') AS network,
                i.method, i.service_name, i.server_key
           FROM hosts h
           LEFT JOIN subscription_templates ht ON ht.id = (h.options->>'xray_template_id')::bigint AND ht.code='xray_json'
           JOIN inbounds i        ON i.id = h.inbound_id
           JOIN squad_inbounds si ON si.inbound_id = i.id
           JOIN client_squads cs  ON cs.squad_id = si.squad_id
           JOIN node_inbounds ni  ON ni.inbound_id = i.id
           JOIN nodes n           ON n.id = ni.node_id
          WHERE cs.client_id = $1
            AND h.is_enabled
            AND NOT COALESCE(h.options->'exclude_squad_ids','[]'::jsonb) @> jsonb_build_array(cs.squad_id)
            AND n.status = 'online'
            AND n.deleted_at IS NULL
          ORDER BY h.sort_order, h.id",
    )
    .bind(client_id)
    .fetch_all(&st.pool)
    .await?;

    Ok(json!({
        "external_squad":sn_sub::overrides::load_external(&st.pool,client_id).await?,
        "username": row.get::<String, _>("username"),
        "status": row.get::<String, _>("status"),
        "tariff": row.try_get::<Option<String>, _>("tariff").ok().flatten(),
        "vpn_uuid": vpn_uuid.to_string(),
        "expires_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")
            .ok().flatten().map(|d| d.to_rfc3339()),
        "traffic_used_bytes": row.try_get::<Option<i64>, _>("traffic_used_bytes").ok().flatten().unwrap_or(0),
        "traffic_limit_bytes": row.try_get::<Option<i64>, _>("traffic_limit_bytes").ok().flatten(),
        "device_limit": row.try_get::<Option<i32>, _>("device_limit").ok().flatten().unwrap_or(1),
        "hosts": hosts.iter().map(|h| json!({
            "options":h.get::<Value,_>("options"),
            "host_template":h.get::<Option<String>,_>("host_template"),
            "remark": h.get::<String, _>("remark"),
            "address": h.get::<String, _>("address"),
            "port": h.get::<i32, _>("port"),
            "protocol": h.get::<String, _>("protocol"),
            "network": h.get::<String, _>("network"),
            "security": h.get::<String, _>("security"),
            "sni": h.get::<Option<String>, _>("sni"),
            "fingerprint": h.get::<Option<String>, _>("fingerprint"),
            "alpn": h.get::<Option<String>, _>("alpn"),
            "path": h.get::<Option<String>, _>("path"),
            "public_key": h.get::<Option<String>, _>("public_key"),
            "short_id": h.get::<Option<String>, _>("short_id"),
            // Параметры протокола и транспорта: без них клиент соберёт
            // конфиг, который загрузится, но не соединится.
            "method": h.try_get::<Option<String>, _>("method").ok().flatten(),
            "service_name": h.try_get::<Option<String>, _>("service_name").ok().flatten(),
            "server_key": h.try_get::<Option<String>, _>("server_key").ok().flatten(),
            "host_header": h.try_get::<Option<String>, _>("host_header").ok().flatten(),
        })).collect::<Vec<_>>(),
    }))
}

/// Настройки страницы подписки — нужны удалённой сабке.
/// Настройки подписки и только публичные поля общего клиентского брендинга.
async fn settings_for_sub(State(st): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    check_service_token(&st, &headers).await?;
    // Брендинг отдаём вместе с настройками подписки: у вынесенной сабки нет
    // ничего своего, и название на её странице иначе было бы дефолтным.
    let rows = sqlx::query(
        "SELECT key, value FROM settings
          WHERE key LIKE 'subscription.%' OR key LIKE 'brand.%'",
    )
    .fetch_all(&st.pool)
    .await?;
    let mut map = serde_json::Map::new();
    for r in &rows {
        map.insert(r.get::<String, _>("key"), r.get::<Value, _>("value"));
    }
    let config:Option<Value>=sqlx::query_scalar("SELECT value FROM settings WHERE key='cabinet.config'").fetch_optional(&st.pool).await?;
    map.insert("subscription.page_branding".into(),serde_json::to_value(sn_core::customer_brand::CustomerBrand::from_config(&config.unwrap_or(Value::Null))).map_err(|e|Error::Internal(e.to_string()))?);
    Ok(Json(Value::Object(map)))
}

/// Правила ответов для вынесенной сабки: у неё нет доступа к базе,
/// а без правил она вернётся к встроенному списку приложений, и
/// настройки из панели молча перестанут работать.
async fn rules_for_sub(State(st): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    check_service_token(&st, &headers).await?;

    let rows = sqlx::query(
        "SELECT r.ua_pattern, r.action, t.body AS template, t.code AS template_code, r.conditions, r.operator, r.response_headers, r.disable_hwid_check
           FROM response_rules r
           LEFT JOIN subscription_templates t ON t.id = r.template_id
          WHERE r.is_active ORDER BY r.sort_order, r.id",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "ua_pattern": r.get::<String, _>("ua_pattern"),
            "action": r.get::<String, _>("action"),
            "template": r.try_get::<Option<String>, _>("template").ok().flatten(),
            "template_code":r.get::<Option<String>,_>("template_code"),"conditions":r.get::<Option<Value>,_>("conditions"),"operator":r.get::<String,_>("operator"),"response_headers":r.get::<Value,_>("response_headers"),"disable_hwid_check":r.get::<bool,_>("disable_hwid_check"),
        }))
        .collect::<Vec<_>>())))
}

/// Список приложений для страницы подписки — нужен удалённой сабке.
async fn apps_list(State(st): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    check_service_token(&st, &headers).await?;
    let rows = sqlx::query(
        "SELECT platform, name, deeplink, store_url, guide, sort_order, icon_url, icon_svg
           FROM subscription_page_apps WHERE is_active ORDER BY platform, sort_order",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "platform": r.get::<String, _>("platform"),
            "name": r.get::<String, _>("name"),
            "deeplink": r.get::<Option<String>, _>("deeplink"),
            "store_url": r.get::<Option<String>, _>("store_url"),
            "guide": r.get::<Option<String>, _>("guide"),
            "sort_order": r.get::<i32, _>("sort_order"),
            "icon_url": r.get::<Option<String>, _>("icon_url"),
            "icon_svg": r.get::<Option<String>, _>("icon_svg"),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct RequestLog {
    user_agent: Option<String>,
    ip: Option<String>,
    response_code: Option<String>,
}

/// Журнал запросов подписки. Пишется отсюда, чтобы удалённая сабка
/// не лезла в базу напрямую.
async fn log_request(
    State(st): State<AppState>,
    Path(short_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RequestLog>,
) -> Result<Json<Value>> {
    check_service_token(&st, &headers).await?;

    // IP приводим в SQL: тип inet требует отдельной фичи драйвера,
    // а строку Postgres разберёт сам. Мусор отсекаем заранее.
    let ip = body
        .ip
        .filter(|s| s.parse::<std::net::IpAddr>().is_ok());
    sqlx::query(
        "INSERT INTO subscription_requests (client_id, user_agent, ip, response_code)
         SELECT id, $2, $3::text::inet, $4 FROM clients WHERE short_id = $1",
    )
    .bind(&short_id)
    .bind(&body.user_agent)
    .bind(ip)
    .bind(&body.response_code)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

#[derive(serde::Deserialize)]
struct DeviceBody {
    hwid: String,
    platform: Option<String>,
    model: Option<String>,
    app_version: Option<String>,
}

/// Учёт устройства, когда сабка вынесена на отдельный сервер и не имеет
/// доступа к базе. Логика та же, что в локальном режиме: лимит проверяем
/// только для нового HWID, уже известное устройство продолжает работать
/// даже после уменьшения лимита.
async fn register_device(
    State(st): State<AppState>,
    Path(short_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DeviceBody>,
) -> Result<Json<Value>> {
    check_service_token(&st, &headers).await?;

    let hwid: String = body.hwid.chars().take(128).collect();
    if hwid.trim().is_empty() {
        return Err(Error::bad("пустой идентификатор устройства"));
    }

    let row = sqlx::query(
        "SELECT c.id,
                COALESCE(s.device_limit, 0) AS device_limit,
                (SELECT count(*) FROM devices d WHERE d.client_id = c.id) AS used,
                EXISTS (SELECT 1 FROM devices d WHERE d.client_id = c.id AND d.hwid = $2) AS known
           FROM clients c
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
          WHERE c.short_id = $1 AND c.deleted_at IS NULL",
    )
    .bind(&short_id)
    .bind(&hwid)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(Error::NotFound)?;

    let client_id: i64 = row.get("id");
    let limit: i32 = row.get("device_limit");
    let used: i64 = row.get("used");
    let known: bool = row.get("known");

    if !known && limit > 0 && used >= limit as i64 {
        return Ok(Json(json!({ "over_limit": true, "used": used, "limit": limit })));
    }

    sqlx::query(
        "INSERT INTO devices (client_id, hwid, platform, model, app_version)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (client_id, hwid) DO UPDATE
            SET last_seen_at = now(),
                platform    = COALESCE(EXCLUDED.platform, devices.platform),
                model       = COALESCE(EXCLUDED.model, devices.model),
                app_version = COALESCE(EXCLUDED.app_version, devices.app_version)",
    )
    .bind(client_id)
    .bind(&hwid)
    .bind(body.platform.map(|s| s.chars().take(64).collect::<String>()))
    .bind(body.model.map(|s| s.chars().take(64).collect::<String>()))
    .bind(body.app_version.map(|s| s.chars().take(32).collect::<String>()))
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "over_limit": false, "used": used + if known { 0 } else { 1 }, "limit": limit })))
}

async fn templates_for_sub(State(st):State<AppState>,headers:HeaderMap)->Result<Json<Value>>{
    check_service_token(&st,&headers).await?;
    let rows=sqlx::query("SELECT code,body FROM subscription_templates WHERE is_default").fetch_all(&st.pool).await?;
    Ok(Json(Value::Object(rows.iter().map(|r|(r.get::<String,_>("code"),json!(r.get::<String,_>("body")))).collect())))
}

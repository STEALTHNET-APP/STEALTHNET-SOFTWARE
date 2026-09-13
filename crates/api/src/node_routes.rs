//! Протокол панель↔нода.
//!
//! Нода сама стучится в панель, а не наоборот. Так проще: у edge-серверов
//! может не быть белого IP или входящего доступа, а исходящий есть всегда.
//! Аутентификация — секрет ноды в заголовке; в БД лежит только его хэш.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get,post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::AppState;
use sn_core::{Error, Result};

fn yes() -> bool { true }

pub fn node_routes() -> Router<AppState> {
    Router::new()
        .route("/api/node/bootstrap",get(bootstrap))
        .route("/api/node/sync", post(sync))
        .route("/api/node/stats", post(stats))
        .route("/api/node/reports", post(reports))
}

/// Находит ноду по секрету. Секрет сравниваем по хэшу — в БД открытого нет.
async fn authenticate(st: &AppState, headers: &HeaderMap) -> Result<(i64, String)> {
    let secret = headers
        .get("x-node-secret")
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;

    let hash = sn_core::auth::token_hash(secret);
    let row = sqlx::query("SELECT id, name FROM nodes WHERE agent_secret_hash = $1 AND deleted_at IS NULL")
        .bind(&hash)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::Unauthorized)?;

    Ok((row.get("id"), row.get("name")))
}

/// Read-only installer handshake: validate the secret before changing a server.
async fn bootstrap(State(st):State<AppState>,headers:HeaderMap)->Result<Json<Value>> {
 let (id,name)=authenticate(&st,&headers).await?;
 let target:Option<String>=sqlx::query_scalar("SELECT value#>>'{}' FROM settings WHERE key='nodes.engine_version'").fetch_optional(&st.pool).await?.flatten();
 let target=target.filter(|t|!t.is_empty()).unwrap_or_else(||crate::xray_releases::INSTALL_DEFAULT.into());
 if !crate::xray_releases::valid_tag(&target){return Err(Error::bad("В панели задан недопустимый тег Xray"));}
 let row=sqlx::query("SELECT profile_id IS NOT NULL AS profile_assigned,reported_config_version,reported_users_version,engine_ok,COALESCE(last_seen_at>now()-interval '45 seconds',false) AS recent,status::text AS status,(SELECT count(*) FROM node_inbounds ni WHERE ni.node_id=nodes.id) AS inbound_count FROM nodes WHERE id=$1").bind(id).fetch_one(&st.pool).await?;
 if row.get::<String,_>("status")=="disabled" {return Err(Error::bad("Нода отключена в панели. Включите её перед установкой"));}
 let online = row.get::<bool,_>("recent") && row.get::<String,_>("status")=="online";
 let idle_ready = !row.get::<bool,_>("profile_assigned") && online && row.get::<bool,_>("engine_ok")
     && row.get::<Option<i32>,_>("reported_config_version")==Some(0)
     && row.get::<Option<String>,_>("reported_users_version").as_deref()==Some("unassigned");
 Ok(Json(json!({"node_id":id,"name":name,"engine_target":target,"idle_ready":idle_ready,"inbound_count":row.get::<i64,_>("inbound_count"),"profile_assigned":row.get::<bool,_>("profile_assigned"),"ready":row.get::<i64,_>("inbound_count")>0&&row.get::<bool,_>("profile_assigned")&&row.get::<bool,_>("engine_ok")&&row.get::<bool,_>("recent")&&row.get::<String,_>("status")=="online"})))
}

#[derive(Deserialize)]
struct SyncRequest {
    agent_version: Option<String>,
    #[serde(default)]
    safe_engine_update: bool,
    engine_version: Option<String>,
    /// Версия конфига, которая сейчас применена на ноде.
    config_version: Option<i32>,
    /// Отпечаток состава клиентов, который сейчас применён на ноде.
    #[serde(default)]
    users_version: Option<String>,
    /// Работает ли движок. Агент может быть жив, а Xray — лежать, и это
    /// принципиально разные состояния для того, кто смотрит в панель.
    #[serde(default = "yes")]
    engine_ok: bool,
    #[serde(default)]
    engine_error: Option<String>,
    online_count: Option<i32>,
    cpu_percent: Option<f32>,
    ram_percent: Option<f32>,
    uplink_bps: Option<i64>,
    downlink_bps: Option<i64>,
    la1: Option<f32>,
    la5: Option<f32>,
    la15: Option<f32>,
    cpu_model: Option<String>,
    cpu_cores: Option<i32>,
    kernel: Option<String>,
    mem_total_bytes: Option<i64>,
    mem_used_bytes: Option<i64>,
    uptime_seconds: Option<i64>,
    iface: Option<String>,
    rx_total_bytes: Option<i64>,
    tx_total_bytes: Option<i64>,
    plugins_status: Option<Value>,
}

#[derive(Deserialize)]
struct TorrentHit {
    hits: i32,
    last_target: Option<String>,
}

#[derive(Deserialize)]
struct ReportsBody {
    #[serde(default)]
    blocked_ips: Vec<String>,
    #[serde(default)]
    block_seconds: Option<i64>,
    #[serde(default)]
    torrents: std::collections::HashMap<String, TorrentHit>,
    #[serde(default)]
    domains: std::collections::HashMap<String, i32>,
}

/// Свод из журнала доступа ноды.
///
/// Приходят счётчики, а не строки журнала: копия истории посещений в
/// панели никому не нужна, а утечь может.
async fn reports(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<ReportsBody>,
) -> Result<Json<Value>> {
    let (node_id, _) = authenticate(&st, &headers).await?;

    if b.torrents.len() > 10000 || b.domains.len() > 10000
        || b.torrents.values().any(|v| v.hits < 0) || b.domains.values().any(|v| *v < 0) {
        return Err(Error::bad("некорректные счётчики отчёта"));
    }
    for (email, hit) in &b.torrents {
        // В поле email конфига движка стоит имя клиента: так его кладёт
        // сам агент при сборке списка пользователей. Проверено на живом
        // журнале — там «email: StealthNet_sup», а не UUID.
        sqlx::query(
            "INSERT INTO torrent_reports (client_id, node_id, day, hits, last_target)
             SELECT c.id, $2, (now() AT TIME ZONE 'UTC')::date, $3, $4
               FROM clients c WHERE c.username = $1 AND c.deleted_at IS NULL
               AND EXISTS (SELECT 1 FROM client_squads cs
                 JOIN squad_inbounds si ON si.squad_id=cs.squad_id
                 JOIN node_inbounds ni ON ni.inbound_id=si.inbound_id
                 WHERE cs.client_id=c.id AND ni.node_id=$2)
             ON CONFLICT (client_id, node_id, day) DO UPDATE
                SET hits = torrent_reports.hits + EXCLUDED.hits,
                    last_target = EXCLUDED.last_target,
                    updated_at = now()",
        )
        .bind(email)
        .bind(node_id)
        .bind(hit.hits)
        .bind(&hit.last_target)
        .execute(&st.pool)
        .await?;
    }

    // Домены принимаем, только если сбор включён: агент мог не успеть
    // получить выключенный флаг, и лишние данные копиться не должны.
    let collect: bool = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'reports.http_enabled'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

    if collect {
        for (domain, hits) in &b.domains {
            sqlx::query(
                "INSERT INTO http_domain_stats (node_id, day, domain, hits)
                 VALUES ($1, (now() AT TIME ZONE 'UTC')::date, $2, $3)
                 ON CONFLICT (node_id, day, domain) DO UPDATE
                    SET hits = http_domain_stats.hits + EXCLUDED.hits",
            )
            .bind(node_id)
            .bind(domain)
            .bind(hits)
            .execute(&st.pool)
            .await?;
        }
    }

    for ip in &b.blocked_ips {
        let _ = sqlx::query(
            "INSERT INTO node_ip_blocks (node_id, ip, reason, until)
             VALUES ($1, $2::inet, 'торренты',
                     CASE WHEN $3::bigint > 0 THEN now() + ($3 || ' seconds')::interval END)
             ON CONFLICT (node_id, ip) DO UPDATE
                SET until = EXCLUDED.until, at = now()",
        )
        .bind(node_id)
        .bind(ip)
        .bind(b.block_seconds.unwrap_or(0))
        .execute(&st.pool)
        .await;
    }

    Ok(Json(json!({ "ok": true })))
}

/// Пульс ноды. Возвращает конфиг, только если он изменился —
/// иначе каждые несколько секунд гоняли бы килобайты JSON впустую.
async fn sync(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SyncRequest>,
) -> Result<Json<Value>> {
    let (node_id, node_name) = authenticate(&st, &headers).await?;

    sqlx::query(
        // Сведения о сервере пишем в саму ноду, а не в метрики: они почти
        // не меняются, и хранить их каждые 15 секунд значит забить таблицу
        // миллионами одинаковых строк.
        "UPDATE nodes
            SET status = 'online',
                last_seen_at = now(),
                agent_version   = COALESCE($2, agent_version),
                engine_version  = COALESCE($3, engine_version),
                cpu_model       = COALESCE($4, cpu_model),
                cpu_cores       = COALESCE($5, cpu_cores),
                kernel          = COALESCE($6, kernel),
                mem_total_bytes = COALESCE($7, mem_total_bytes),
                mem_used_bytes  = COALESCE($8, mem_used_bytes),
                uptime_seconds  = COALESCE($9, uptime_seconds),
                iface           = COALESCE($10, iface),
                rx_total_bytes  = COALESCE($11, rx_total_bytes),
                tx_total_bytes  = COALESCE($12, tx_total_bytes),
                plugins_status  = COALESCE($13, plugins_status),
                -- Состояние движка отдельно от состояния агента: агент
                -- может исправно отвечать, пока Xray лежит.
                engine_ok       = $14,
                engine_error    = $15,
                reported_config_version = $16,
                reported_users_version = $17,
                safe_engine_update = $18
          WHERE id = $1",
    )
    .bind(node_id)
    .bind(&req.agent_version)
    .bind(&req.engine_version)
    .bind(&req.cpu_model)
    .bind(req.cpu_cores)
    .bind(&req.kernel)
    .bind(req.mem_total_bytes)
    .bind(req.mem_used_bytes)
    .bind(req.uptime_seconds)
    .bind(&req.iface)
    .bind(req.rx_total_bytes)
    .bind(req.tx_total_bytes)
    .bind(&req.plugins_status)
    .bind(req.engine_ok)
    .bind(req.engine_error.as_ref().map(|e| e.chars().take(400).collect::<String>()))
    .bind(req.config_version)
    .bind(&req.users_version)
    .bind(req.safe_engine_update)
    .execute(&st.pool)
    .await?;

    if req.cpu_percent.is_some() || req.online_count.is_some() {
        sqlx::query(
            "INSERT INTO node_metrics
                (node_id, cpu_percent, ram_percent, online_count,
                 uplink_bps, downlink_bps, la1, la5, la15)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(node_id)
        .bind(req.cpu_percent)
        .bind(req.ram_percent)
        .bind(req.online_count)
        .bind(req.uplink_bps)
        .bind(req.downlink_bps)
        .bind(req.la1)
        .bind(req.la5)
        .bind(req.la15)
        .execute(&st.pool)
        .await?;
    }

    let profile = sqlx::query(
        "SELECT p.id, p.name, p.config, p.version
           FROM config_profiles p JOIN nodes n ON n.profile_id = p.id
          WHERE n.id = $1",
    )
    .bind(node_id)
    .fetch_optional(&st.pool)
    .await?;

    // Тумблер сбора доменов живёт в панели: иначе выключить его можно
    // было бы только зайдя на каждую ноду.
    // Отметка «перезапустить движок». Отдаём агенту как число: он
    // помнит отработанную и перезапускает ровно один раз на нажатие.
    let restart_token: Option<i64> = sqlx::query_scalar(
        "SELECT (extract(epoch from restart_requested_at) * 1000)::bigint
           FROM nodes WHERE id = $1",
    )
    .bind(node_id)
    .fetch_optional(&st.pool)
    .await?
    .flatten();

    // Желаемая версия движка. Одна на весь парк: держать ноды на разных
    // ядрах — значит однажды получить локацию, которая не работает у
    // половины клиентов, и искать причину по одной ноде.
    let engine_target: Option<String> = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'nodes.engine_version'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_str().map(str::to_string))
    .filter(|v| !v.trim().is_empty())
    .filter(|_| req.safe_engine_update || safe_engine_updater(req.agent_version.as_deref()));

    // Отметка «обновить агента». Одна на парк: агент — наш код, и держать
    // ноды на разных его сборках незачем.
    let agent_token: Option<i64> = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'nodes.agent_update_at'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_i64()).or(Some(0));

    let collect_domains: bool = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT value FROM settings WHERE key = 'reports.http_enabled'",
    )
    .fetch_optional(&st.pool)
    .await?
    .flatten()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

    // Настройки плагинов шлём с каждым ответом, а не только при смене
    // конфига движка: ждать блокировку адреса до следующего релиза
    // профиля нельзя.
    let plugins_cfg: Value = sqlx::query_scalar("SELECT plugins FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_optional(&st.pool)
        .await?
        .unwrap_or(Value::Null);

    // Снятия за последние десять минут: агент опрашивает раз в 15 секунд,
    // и такого окна с запасом хватает, чтобы он их увидел даже после
    // короткого обрыва связи.
    let unblock_ips: Vec<String> = sqlx::query_scalar(
        "SELECT ip::text FROM node_ip_unblocks
          WHERE node_id = $1 AND at > now() - interval '10 minutes'",
    )
    .bind(node_id)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    let Some(profile) = profile else {
        return Ok(Json(json!({
            "ok": true,
            "config_changed": req.config_version != Some(0) || req.users_version.as_deref() != Some("unassigned"),
            "config_version": 0,
            "users_version": "unassigned",
            "config": sn_core::service_parts::ensure_service_parts(&json!({"log":{"loglevel":"warning"},"inbounds":[],"outbounds":[{"tag":"direct","protocol":"freedom"}]})).0,
            "users": [],
            "collect_domains": collect_domains,
            "plugins": plugins_cfg,
            "unblock_ips": unblock_ips,
            "restart_token": restart_token,
            "engine_target": engine_target,
            "agent_token": agent_token,
            "note": "ноде не назначен профиль конфигурации",
        })));
    };

    let version: i32 = profile.get("version");

    // Отпечаток состава клиентов.
    //
    // Версия профиля меняется только при правке конфига, а состав
    // клиентов — при каждой покупке, отзыве и правке сквада. Раньше нода
    // сверялась лишь с версией, поэтому новый клиент не получал доступа,
    // а отключённый продолжал работать, пока кто-нибудь не отредактирует
    // профиль. Считаем отдельный отпечаток и сравниваем его тоже.
    let users_version: String = sqlx::query_scalar(
        "SELECT COALESCE(md5(string_agg(c.vpn_uuid::text || ':' || i.tag, ','
                                        ORDER BY c.vpn_uuid, i.tag)), 'empty')
           FROM clients c
           JOIN client_squads cs  ON cs.client_id = c.id
           JOIN squad_inbounds si ON si.squad_id = cs.squad_id
           JOIN inbounds i        ON i.id = si.inbound_id
           JOIN node_inbounds ni  ON ni.inbound_id = i.id
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
          WHERE ni.node_id = $1
            AND c.deleted_at IS NULL
            AND effective_status(c.status, s.expires_at, s.traffic_used_bytes,
                                 s.traffic_limit_bytes) = 'active'",
    )
    .bind(node_id)
    .fetch_one(&st.pool)
    .await?;

    let users_version=format!("{}:{}",profile.get::<i64,_>("id"),users_version);
    if req.config_version == Some(version) && req.users_version.as_deref() == Some(users_version.as_str()) {
        return Ok(Json(json!({
            "ok": true,
            "config_changed": false,
            "config_version": version,
            "users_version": users_version,
            "collect_domains": collect_domains,
            "restart_token": restart_token,
            "engine_target": engine_target,
            "agent_token": agent_token,
            "plugins": plugins_cfg,
            "unblock_ips": unblock_ips,
        })));
    }

    // Клиенты для инбаундов ноды: только активные и только те,
    // чьи сквады дают доступ к поднятым здесь инбаундам.
    let users = sqlx::query(
        "SELECT DISTINCT c.vpn_uuid, c.username, i.tag AS inbound
           FROM clients c
           JOIN client_squads cs  ON cs.client_id = c.id
           JOIN squad_inbounds si ON si.squad_id = cs.squad_id
           JOIN inbounds i        ON i.id = si.inbound_id
           JOIN node_inbounds ni  ON ni.inbound_id = i.id
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
          WHERE ni.node_id = $1
            AND c.deleted_at IS NULL
            -- Фактический статус, а не хранимый: клиент с прошедшей датой
            -- не должен оставаться в конфиге ноды до пробуждения воркера.
            AND effective_status(c.status, s.expires_at, s.traffic_used_bytes,
                                 s.traffic_limit_bytes) = 'active'",
    )
    .bind(node_id)
    .fetch_all(&st.pool)
    .await?;

    tracing::info!(node = node_name, version, users = users.len(), "нода забрала конфиг");

    Ok(Json(json!({
        "ok": true,
        "config_changed": true,
        "config_version": version,
        "users_version": users_version,
        "collect_domains": collect_domains,
        "restart_token": restart_token,
        "engine_target": engine_target,
        "agent_token": agent_token,
        "plugins": plugins_cfg,
        "unblock_ips": unblock_ips,
        "profile": profile.get::<String, _>("name"),
        "config": profile.get::<Value, _>("config"),
        "users": users.iter().map(|u| json!({
            "uuid": u.get::<uuid::Uuid, _>("vpn_uuid").to_string(),
            "email": u.get::<String, _>("username"),
            "inbound": u.get::<String, _>("inbound"),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct StatsRequest {
    /// Потребление с прошлой отправки, по клиентам.
    usage: Vec<UsageItem>,
}

#[derive(Deserialize)]
struct UsageItem {
    uuid: String,
    upload_bytes: i64,
    download_bytes: i64,
}

fn validate_usage(items: &[UsageItem]) -> Result<()> {
    if items.len() > 10000 { return Err(Error::bad("слишком большой пакет статистики")); }
    let mut seen = std::collections::HashSet::new();
    let mut total = 0i64;
    for item in items {
        if item.upload_bytes < 0 || item.download_bytes < 0 || !seen.insert(&item.uuid) {
            return Err(Error::bad("отрицательный или повторяющийся счётчик трафика"));
        }
        total = total.checked_add(item.upload_bytes).and_then(|n| n.checked_add(item.download_bytes))
            .ok_or_else(|| Error::bad("переполнение счётчика трафика"))?;
    }
    Ok(())
}

#[cfg(test)]
mod usage_security_tests {
    use super::*;
    fn usage(up: i64, down: i64) -> UsageItem { UsageItem { uuid: "test".into(), upload_bytes: up, download_bytes: down } }
    #[test]
    fn rejects_negative_duplicate_and_overflowing_batches() {
        assert!(validate_usage(&[usage(2,3)]).is_ok());
        assert!(validate_usage(&[usage(-1,3)]).is_err());
        assert!(validate_usage(&[usage(i64::MAX,1)]).is_err());
        assert!(validate_usage(&[usage(1,2),usage(1,2)]).is_err());
    }
}

/// Приём трафика с ноды.
///
/// ⚠️ Дата берётся как `(now() AT TIME ZONE 'UTC')::date`, а НЕ `current_date`:
/// драйвер выставляет соединению UTC, а psql администратора работает в поясе
/// сервера — из-за этого «сегодня» отличалось на день, и трафик попадал
/// в соседние сутки. Явная дата убирает зависимость от настроек соединения.
///
/// Нода присылает ДЕЛЬТУ с прошлой отправки, а не абсолютные счётчики:
/// после перезапуска Xray счётчики обнуляются, и абсолютные значения
/// дали бы отрицательный прирост.
async fn stats(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<Json<Value>> {
    let (node_id, _) = authenticate(&st, &headers).await?;

    validate_usage(&req.usage)?;
    if req.usage.is_empty() {
        return Ok(Json(json!({ "ok": true, "applied": 0 })));
    }

    let multiplier: f64 = sqlx::query_scalar("SELECT traffic_multiplier::float8 FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_one(&st.pool)
        .await?;
    let count_traffic: bool = sqlx::query_scalar("SELECT count_traffic FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_one(&st.pool)
        .await?;

    let mut tx = st.pool.begin().await?;
    let mut applied = 0u32;
    let (mut uploaded, mut downloaded) = (0i64, 0i64);

    for item in &req.usage {
        let Ok(uuid) = item.uuid.parse::<uuid::Uuid>() else { continue };
        if item.upload_bytes < 0 || item.download_bytes < 0 {
            continue; // защита от мусора: отрицательный трафик невозможен
        }

        let client_id: Option<i64> = sqlx::query_scalar(
            "SELECT c.id FROM clients c WHERE c.vpn_uuid=$1 AND c.deleted_at IS NULL
             AND EXISTS (SELECT 1 FROM client_squads cs
               JOIN squad_inbounds si ON si.squad_id=cs.squad_id
               JOIN node_inbounds ni ON ni.inbound_id=si.inbound_id
               WHERE cs.client_id=c.id AND ni.node_id=$2)")
            .bind(uuid)
            .bind(node_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(client_id) = client_id else { continue };

        if item.upload_bytes > 0 || item.download_bytes > 0 {
            sqlx::query("INSERT INTO client_node_activity(client_id,node_id,last_seen_at,upload_bytes,download_bytes) VALUES($1,$2,now(),$3,$4) ON CONFLICT(client_id,node_id) DO UPDATE SET last_seen_at=now(),upload_bytes=EXCLUDED.upload_bytes,download_bytes=EXCLUDED.download_bytes")
                .bind(client_id).bind(node_id).bind(item.upload_bytes).bind(item.download_bytes).execute(&mut *tx).await?;
            sqlx::query("UPDATE clients SET last_online_at=now() WHERE id=$1")
                .bind(client_id).execute(&mut *tx).await?;
        }

        // Суточная запись: складываем дельты в ту же строку.
        sqlx::query(
            "INSERT INTO traffic_usage (client_id, node_id, day, upload_bytes, download_bytes)
             VALUES ($1, $2, (now() AT TIME ZONE 'UTC')::date, $3, $4)
             ON CONFLICT (day, client_id, node_id) DO UPDATE
                SET upload_bytes   = traffic_usage.upload_bytes   + EXCLUDED.upload_bytes,
                    download_bytes = traffic_usage.download_bytes + EXCLUDED.download_bytes",
        )
        .bind(client_id)
        .bind(node_id)
        .bind(item.upload_bytes)
        .bind(item.download_bytes)
        .execute(&mut *tx)
        .await?;

        // С лимита клиента списываем с учётом множителя ноды:
        // дорогая локация может расходовать пакет быстрее.
        if count_traffic {
            let scaled = ((item.upload_bytes + item.download_bytes) as f64) * multiplier;
            if !scaled.is_finite() || scaled < 0.0 || scaled >= i64::MAX as f64 {
                return Err(Error::bad("переполнение счётчика с множителем трафика"));
            }
            let charged = scaled as i64;
            sqlx::query(
                "UPDATE subscriptions SET traffic_used_bytes = traffic_used_bytes + $2
                  WHERE client_id = $1 AND is_current",
            )
            .bind(client_id)
            .bind(charged)
            .execute(&mut *tx)
            .await?;
        }
        applied += 1;
        uploaded += item.upload_bytes;
        downloaded += item.download_bytes;
    }

    sqlx::query(
        "INSERT INTO node_daily_stats (node_id, day, upload_bytes, download_bytes)
         VALUES ($1, (now() AT TIME ZONE 'UTC')::date, $2, $3)
         ON CONFLICT (node_id, day) DO UPDATE
            SET upload_bytes   = node_daily_stats.upload_bytes   + EXCLUDED.upload_bytes,
                download_bytes = node_daily_stats.download_bytes + EXCLUDED.download_bytes",
    )
    .bind(node_id)
    .bind(uploaded)
    .bind(downloaded)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Кто исчерпал пакет — переводим в «лимит»: подписка перестаёт отдавать конфиги.
    sqlx::query(
        "UPDATE clients SET status = 'limited'
          WHERE status = 'active' AND id IN (
              SELECT client_id FROM subscriptions
               WHERE is_current AND traffic_limit_bytes IS NOT NULL
                 AND traffic_used_bytes >= traffic_limit_bytes)",
    )
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "applied": applied })))
}

// Legacy agents replace the running executable directly and cannot roll back.
fn safe_engine_updater(version: Option<&str>) -> bool {
    let Some(version)=version else {return false};
    let parts:Option<Vec<u32>>=version.trim_start_matches('v').split('.').map(|s|s.parse().ok()).collect();
    matches!(parts,Some(v) if v.len()==3 && (v[0],v[1],v[2]) >= (0,1,2))
}
#[cfg(test)]
mod updater_tests {
    use super::*;
    #[test] fn only_compatible_agents_receive_core_updates() {
        for v in [None,Some("0.1.0"),Some("0.1.1"),Some("unknown")] {assert!(!safe_engine_updater(v));}
        for v in ["0.1.2","v0.2.0","1.0.0"] {assert!(safe_engine_updater(Some(v)));}
    }
}

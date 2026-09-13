//! Управление нодами из панели: создание, правка, установочные материалы.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn node_admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/nodes", post(node_create))
        .route("/api/nodes/xray-releases",get(crate::xray_releases::list))
        .route(
            "/api/nodes/{id}",
            axum::routing::patch(node_update).delete(node_delete),
        )
        .route("/api/nodes/{id}/rotate-secret", post(node_rotate_secret))
        .route("/api/nodes/{id}/install", get(node_install))
        .route("/api/nodes/{id}/installation-command", post(node_installation_command))
        .route("/api/nodes/{id}/reset-traffic", post(node_reset_traffic))
        .route("/api/nodes/{id}/restart-engine", post(node_restart_engine))
        .route("/api/nodes/update-agents", post(nodes_update_agents))
        .route("/api/nodes/traffic", get(nodes_traffic))
        .route("/api/nodes/{id}/metrics", get(node_metrics))
        .route("/api/nodes/{id}/bgp", get(crate::node_bgp::check))
        .route("/api/nodes/{id}/plugins", get(plugins_get).patch(plugins_set))
        .route("/api/nodes/{id}/blocks", get(blocks_list))
        .route("/api/nodes/{id}/blocks/{ip}", axum::routing::delete(block_remove))
}

#[derive(Deserialize)]
struct CreateNode {
    name: String,
    country_code: String,
    address: String,
    api_port: Option<i32>,
    profile_id: Option<i64>,
    /// Инбаунды профиля, которые поднимаются именно на этой ноде.
    /// Пусто — берём все инбаунды профиля.
    inbound_tags: Option<Vec<String>>,
    traffic_multiplier: Option<f64>,
    count_traffic: Option<bool>,
    notify: Option<bool>,
}

/// Создание ноды.
///
/// Секрет показывается ровно один раз: в базе лежит только его SHA-256.
/// Потерялся — перевыпустить, восстановить нельзя.
async fn node_create(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<CreateNode>,
) -> Result<Json<Value>> {
    let name = b.name.trim();
    if name.is_empty() {
        return Err(Error::bad("имя ноды не может быть пустым"));
    }
    if !valid_node_address(b.address.trim()) {
        return Err(Error::bad("Укажите IP или домен сервера без протокола, порта и пути"));
    }
    if b.country_code.len() != 2 {
        return Err(Error::bad("код страны — две буквы, например NL"));
    }

    install_panel_url(&st).await?;
    let secret = sn_core::auth::generate_token();
    let hash = sn_core::auth::token_hash(&secret);

    let mut tx = st.pool.begin().await?;
    crate::profile_workflow::placement_lock(&mut tx).await?;

    let node_id: i64 = sqlx::query_scalar(
        "INSERT INTO nodes (name, country_code, address, api_port, profile_id,
                            status, agent_secret_hash, traffic_multiplier,
                            count_traffic, notify)
         VALUES ($1, upper($2), $3, COALESCE($4, 2222), $5, 'provisioning', $6,
                 COALESCE($7, 1.0), COALESCE($8, true), COALESCE($9, true))
         RETURNING id",
    )
    .bind(name)
    .bind(b.country_code.trim())
    .bind(b.address.trim())
    .bind(b.api_port)
    .bind(b.profile_id)
    .bind(&hash)
    .bind(b.traffic_multiplier)
    .bind(b.count_traffic)
    .bind(b.notify)
    .fetch_one(&mut *tx)
    .await?;
    crate::profile_workflow::check_assigned_site(&mut tx, b.profile_id).await?;

    // Привязка инбаундов: без неё нода получит конфиг, но ни один клиент
    // в него не попадёт — и это выглядит как «всё работает, но не работает».
    if let Some(profile_id) = b.profile_id {
        let inserted = match &b.inbound_tags {
            Some(tags) if !tags.is_empty() => {
                sqlx::query(
                    "INSERT INTO node_inbounds (node_id, inbound_id)
                     SELECT $1, i.id FROM inbounds i
                      WHERE i.profile_id = $2 AND i.tag = ANY($3)
                     ON CONFLICT DO NOTHING",
                )
                .bind(node_id)
                .bind(profile_id)
                .bind(tags)
                .execute(&mut *tx)
                .await?
            }
            _ => {
                sqlx::query(
                    "INSERT INTO node_inbounds (node_id, inbound_id)
                     SELECT $1, i.id FROM inbounds i WHERE i.profile_id = $2
                     ON CONFLICT DO NOTHING",
                )
                .bind(node_id)
                .bind(profile_id)
                .execute(&mut *tx)
                .await?
            }
        };
        if inserted.rows_affected() == 0 {
            tracing::warn!(node = name, "у ноды нет инбаундов — клиенты её не увидят");
        }
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id, payload)
         VALUES ('admin', $1, 'node.create', 'node', $2, $3)",
    )
    .bind(admin.id)
    .bind(node_id)
    .bind(json!({ "name": name, "address": b.address }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(json!({
        "id": node_id,
        "secret": secret,
        "install": build_install(&st, node_id, &secret).await?,
    })))
}

#[derive(Deserialize)]
struct UpdateNode {
    name: Option<String>,
    country_code: Option<String>,
    address: Option<String>,
    api_port: Option<i32>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    profile_id: Option<Option<i64>>,
    inbound_tags: Option<Vec<String>>,
    traffic_multiplier: Option<f64>,
    count_traffic: Option<bool>,
    notify: Option<bool>,
    status: Option<String>,
    /// Расходы: у кого арендован сервер, почём и когда списывают.
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    infra_provider_id: Option<Option<i64>>,
    monthly_cost_minor: Option<i64>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    bill_day: Option<Option<i32>>,
}

async fn node_update(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<UpdateNode>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    crate::profile_workflow::placement_lock(&mut tx).await?;

    let old_profile: Option<i64> = sqlx::query_scalar(
        "SELECT profile_id FROM nodes WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let profile_changed = b.profile_id.is_some_and(|profile| profile != old_profile);
    if b.profile_id.flatten().is_some_and(|profile| profile <= 0) {
        return Err(Error::bad("Выберите профиль или вариант «Без профиля»"));
    }

    let res = sqlx::query(
        "UPDATE nodes
            SET name = COALESCE($2, name),
                country_code = COALESCE(upper($3), country_code),
                address = COALESCE($4, address),
                api_port = COALESCE($5, api_port),
                profile_id = CASE WHEN $16 THEN $6 ELSE profile_id END,
                reported_config_version = CASE WHEN $17 THEN NULL ELSE reported_config_version END,
                reported_users_version = CASE WHEN $17 THEN NULL ELSE reported_users_version END,
                traffic_multiplier = COALESCE($7, traffic_multiplier),
                count_traffic = COALESCE($8, count_traffic),
                notify = COALESCE($9, notify),
                status = COALESCE($10::node_status, status),
                infra_provider_id  = CASE WHEN $14 THEN $11 ELSE infra_provider_id END,
                monthly_cost_minor = COALESCE($12, monthly_cost_minor),
                bill_day           = CASE WHEN $15 THEN $13 ELSE bill_day END
          WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(b.name.as_deref().map(str::trim))
    .bind(&b.country_code)
    .bind(b.address.as_deref().map(str::trim))
    .bind(b.api_port)
    .bind(b.profile_id.flatten())
    .bind(b.traffic_multiplier)
    .bind(b.count_traffic)
    .bind(b.notify)
    .bind(&b.status)
    .bind(b.infra_provider_id.flatten())
    .bind(b.monthly_cost_minor)
    .bind(b.bill_day.flatten())
    .bind(b.infra_provider_id.is_some())
    .bind(b.bill_day.is_some())
    .bind(b.profile_id.is_some())
    .bind(profile_changed)
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    crate::profile_workflow::check_assigned_site(&mut tx, b.profile_id.unwrap_or(old_profile)).await?;

    // Releasing/reassigning a test node also ends its rehearsal. Keep the
    // candidate profile so the administrator can inspect or reuse it.
    // Lock order is node -> trial, shared with trial_finish.
    if profile_changed {
        sqlx::query("UPDATE profile_trials SET state='finished' WHERE node_id=$1 AND state='testing'")
            .bind(id).execute(&mut *tx).await?;
    }

    // Список инбаундов заменяем целиком: частичное обновление здесь
    // означало бы «добавить», и убрать инбаунд стало бы невозможно.
    if let Some(tags) = &b.inbound_tags {
        sqlx::query("DELETE FROM node_inbounds WHERE node_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO node_inbounds (node_id, inbound_id)
             SELECT $1, i.id FROM inbounds i
              JOIN nodes n ON n.id = $1
             WHERE i.profile_id = n.profile_id AND i.tag = ANY($2)",
        )
        .bind(id)
        .bind(tags)
        .execute(&mut *tx)
        .await?;
    } else if b.profile_id.is_some() {
        // Сменили профиль, а инбаунды не прислали.
        //
        // Привязки остаются от прежнего профиля и указывают на его
        // инбаунды. Дальше их отсекает условие i.profile_id = n.profile_id,
        // и нода оказывается без единого инбаунда: в панели профиль новый,
        // а клиенты локацию не видят. Пересобираем список под новый
        // профиль — по совпадающим тегам, а если таких нет, берём все его
        // инбаунды, как при создании ноды.
        let old_tags: Vec<String> = sqlx::query_scalar(
            "SELECT i.tag FROM node_inbounds ni
               JOIN inbounds i ON i.id = ni.inbound_id
              WHERE ni.node_id = $1",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM node_inbounds WHERE node_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let carried = sqlx::query(
            "INSERT INTO node_inbounds (node_id, inbound_id)
             SELECT $1, i.id FROM inbounds i
              JOIN nodes n ON n.id = $1
             WHERE i.profile_id = n.profile_id AND i.tag = ANY($2)
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(&old_tags)
        .execute(&mut *tx)
        .await?;

        if carried.rows_affected() == 0 {
            sqlx::query(
                "INSERT INTO node_inbounds (node_id, inbound_id)
                 SELECT $1, i.id FROM inbounds i
                  JOIN nodes n ON n.id = $1
                 WHERE i.profile_id = n.profile_id
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'node.update', 'node', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn node_delete(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Мягкое удаление: история трафика и метрики привязаны к ноде,
    // а отчёты за прошлые месяцы должны остаться читаемыми.
    let res = sqlx::query(
        "UPDATE nodes SET deleted_at = now(), status = 'disabled'
          WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(&st.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'node.delete', 'node', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// Перевыпуск секрета. Старый перестаёт работать немедленно —
/// нода отвалится, пока не обновят её настройки.
async fn node_rotate_secret(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    install_panel_url(&st).await?;
    let secret = sn_core::auth::generate_token();
    let hash = sn_core::auth::token_hash(&secret);

    let res = sqlx::query(
        "UPDATE nodes SET agent_secret_hash = $2, status = 'provisioning'
          WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(&hash)
    .execute(&st.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'node.rotate_secret', 'node', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({
        "secret": secret,
        "install": build_install(&st, id, &secret).await?,
    })))
}

/// Установочные материалы. Секрет доступен только сразу после создания
/// или перевыпуска — здесь возвращаем шаблон с подстановкой.
async fn node_install(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let mut info=build_install(&st, id, "ВАШ_СЕКРЕТ_ИЗ_ПАНЕЛИ").await?;
    let seen: bool = sqlx::query_scalar("SELECT last_seen_at IS NOT NULL FROM nodes WHERE id=$1 AND deleted_at IS NULL").bind(id).fetch_optional(&st.pool).await?.ok_or(Error::NotFound)?;
    info["previously_connected"] = json!(seen);
    Ok(Json(info))
}

/// Recover a lost first-install command without silently revoking a live agent.
async fn node_installation_command(
    CurrentAdmin(admin): CurrentAdmin, State(st): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<Value>> {
    install_panel_url(&st).await?;
    let mut tx=st.pool.begin().await?;
    let seen:bool=sqlx::query_scalar("SELECT last_seen_at IS NOT NULL FROM nodes WHERE id=$1 AND deleted_at IS NULL FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    if seen { return Err(Error::bad("This node has already connected. Use the explicit key replacement action / Эта нода уже подключалась. Используйте отдельное действие замены ключа")); }
    let secret=sn_core::auth::generate_token();
    sqlx::query("UPDATE nodes SET agent_secret_hash=$2,status='provisioning' WHERE id=$1")
        .bind(id).bind(sn_core::auth::token_hash(&secret)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id) VALUES('admin',$1,'node.installation_command','node',$2)")
        .bind(admin.id).bind(id).execute(&mut *tx).await?;
    let mut install=build_install(&st,id,&secret).await?;
    install["reissued"]=json!(true);
    tx.commit().await?;
    Ok(Json(json!({"install":install})))
}

/// Собирает готовые к вставке материалы: compose, systemd и одну команду.
#[derive(Deserialize)]
struct TrafficQuery {
    days: Option<i32>,
}

/// Трафик по нодам и дням.
///
/// Для тепловой карты: рисовать её на сгенерированных числах — значит
/// показывать нагрузку, которой не было, и принимать по ней решения.
async fn nodes_traffic(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<TrafficQuery>,
) -> Result<Json<Value>> {
    let days = q.days.unwrap_or(30).clamp(1, 90);

    let rows = sqlx::query(
        "SELECT n.id, n.name, n.country_code, tu.day,
                sum(tu.upload_bytes + tu.download_bytes)::bigint AS bytes
           FROM nodes n
           LEFT JOIN traffic_usage tu
                  ON tu.node_id = n.id
                 AND tu.day >= (now() AT TIME ZONE 'UTC')::date - $1::int
          WHERE n.deleted_at IS NULL
          GROUP BY n.id, n.name, n.country_code, tu.day
          ORDER BY n.name, tu.day",
    )
    .bind(days)
    .fetch_all(&st.pool)
    .await?;

    let mut by_node: std::collections::BTreeMap<i64, Value> = Default::default();
    for r in &rows {
        let id: i64 = r.get("id");
        let entry = by_node.entry(id).or_insert_with(|| {
            json!({
                "id": id,
                "name": r.get::<String, _>("name"),
                "country_code": r.get::<String, _>("country_code"),
                "days": {},
            })
        });
        // day = NULL означает «за период данных нет» — не ноль за день,
        // а отсутствие записи; в карте это пустая клетка.
        if let Ok(Some(day)) = r.try_get::<Option<chrono::NaiveDate>, _>("day") {
            let bytes: i64 = r.try_get("bytes").unwrap_or(0);
            entry["days"][day.to_string()] = json!(bytes);
        }
    }

    Ok(Json(json!({
        "days": days,
        "nodes": by_node.into_values().collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct MetricsQuery {
    hours: Option<i32>,
}

/// История метрик ноды: онлайн и нагрузка по времени.
///
/// Нужна графикам. Рисовать их на сгенерированных числах — показывать
/// нагрузку, которой не было, и принимать по ней решения.
async fn node_metrics(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    axum::extract::Query(q): axum::extract::Query<MetricsQuery>,
) -> Result<Json<Value>> {
    let hours = q.hours.unwrap_or(24).clamp(1, 168);

    let rows = sqlx::query(
        "SELECT at, online_count, cpu_percent, ram_percent, uplink_bps, downlink_bps
           FROM node_metrics
          WHERE node_id = $1 AND at >= now() - ($2 || ' hours')::interval
          ORDER BY at",
    )
    .bind(id)
    .bind(hours.to_string())
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "at": r.get::<chrono::DateTime<chrono::Utc>, _>("at").to_rfc3339(),
            "online": r.try_get::<Option<i32>, _>("online_count").ok().flatten().unwrap_or(0),
            "cpu": r.try_get::<Option<f32>, _>("cpu_percent").ok().flatten(),
            "rx_bps": r.try_get::<Option<i64>, _>("downlink_bps").ok().flatten(),
            "tx_bps": r.try_get::<Option<i64>, _>("uplink_bps").ok().flatten(),
        }))
        .collect::<Vec<_>>())))
}

/// Настройки плагинов ноды и состояние сервера.
async fn plugins_get(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let row = sqlx::query("SELECT plugins, plugins_status FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(Json(json!({
        "config": row.get::<Value, _>("plugins"),
        // Что сообщил агент. Без nftables и прав NET_ADMIN плагины не
        // работают, и панель обязана показать это, а не скрыть.
        "status": row.try_get::<Option<Value>, _>("plugins_status").ok().flatten(),
    })))
}

/// Проверка настроек до сохранения.
///
/// Кривой адрес не даст применить весь набор правил целиком, и нода
/// останется без фильтров — а администратор будет думать, что включил.
fn check_plugins(cfg: &Value) -> Result<()> {
    fn object<'a>(v: &'a Value, keys: &[&str]) -> Result<&'a serde_json::Map<String, Value>> {
        let map = v.as_object().ok_or_else(|| Error::bad("настройки плагина должны быть объектом"))?;
        for key in map.keys() {
            if !keys.contains(&key.as_str()) { return Err(Error::bad(format!("неизвестное поле плагина: {key}"))); }
        }
        Ok(map)
    }
    fn addresses(v: &Value, lists: &[String], allow_refs: bool) -> Result<()> {
        let arr = v.as_array().ok_or_else(|| Error::bad("список адресов должен быть массивом"))?;
        for item in arr {
            let s = item.as_str().ok_or_else(|| Error::bad("адрес должен быть строкой"))?;
            if let Some(name) = s.strip_prefix("ext:") {
                if allow_refs && lists.iter().any(|v| v == name || v == s) { continue; }
                return Err(Error::bad(format!("общий список «{name}» не найден или вложен в другой список")));
            }
            let (addr, mask) = s.split_once('/').map_or((s, None), |(a, m)| (a, Some(m)));
            let ip: std::net::IpAddr = addr.parse().map_err(|_| Error::bad(format!("некорректный IP: {s}")))?;
            if let Some(mask) = mask {
                let bits = mask.parse::<u8>().map_err(|_| Error::bad(format!("некорректная маска: {s}")))?;
                if bits > if ip.is_ipv4() { 32 } else { 128 } { return Err(Error::bad(format!("некорректная маска: {s}"))); }
            }
        }
        Ok(())
    }
    let cfg = object(cfg, &["ingressFilter", "egressFilter", "torrentBlocker", "sharedLists"])?;
    let mut names = Vec::new();
    if let Some(lists) = cfg.get("sharedLists") {
        for list in lists.as_array().ok_or_else(|| Error::bad("общие списки должны быть массивом"))? {
            let list = object(list, &["name", "items"])?;
            let name = list.get("name").and_then(Value::as_str).filter(|v| !v.trim().is_empty())
                .ok_or_else(|| Error::bad("у общего списка должно быть имя"))?;
            if names.iter().any(|v: &String| v.trim_start_matches("ext:") == name.trim_start_matches("ext:")) {
                return Err(Error::bad("имена общих списков должны быть уникальными"));
            }
            addresses(list.get("items").ok_or_else(|| Error::bad("у общего списка нет items"))?, &[], false)?;
            names.push(name.to_string());
        }
    }
    for key in ["ingressFilter", "egressFilter", "torrentBlocker"] {
        let Some(value) = cfg.get(key) else { continue };
        let torrent = key == "torrentBlocker";
        let map = object(value, if torrent { &["enabled", "ignoreIps", "blockDuration"] } else { &["enabled", "blockedIps", "blockedPorts"] })?;
        if map.get("enabled").is_some_and(|v| !v.is_boolean()) { return Err(Error::bad("enabled должен быть true или false")); }
        if let Some(ips) = map.get(if torrent { "ignoreIps" } else { "blockedIps" }) { addresses(ips, &names, true)?; }
        if let Some(ports) = map.get("blockedPorts") {
            for p in ports.as_array().ok_or_else(|| Error::bad("порты должны быть массивом"))? {
                if !p.as_u64().is_some_and(|n| (1..=65535).contains(&n)) { return Err(Error::bad("порт должен быть целым числом от 1 до 65535")); }
            }
        }
        if map.get("blockDuration").is_some_and(|v| !v.as_u64().is_some_and(|n| n <= 604_800)) {
            return Err(Error::bad("срок блокировки — целое число от 0 до 604800 секунд"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod plugin_validation_tests {
    use super::*;
    #[test]
    fn accepts_filters_and_resolved_shared_lists() {
        assert!(check_plugins(&json!({"ingressFilter":{"enabled":true,"blockedIps":["ext:office"]},"sharedLists":[{"name":"office","items":["192.0.2.0/24","2001:db8::/32"]}]})).is_ok());
        assert!(check_plugins(&json!({})).is_ok());
    }
    #[test]
    fn rejects_ignored_fields_invalid_masks_and_types() {
        for cfg in [json!({"ip_filter":{}}),json!({"egressFilter":{"enabled":"true"}}),json!({"ingressFilter":{"blockedIps":["192.0.2.1/99"]}}),json!({"egressFilter":{"blockedPorts":[0]}}),json!({"torrentBlocker":{"blockDuration":-1}}),json!({"ingressFilter":{"blockedIps":["ext:missing"]}}),json!({"sharedLists":[{"name":"a","items":["ext:a"]}]})] {
            assert!(check_plugins(&cfg).is_err(), "accepted {cfg}");
        }
    }
}

async fn plugins_set(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(cfg): Json<Value>,
) -> Result<Json<Value>> {
    check_plugins(&cfg)?;

    let res = sqlx::query("UPDATE nodes SET plugins = $2 WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .bind(&cfg)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id, payload)
         VALUES ('admin', $1, 'node.plugins', 'node', $2, $3)",
    )
    .bind(admin.id)
    .bind(id)
    .bind(&cfg)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// Кого сейчас блокирует нода.
async fn blocks_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT ip::text AS ip, reason, until, at FROM node_ip_blocks
          WHERE node_id = $1 AND (until IS NULL OR until > now())
          ORDER BY at DESC LIMIT 200",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "ip": r.get::<String, _>("ip"),
            "reason": r.get::<String, _>("reason"),
            "until": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("until")
                .ok().flatten().map(|d| d.to_rfc3339()),
            "at": r.get::<chrono::DateTime<chrono::Utc>, _>("at").to_rfc3339(),
        }))
        .collect::<Vec<_>>())))
}

/// Снять блокировку.
///
/// Удалить строку мало: запись живёт в ядре ноды, и агенту надо о снятии
/// сказать. Складываем его в отдельную очередь — из удалённой строки он
/// бы ничего не узнал.
async fn block_remove(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path((id, ip)): Path<(i64, String)>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    sqlx::query("DELETE FROM node_ip_blocks WHERE node_id = $1 AND ip = $2::inet")
        .bind(id)
        .bind(&ip)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO node_ip_unblocks (node_id, ip) VALUES ($1, $2::inet)")
        .bind(id)
        .bind(&ip)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Обнулить счётчики трафика ноды.
///
/// Трогаем только статистику самой ноды: лимиты клиентов считаются по их
/// собственным счётчикам, и обнулять их заодно значило бы раздать всем
/// безлимит одной кнопкой.
async fn node_reset_traffic(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM traffic_usage WHERE node_id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'node.reset_traffic', 'node', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "deleted_rows": res.rows_affected() })))
}

/// Перезапустить движок на ноде.
///
/// Панель ставит отметку времени, агент видит её при очередном опросе и
/// перезапускает Xray. Прямого канала до ноды у панели нет — агент ходит
/// сам, потому что у edge-серверов бывает нет белого адреса.
///
/// Нужно это там, где движок формально жив, но работает не так:
/// подхватил обновлённый сертификат, упёрся в исчерпанные соединения,
/// завис после смены сети. Раньше в таких случаях приходилось идти на
/// сервер руками.
async fn node_restart_engine(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query(
        "UPDATE nodes SET restart_requested_at = now()
          WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(&st.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'node.restart_engine', 'node', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// Обновить агента на всех нодах.
///
/// Агент — наш код, и держать парк на разных его сборках незачем: нода
/// со старым агентом не понимает новых команд панели и, например,
/// навсегда остаётся со старым движком.
///
/// Ставим отметку времени; агент видит её при опросе, скачивает свежую
/// сборку, проверяет, что она запускается, подменяет себя и выходит —
/// systemd поднимает его заново. Старый агент отметку игнорирует: его
/// обновляют командой `update-node.sh` на самой ноде, один раз.
async fn nodes_update_agents(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
) -> Result<Json<Value>> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO settings (key, value, updated_by, updated_at)
         VALUES ('nodes.agent_update_at', $1::text::jsonb, $2, now())
         ON CONFLICT (key) DO UPDATE
            SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(now_ms.to_string())
    .bind(admin.id)
    .execute(&st.pool)
    .await?;

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type)
         VALUES ('admin', $1, 'nodes.update_agents', 'node')",
    )
    .bind(admin.id)
    .execute(&st.pool)
    .await?;

    // Сколько нод отзовётся, заранее неизвестно: старые агенты отметку
    // не понимают. Отдаём общее число, чтобы панель не обещала лишнего.
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM nodes WHERE deleted_at IS NULL")
        .fetch_one(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true, "nodes": total })))
}

async fn build_install(st: &AppState, node_id: i64, secret: &str) -> Result<Value> {
    let row = sqlx::query("SELECT name, address FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;
    let name: String = row.get("name");

    let panel_url=install_panel_url(st).await?;
    let target:Option<String>=sqlx::query_scalar("SELECT value#>>'{}' FROM settings WHERE key='nodes.engine_version'").fetch_optional(&st.pool).await?.flatten();
    let target=target.filter(|v|!v.is_empty()).unwrap_or_else(||crate::xray_releases::INSTALL_DEFAULT.into());
    // A Docker image must be configured by the operator. Do not advertise an unpublished image.
    let image=std::env::var("NODE_IMAGE").ok().filter(|v|!v.trim().is_empty());
    let compose=image.map(|image|format!("services:\n  node:\n    image: {}\n    restart: unless-stopped\n    network_mode: host\n    environment:\n      PANEL_URL: {}\n      NODE_SECRET: {}\n      XRAY_LOCATION_ASSET: /usr/local/share/xray\n    volumes:\n      - node-config:/etc/sn-node\nvolumes:\n  node-config:\n",json!(image),json!(panel_url),json!(secret)));
    let systemd=format!("[Unit]\nDescription=STEALTHNET node agent\nAfter=network-online.target\nWants=network-online.target\n[Service]\nEnvironmentFile=/etc/sn-node/node.env\nEnvironment=ENGINE_BIN=/usr/local/bin/xray\nEnvironment=ENGINE_CONFIG=/etc/sn-node/config.json\nEnvironment=ENGINE_API=127.0.0.1:10085\nEnvironment=XRAY_LOCATION_ASSET=/usr/local/share/xray\nExecStart=/usr/local/bin/sn-node\nRestart=always\nRestartSec=5\nUMask=0077\n[Install]\nWantedBy=multi-user.target\n");
    let command="set -euo pipefail; command -v curl >/dev/null || { apt-get update -qq; apt-get install -y curl ca-certificates; }; curl -fsSL \"$PANEL_URL/install-node.sh\" | bash";
    let one_liner=format!("env PANEL_URL={} NODE_SECRET={} bash -c {}",shell_quote(&panel_url),shell_quote(secret),shell_quote(command));

    Ok(json!({
        "node_id": node_id,
        "name": name,
        // Адрес нужен окну установки: оно показывает готовую строку входа
        // по SSH. Без неё человеку с чистым сервером неоткуда узнать,
        // куда вообще вставлять команду.
        "address": row.get::<String, _>("address"),
        "panel_url": panel_url,
        "secret": secret,
        "compose": compose,
        "engine_version": target,
        "systemd": systemd,
        "env": format!("PANEL_URL={panel_url}\nNODE_SECRET={secret}\n"),
        "one_liner": one_liner,
    }))
}

fn shell_quote(value:&str)->String {format!("'{}'",value.replace('\'',"'\"'\"'"))}
async fn install_panel_url(st:&AppState)->Result<String>{
    let saved:Option<String>=sqlx::query_scalar("SELECT value#>>'{}' FROM settings WHERE key='panel.public_url'").fetch_optional(&st.pool).await?.flatten();
    let url=saved.filter(|u|!u.is_empty()).unwrap_or_else(||st.config.panel_url.clone());
    let parsed=reqwest::Url::parse(&url).map_err(|_|Error::bad("Укажите публичный адрес панели в настройках"))?;
    if !matches!(parsed.scheme(),"http"|"https") || parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() || parsed.query().is_some() || parsed.fragment().is_some() || url.chars().any(|c|c.is_whitespace()||matches!(c,'\''|'"'|'$'|'`'|'\\')) {return Err(Error::bad("Адрес панели должен быть HTTP(S)-ссылкой без логина, параметров и пробелов"));}
    Ok(url.trim_end_matches('/').to_string())
}
#[cfg(test)] mod install_tests {
    use super::*;
    #[test] fn addresses_exclude_shell_and_urls(){for s in ["192.0.2.1","vpn.example.com","2001:db8::1","[2001:db8::1]"]{assert!(valid_node_address(s));}for s in ["https://vpn.example.com","vpn.example.com:443","a;id","x/y","","-oProxyCommand=evil"]{assert!(!valid_node_address(s));}}
    #[test] fn commands_quote_shell_metacharacters(){assert_eq!(shell_quote("abc"),"'abc'");assert_eq!(shell_quote("a'b"),"'a'\"'\"'b'");assert_eq!(shell_quote("$(id)"),"'$(id)'");}
}

pub(crate) fn valid_node_address(address:&str)->bool {
    let ip=address.trim_start_matches('[').trim_end_matches(']');
    ip.parse::<std::net::IpAddr>().is_ok() || (!address.is_empty() && address.len()<=253 && address.split('.').all(|part|!part.is_empty() && part.len()<=63 && !part.starts_with('-') && !part.ends_with('-') && part.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')))
}

#[cfg(test)]
mod profile_assignment_tests {
    use super::*;

    #[test]
    fn profile_patch_distinguishes_omitted_null_and_selected() {
        let omitted: UpdateNode = serde_json::from_value(json!({"notify": false})).unwrap();
        let detached: UpdateNode = serde_json::from_value(json!({"profile_id": null})).unwrap();
        let assigned: UpdateNode = serde_json::from_value(json!({"profile_id": 42})).unwrap();
        assert_eq!(omitted.profile_id, None);
        assert_eq!(detached.profile_id, Some(None));
        assert_eq!(assigned.profile_id, Some(Some(42)));
        for invalid in [json!(""), json!("42"), json!([]), json!(true)] {
            assert!(serde_json::from_value::<UpdateNode>(json!({"profile_id": invalid})).is_err());
        }
    }

    #[test]
    fn node_can_be_created_before_a_profile_exists() {
        let node: CreateNode = serde_json::from_value(json!({
            "name": "test-node", "country_code": "NL", "address": "192.0.2.10",
            "profile_id": null, "inbound_tags": []
        })).unwrap();
        assert_eq!(node.profile_id, None);
        assert!(node.inbound_tags.unwrap().is_empty());
    }
}

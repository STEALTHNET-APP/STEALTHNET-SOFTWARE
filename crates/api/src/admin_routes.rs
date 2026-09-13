//! Системные разделы панели: внешние сквады, API-токены, безопасность.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/ext-squads", get(ext_list).post(ext_create))
        .route(
            "/api/ext-squads/{id}",
            axum::routing::patch(ext_update).delete(ext_delete),
        )
        .route("/api/ext-squads/{id}/rotate", post(ext_rotate))
        .route("/api/ext-squads/{id}/clients", post(ext_clients))
        .route("/api/audit", get(audit_list))
        .route("/api/tokens", get(tokens_list).post(token_create))
        .route("/api/tokens/{id}", axum::routing::delete(token_revoke))
        .route("/api/admin/password", post(change_password))
        .route("/api/admin/sessions", get(sessions_list).delete(sessions_revoke))
        .route("/api/admin/passkeys", get(passkeys_list))
        .route("/api/admin/totp", get(totp_status).post(totp_enable).delete(totp_disable))
        .route("/api/admin/totp/setup", post(totp_setup))
        .route("/api/whoami", get(whoami))
        // Публичный: потребитель приходит со своим токеном, без входа в панель.
        .route("/api/ext/hosts", get(ext_hosts_public))
}

/// С какого адреса пришёл администратор.
///
/// Нужен исключениям блокировщика: свой адрес набирают руками и
/// ошибаются, а ошибка здесь стоит потери доступа к серверу.
pub(crate) fn trusted_client_ip(headers: &HeaderMap, peer: std::net::IpAddr) -> std::net::IpAddr {
    let configured = std::env::var("TRUSTED_PROXY_IPS").unwrap_or_default();
    let trusted = peer.is_loopback() || configured.split(',').any(|s| s.trim().parse::<std::net::IpAddr>().ok() == Some(peer));
    if trusted {
        // Reverse proxies append their observed source on the right. Skip only
        // explicitly trusted hops; never trust an arbitrary client-supplied XFF.
        if let Some(chain) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            for hop in chain.split(',').rev() {
                let Ok(ip) = hop.trim().parse::<std::net::IpAddr>() else { return peer; };
                if !ip.is_loopback() && !configured.split(',').any(|s| s.trim().parse::<std::net::IpAddr>().ok() == Some(ip)) { return ip; }
            }
        }
    }
    peer
}

async fn whoami(
    _a: CurrentAdmin,
    headers: HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Result<Json<Value>> {
    let ip = trusted_client_ip(&headers, peer.ip()).to_string();
    Ok(Json(json!({ "ip": ip })))
}

/// Отдаёт хосты внешнего сквада его потребителю.
///
/// Без этого выпуск токена бессмысленен: его некому предъявлять.
/// Авторизация — сам токен, поэтому здесь же проверяются белый список
/// адресов и ограничение частоты.
async fn ext_hosts_public(
    State(st): State<AppState>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Result<Json<Value>> {
    let token = headers
        .get("x-squad-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;

    let row = sqlx::query(
        "SELECT id, allowed_ips::text[] AS allowed_ips, rate_limit_per_min,
                brand_name, support_url, logo_url
           FROM external_squads
          WHERE token_hash = $1 AND is_active",
    )
    .bind(sn_core::auth::token_hash(token))
    .fetch_optional(&st.pool)
    .await?
    .ok_or(Error::Unauthorized)?;

    let squad_id: i64 = row.get("id");

    // Адрес берём из заголовка обратного прокси, а сокет — только если
    // его нет: панель почти всегда стоит за Caddy или nginx, и без
    // этого в белый список пришлось бы вносить сам прокси.
    let ip = trusted_client_ip(&headers, peer.ip()).to_string();

    let allowed: bool = sqlx::query_scalar(
        "SELECT cardinality(allowed_ips) = 0 OR allowed_ips IS NULL OR EXISTS
         (SELECT 1 FROM unnest(allowed_ips) AS network WHERE $2::inet <<= network)
         FROM external_squads WHERE id = $1"
    ).bind(squad_id).bind(&ip).fetch_one(&st.pool).await?;
    if !allowed { return Err(Error::Forbidden); }

    // Serialize checks per squad so concurrent requests cannot exceed its quota.
    let mut tx = st.pool.begin().await?;
    sqlx::query("SELECT id FROM external_squads WHERE id=$1 FOR UPDATE")
        .bind(squad_id).fetch_one(&mut *tx).await?;
    if let Some(limit) = row.get::<Option<i32>, _>("rate_limit_per_min") {
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM external_squad_requests
              WHERE external_squad_id = $1 AND at >= now() - interval '1 minute'",
        ).bind(squad_id).fetch_one(&mut *tx).await?;
        if recent >= i64::from(limit) { return Err(Error::TooManyRequests); }
    }
    sqlx::query("INSERT INTO external_squad_requests (external_squad_id, ip) VALUES ($1, $2::inet)")
        .bind(squad_id).bind(&ip).execute(&mut *tx).await?;
    tx.commit().await?;

    let hosts = sqlx::query(
        "SELECT h.remark, h.address, h.port, h.security::text AS security,
                h.sni, h.fingerprint, h.alpn, h.path, h.public_key, h.short_id,
                i.protocol, COALESCE(i.network, 'tcp') AS network
           FROM external_squad_hosts eh
           JOIN hosts h    ON h.id = eh.host_id AND h.is_enabled
           JOIN inbounds i ON i.id = h.inbound_id
          WHERE eh.external_squad_id = $1
          ORDER BY h.sort_order, h.id",
    )
    .bind(squad_id)
    .fetch_all(&st.pool)
    .await?;

    // Отдаём вместе с витриной: потребитель поднимает свой сервис на
    // вашей инфраструктуре, и его клиенты должны видеть его марку.
    Ok(Json(json!({
        "brand": {
            "name": row.get::<Option<String>, _>("brand_name"),
            "support_url": row.get::<Option<String>, _>("support_url"),
            "logo_url": row.get::<Option<String>, _>("logo_url"),
        },
        "hosts": hosts
        .iter()
        .map(|r| json!({
            "remark": r.get::<String, _>("remark"),
            "address": r.get::<String, _>("address"),
            "port": r.get::<i32, _>("port"),
            "protocol": r.get::<String, _>("protocol"),
            "network": r.get::<String, _>("network"),
            "security": r.get::<String, _>("security"),
            "sni": r.get::<Option<String>, _>("sni"),
            "fingerprint": r.get::<Option<String>, _>("fingerprint"),
            "alpn": r.get::<Option<String>, _>("alpn"),
            "path": r.get::<Option<String>, _>("path"),
            "public_key": r.get::<Option<String>, _>("public_key"),
            "short_id": r.get::<Option<String>, _>("short_id"),
        }))
        .collect::<Vec<_>>(),
    })))
}

// ── Внешние сквады ───────────────────────────────────────────────────
//
// Отдают список хостов наружу — партнёру или другой панели. Доступ по
// токену, поэтому отзыв и лимит запросов здесь не украшение.

async fn ext_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        // inet[] приводим к text[]: адреса нужны панели строками,
        // а тип inet сам по себе она не разбирает.
        "SELECT e.id, e.name, e.rate_limit_per_min, e.allowed_ips::text[] AS allowed_ips,
                e.is_active, e.created_at, e.subscription_settings, e.template_overrides, e.host_overrides,
                (SELECT count(*) FROM clients c WHERE c.external_squad_id=e.id AND c.deleted_at IS NULL) AS members,
                ARRAY(SELECT host_id FROM external_squad_hosts h WHERE h.external_squad_id=e.id ORDER BY host_id) AS host_ids,
                e.brand_name, e.support_url, e.logo_url, e.page_url,
                (SELECT count(*) FROM external_squad_hosts h WHERE h.external_squad_id = e.id) AS host_count
           FROM external_squads e ORDER BY e.name",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "name": r.get::<String, _>("name"),
            "rate_limit_per_min": r.get::<Option<i32>, _>("rate_limit_per_min"),
            "allowed_ips": r.try_get::<Option<Vec<String>>, _>("allowed_ips").ok().flatten(),
            "is_active": r.get::<bool, _>("is_active"),
            "brand_name": r.get::<Option<String>, _>("brand_name"),
            "support_url": r.get::<Option<String>, _>("support_url"),
            "logo_url": r.get::<Option<String>, _>("logo_url"),
            "page_url": r.get::<Option<String>, _>("page_url"),
            "host_count": r.get::<i64, _>("host_count"),
            "host_ids":r.get::<Vec<i64>,_>("host_ids"),
            "members":r.get::<i64,_>("members"),
            "subscription_settings":r.get::<Value,_>("subscription_settings"),
            "template_overrides":r.get::<Value,_>("template_overrides"),
            "host_overrides":r.get::<Value,_>("host_overrides"),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct ExtBody {
    subscription_settings: Option<Value>,
    template_overrides: Option<Value>,
    host_overrides: Option<Value>,
    name: Option<String>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    rate_limit_per_min: Option<Option<i32>>,
    /// Белый список адресов. Пустой — без ограничения.
    allowed_ips: Option<Vec<String>>,
    is_active: Option<bool>,
    /// Хосты, которые увидит потребитель.
    host_ids: Option<Vec<i64>>,
    /// Витрина партнёра: под этим именем его клиенты видят сервис.
    brand_name: Option<String>,
    support_url: Option<String>,
    logo_url: Option<String>,
    page_url: Option<String>,
}

async fn ext_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<ExtBody>,
) -> Result<Json<Value>> {
    validate_ext(&st,&b).await?;
    if b.rate_limit_per_min.flatten().is_some_and(|n| n < 1) { return Err(Error::bad("лимит запросов должен быть больше нуля или пустым")); }
    let name = b.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::bad("нужно название сквада"));
    }

    let token = sn_core::auth::generate_token();
    let hash = sn_core::auth::token_hash(&token);

    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO external_squads
            (name, token_hash, rate_limit_per_min, allowed_ips,
             brand_name, support_url, logo_url, page_url)
         VALUES ($1, $2, $3, $4::text[]::inet[], $5, $6, $7, $8) RETURNING id",
    )
    .bind(name)
    .bind(&hash)
    .bind(b.rate_limit_per_min.flatten())
    .bind(b.allowed_ips.clone().unwrap_or_default())
    .bind(&b.brand_name)
    .bind(&b.support_url)
    .bind(&b.logo_url)
    .bind(&b.page_url)
    .fetch_one(&mut *tx)
    .await?;

    set_ext_hosts(&mut tx, id, b.host_ids.as_deref()).await?;
    sqlx::query("UPDATE external_squads SET subscription_settings=COALESCE($2,subscription_settings),template_overrides=COALESCE($3,template_overrides),host_overrides=COALESCE($4,host_overrides) WHERE id=$1")
        .bind(id).bind(&b.subscription_settings).bind(&b.template_overrides).bind(&b.host_overrides).execute(&mut *tx).await?;
    tx.commit().await?;

    // Токен виден один раз: в базе только хеш.
    Ok(Json(json!({ "id": id, "token": token })))
}

async fn set_ext_hosts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    squad_id: i64,
    hosts: Option<&[i64]>,
) -> Result<()> {
    let Some(hosts) = hosts else { return Ok(()) };
    sqlx::query("DELETE FROM external_squad_hosts WHERE external_squad_id = $1")
        .bind(squad_id)
        .execute(&mut **tx)
        .await?;
    for h in hosts {
        sqlx::query(
            "INSERT INTO external_squad_hosts (external_squad_id, host_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(squad_id)
        .bind(h)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn ext_update(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ExtBody>,
) -> Result<Json<Value>> {
    validate_ext(&st,&b).await?;
    if b.rate_limit_per_min.flatten().is_some_and(|n| n < 1) { return Err(Error::bad("лимит запросов должен быть больше нуля или пустым")); }
    let mut tx = st.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE external_squads SET
            name = COALESCE($2, name),
            rate_limit_per_min = CASE WHEN $10 THEN $3 ELSE rate_limit_per_min END,
            allowed_ips = COALESCE($4::text[]::inet[], allowed_ips),
            is_active = COALESCE($5, is_active),
            brand_name = COALESCE($6, brand_name),
            support_url = COALESCE($7, support_url),
            logo_url = COALESCE($8, logo_url),
            page_url = COALESCE($9, page_url)
          WHERE id = $1",
    )
    .bind(id)
    .bind(b.name.as_deref().map(str::trim))
    .bind(b.rate_limit_per_min.flatten())
    .bind(b.allowed_ips.as_deref())
    .bind(b.is_active)
    .bind(&b.brand_name)
    .bind(&b.support_url)
    .bind(&b.logo_url)
    .bind(&b.page_url)
    .bind(b.rate_limit_per_min.is_some())
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    set_ext_hosts(&mut tx, id, b.host_ids.as_deref()).await?;
    sqlx::query("UPDATE external_squads SET subscription_settings=COALESCE($2,subscription_settings),template_overrides=COALESCE($3,template_overrides),host_overrides=COALESCE($4,host_overrides) WHERE id=$1")
        .bind(id).bind(&b.subscription_settings).bind(&b.template_overrides).bind(&b.host_overrides).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn ext_rotate(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let token = sn_core::auth::generate_token();
    let hash = sn_core::auth::token_hash(&token);

    let res = sqlx::query("UPDATE external_squads SET token_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(&hash)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "token": token })))
}

async fn ext_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM external_squads WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

// ── API-токены панели ────────────────────────────────────────────────

/// Журнал действий администраторов.
///
/// Пишется он давно, но посмотреть его в панели было негде: разбирать
/// «кто это поменял» приходилось запросом к базе. Когда администраторов
/// больше одного, это первое, куда смотрят.
///
/// Сразу подставляем имя администратора и, где есть, название объекта:
/// строка «node.update 7» ничего не говорит, а «Иван изменил ноду
/// Germani» — говорит.
async fn audit_list(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> Result<Json<Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    let rows = sqlx::query(
        "SELECT a.id, a.actor_kind, a.action, a.entity_type, a.entity_id,
                a.payload, host(a.ip) AS ip, a.created_at,
                adm.username AS actor,
                CASE a.entity_type
                    WHEN 'node'    THEN (SELECT name  FROM nodes           WHERE id = a.entity_id)
                    WHEN 'client'  THEN (SELECT username FROM clients      WHERE id = a.entity_id)
                    WHEN 'tariff'  THEN (SELECT title FROM tariffs         WHERE id = a.entity_id)
                    WHEN 'squad'   THEN (SELECT name  FROM squads          WHERE id = a.entity_id)
                    WHEN 'profile' THEN (SELECT name  FROM config_profiles WHERE id = a.entity_id)
                END AS entity_name
           FROM audit_log a
           LEFT JOIN admins adm ON adm.id = a.actor_id
          WHERE ($1::text IS NULL OR a.action LIKE $1 || '%')
            AND ($2::bigint IS NULL OR a.actor_id = $2)
          ORDER BY a.id DESC
          LIMIT $3",
    )
    .bind(q.action.as_deref())
    .bind(q.actor_id)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":          r.get::<i64, _>("id"),
                "actor_kind":  r.get::<String, _>("actor_kind"),
                // Администратора могли удалить — тогда остаётся только
                // род деятеля. Придумывать имя за него не станем.
                "actor":       r.get::<Option<String>, _>("actor"),
                "action":      r.get::<String, _>("action"),
                "entity_type": r.get::<Option<String>, _>("entity_type"),
                "entity_id":   r.get::<Option<i64>, _>("entity_id"),
                "entity_name": r.get::<Option<String>, _>("entity_name"),
                "payload":     r.get::<Value, _>("payload"),
                "ip":          r.get::<Option<String>, _>("ip"),
                "created_at":  r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "items": items })))
}

#[derive(serde::Deserialize)]
struct AuditQuery {
    /// Префикс действия: «node» покажет все node.*.
    action: Option<String>,
    actor_id: Option<i64>,
    limit: Option<i64>,
}

async fn tokens_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT id, name, prefix, scopes, last_used_at, created_at
           FROM api_tokens WHERE revoked_at IS NULL ORDER BY created_at DESC",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "name": r.get::<String, _>("name"),
            "prefix": r.get::<String, _>("prefix"),
            "scopes": r.try_get::<Option<Vec<String>>, _>("scopes").ok().flatten(),
            "last_used_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at")
                .ok().flatten().map(|d| d.to_rfc3339()),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct TokenBody {
    name: Option<String>,
    scopes: Option<Vec<String>>,
}

async fn token_create(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<TokenBody>,
) -> Result<Json<Value>> {
    let name = b.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::bad("нужно название токена"));
    }

    let token = sn_core::auth::generate_token();
    let hash = sn_core::auth::token_hash(&token);
    let prefix: String = token.chars().take(8).collect();

    let mut scopes = b.scopes.clone().unwrap_or_else(|| vec!["read".into()]);
    if scopes.is_empty() { scopes.push("read".into()); }
    if scopes.iter().any(|scope| !matches!(scope.as_str(), "read" | "write")) {
        return Err(Error::bad("доступные права токена: read, write"));
    }
    scopes.sort(); scopes.dedup();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO api_tokens (name, prefix, token_hash, scopes, created_by)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(name)
    .bind(&prefix)
    .bind(&hash)
    .bind(&scopes)
    .bind(admin.id)
    .fetch_one(&st.pool)
    .await?;

    Ok(Json(json!({ "id": id, "token": token, "prefix": prefix, "scopes": scopes })))
}

async fn token_revoke(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Не удаляем строку: по журналу разбирают, чем пользовались и когда.
    let res = sqlx::query("UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

// ── Безопасность администратора ──────────────────────────────────────

#[derive(Deserialize)]
struct PasswordBody {
    current: String,
    new_password: String,
}

/// Хеш токена текущей сессии.
///
/// Нужен, чтобы закрыть все сессии, кроме той, из которой пришёл запрос:
/// иначе администратор выкидывает сам себя и не может войти обратно, если
/// как раз меняет пароль.
fn current_session_hash(headers: &HeaderMap) -> Option<Vec<u8>> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    Some(sn_core::auth::token_hash(raw))
}

/// Смена собственного пароля.
///
/// Требуем текущий пароль: перехваченная сессия иначе позволила бы
/// сменить пароль и запереть владельца снаружи.
async fn change_password(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<PasswordBody>,
) -> Result<Json<Value>> {
    let session = current_session_hash(&headers).unwrap_or_default();
    if b.new_password.chars().count() < 10 {
        return Err(Error::bad("пароль короче 10 символов"));
    }

    let hash: String = sqlx::query_scalar("SELECT password_hash FROM admins WHERE id = $1")
        .bind(admin.id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;

    if !sn_core::auth::verify_password(&b.current, &hash) {
        return Err(Error::bad("текущий пароль неверен"));
    }

    let new_hash = sn_core::auth::hash_password(&b.new_password)?;
    let mut tx = st.pool.begin().await?;

    let changed = sqlx::query("UPDATE admins SET password_hash = $2, updated_at = now() WHERE id = $1 AND password_hash = $3")
        .bind(admin.id)
        .bind(&new_hash)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    if changed.rows_affected() != 1 {
        return Err(Error::bad("пароль уже изменён — войдите заново"));
    }

    // Все прочие сессии закрываем: если пароль меняют из-за утечки,
    // оставить чужую сессию живой значит не решить проблему.
    sqlx::query("DELETE FROM admin_sessions WHERE admin_id = $1 AND token_hash <> $2")
        .bind(admin.id)
        .bind(&session)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'admin.password_change', 'admin', $1)",
    )
    .bind(admin.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn sessions_list(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    let session = current_session_hash(&headers).unwrap_or_default();
    let rows = sqlx::query(
        "SELECT id, ip::text AS ip, user_agent, created_at, expires_at, token_hash
           FROM admin_sessions WHERE admin_id = $1 ORDER BY created_at DESC",
    )
    .bind(admin.id)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "ip": r.try_get::<Option<String>, _>("ip").ok().flatten(),
            "user_agent": r.try_get::<Option<String>, _>("user_agent").ok().flatten(),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            "expires_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at")
                .ok().flatten().map(|d| d.to_rfc3339()),
            // Текущую сессию помечаем: закрыть её кнопкой «все остальные»
            // человек не должен, иначе он выкинет сам себя.
            "is_current": r.try_get::<Vec<u8>, _>("token_hash").map(|h| h == session).unwrap_or(false),
        }))
        .collect::<Vec<_>>())))
}

async fn sessions_revoke(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    let session = current_session_hash(&headers).unwrap_or_default();
    let res = sqlx::query("DELETE FROM admin_sessions WHERE admin_id = $1 AND token_hash <> $2")
        .bind(admin.id)
        .bind(&session)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "closed": res.rows_affected() })))
}

/// Включена ли двухфакторная у текущего администратора.
async fn totp_status(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
) -> Result<Json<Value>> {
    let secret: Option<String> = sqlx::query_scalar("SELECT totp_secret FROM admins WHERE id = $1")
        .bind(admin.id)
        .fetch_optional(&st.pool)
        .await?
        .flatten();
    Ok(Json(json!({ "enabled": secret.map(|s| !s.is_empty()).unwrap_or(false) })))
}

/// Новый секрет для подключения приложения-аутентификатора.
///
/// В базу его пока не пишем: пока человек не подтвердил кодом, что
/// приложение действительно настроено, включать двухфакторную нельзя —
/// иначе он запрёт себя снаружи.
async fn totp_setup(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
) -> Result<Json<Value>> {
    let secret = sn_core::totp::generate_secret();
    let brand: Option<String> =
        sqlx::query_scalar("SELECT value #>> '{}' FROM settings WHERE key = 'brand.name'")
            .fetch_optional(&st.pool)
            .await?
            .flatten();

    Ok(Json(json!({
        "secret": secret.clone(),
        "uri": sn_core::totp::provisioning_uri(
            &secret,
            &admin.username,
            brand.as_deref().filter(|b| !b.is_empty()).unwrap_or("STEALTHNET"),
        ),
    })))
}

#[derive(Deserialize)]
struct TotpBody {
    secret: Option<String>,
    code: String,
}

async fn totp_enable(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<TotpBody>,
) -> Result<Json<Value>> {
    let secret = b
        .secret
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::bad("нет секрета — начните настройку заново"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if !sn_core::totp::verify(secret, &b.code, now) {
        return Err(Error::bad("код не подошёл — проверьте время на телефоне"));
    }

    let changed = sqlx::query("UPDATE admins SET totp_secret = $2, totp_last_used_step = NULL, updated_at = now() WHERE id = $1 AND COALESCE(totp_secret, '') = ''")
        .bind(admin.id)
        .bind(secret)
        .execute(&st.pool)
        .await?;
    if changed.rows_affected() != 1 {
        return Err(Error::bad("двухфакторная уже включена — сначала отключите её действующим кодом"));
    }

    Ok(Json(json!({ "ok": true })))
}

/// Выключение. Требуем действующий код: иначе перехваченная сессия
/// снимала бы вторую ступень защиты одним запросом.
async fn totp_disable(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<TotpBody>,
) -> Result<Json<Value>> {
    let secret: Option<String> = sqlx::query_scalar("SELECT totp_secret FROM admins WHERE id = $1")
        .bind(admin.id)
        .fetch_optional(&st.pool)
        .await?
        .flatten();

    let Some(secret) = secret.filter(|s| !s.is_empty()) else {
        return Err(Error::bad("двухфакторная и так выключена"));
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if !sn_core::totp::verify(&secret, &b.code, now) {
        return Err(Error::bad("неверный код"));
    }

    let changed = sqlx::query("UPDATE admins SET totp_secret = NULL, totp_last_used_step = NULL, updated_at = now() WHERE id = $1 AND totp_secret = $2")
        .bind(admin.id)
        .bind(&secret)
        .execute(&st.pool)
        .await?;
    if changed.rows_affected() != 1 {
        return Err(Error::bad("настройки двухфакторной изменились — повторите запрос"));
    }

    Ok(Json(json!({ "ok": true })))
}

/// Ключи входа без пароля.
///
/// Зарегистрированные WebAuthn-ключи текущего администратора.
async fn passkeys_list(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT id, label, created_at FROM admin_passkeys WHERE admin_id = $1",
    )
    .bind(admin.id)
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!({
        // Вход по ключу реализован (crates/api/src/passkeys.rs). Раньше
        // здесь стояло жёсткое false, и настройки писали «не
        // поддерживаются» при работающей возможности.
        "supported": true,
        "items": rows
            .iter()
            .map(|r| json!({
                "id": r.get::<i64, _>("id"),
                "name": r.get::<Option<String>, _>("label").unwrap_or_else(|| "ключ".into()),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            }))
            .collect::<Vec<_>>(),
    })))
}

async fn validate_ext(st:&AppState,b:&ExtBody)->Result<()> {
    if let Some(v)=&b.subscription_settings{sn_sub::overrides::validate_settings(v).map_err(Error::bad)?;}
    if let Some(v)=&b.template_overrides{sn_sub::overrides::validate_templates(&st.pool,v).await?;}
    if let Some(v)=&b.host_overrides{sn_sub::overrides::validate_host(v,true).map_err(Error::bad)?;}
    if b.name.as_deref().is_some_and(|n|n.trim().is_empty()||n.chars().count()>100){return Err(Error::bad("Название: от 1 до 100 символов"))}
    Ok(())
}

#[derive(Deserialize)]
struct ExtClients {action:String,client_ids:Option<Vec<i64>>}
async fn ext_clients(CurrentAdmin(admin):CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>,Json(b):Json<ExtClients>)->Result<Json<Value>>{
    if !matches!(b.action.as_str(),"assign"|"remove"){return Err(Error::bad("Неизвестное действие"))}
    let mut tx=st.pool.begin().await?;
    let active:Option<bool>=sqlx::query_scalar("SELECT is_active FROM external_squads WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    let active=active.ok_or(Error::NotFound)?;
    if b.action=="assign"&&!active{return Err(Error::bad("Сначала включите внешний сквад"))}
    if let Some(ids)=&b.client_ids{if ids.len()>10000{return Err(Error::bad("Не более 10000 клиентов за запрос"))}}
    let result=if b.action=="assign"{
        sqlx::query("UPDATE clients SET external_squad_id=$1 WHERE deleted_at IS NULL AND ($2::bigint[] IS NULL OR id=ANY($2))").bind(id).bind(&b.client_ids).execute(&mut *tx).await?
    }else{
        sqlx::query("UPDATE clients SET external_squad_id=NULL WHERE deleted_at IS NULL AND external_squad_id=$1 AND ($2::bigint[] IS NULL OR id=ANY($2))").bind(id).bind(&b.client_ids).execute(&mut *tx).await?
    };
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id,payload) VALUES ('admin',$1,'ext_squad.clients','external_squad',$2,$3)").bind(admin.id).bind(id).bind(json!({"action":b.action,"count":result.rows_affected()})).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(json!({"ok":true,"affected":result.rows_affected()})))
}

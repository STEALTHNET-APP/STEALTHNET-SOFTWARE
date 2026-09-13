//! Управление сервисом подписок из панели.
//!
//! Сабку обычно держат на отдельном домене и отдельной машине: если панель
//! заблокируют, клиенты продолжат обновлять конфиги. У такой машины нет
//! доступа к базе — она ходит в API панели по служебному токену.
//!
//! Здесь этот токен выпускают, меняют и получают готовую установку. Схема
//! двусторонняя: в панели указывают публичный адрес сабки (из него собираются
//! ссылки клиентам), на сервере сабки — адрес панели и токен.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn sub_service_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sub-service", get(status).patch(update))
        .route("/api/sub-service/token", post(issue_token))
}

/// Публичный адрес сабки: сначала настройка из панели, потом переменная
/// окружения. Настройку правит администратор, не пересобирая сервисы.
pub async fn public_sub_url(st: &AppState) -> String {
    let from_db: Option<String> = sqlx::query_scalar(
        "SELECT value #>> '{}' FROM settings WHERE key = 'subscription.public_url'",
    )
    .fetch_optional(&st.pool)
    .await
    .ok()
    .flatten()
    .flatten();

    from_db
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| st.config.sub_public_url.clone())
}

/// Публичный адрес панели. Нужен и passkey: ключ привязывается к домену.
pub async fn panel_public_url_pub(st: &AppState) -> String {
    panel_public_url(st).await
}

async fn panel_public_url(st: &AppState) -> String {
    let from_db: Option<String> =
        sqlx::query_scalar("SELECT value #>> '{}' FROM settings WHERE key = 'panel.public_url'")
            .fetch_optional(&st.pool)
            .await
            .ok()
            .flatten()
            .flatten();

    from_db
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| st.config.panel_url.clone())
}

async fn status(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let token = sqlx::query(
        "SELECT prefix, created_at, last_used_at
           FROM service_tokens
          WHERE kind = 'sub' AND revoked_at IS NULL",
    )
    .fetch_optional(&st.pool)
    .await?
    .map(|r| {
        json!({
            "prefix": r.get::<String, _>("prefix"),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            "last_used_at": r
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at")
                .ok()
                .flatten()
                .map(|d| d.to_rfc3339()),
        })
    });

    let sub_url = public_sub_url(&st).await;
    let panel_url = panel_public_url(&st).await;

    Ok(Json(json!({
        "sub_public_url": sub_url,
        "panel_public_url": panel_url,
        "token": token,
        // Токен уже выпущен — показать его повторно нельзя, у нас только хеш.
        // Поэтому в установке оставляем плейсхолдер: человек подставит тот
        // токен, который сохранил при выпуске, либо выпустит новый.
        "install": build_install(&panel_url, &sub_url, "ВАШ_ТОКЕН"),
        // Переменная окружения ещё работает — сервисы, настроенные до
        // появления этой страницы, продолжают жить без изменений.
        "env_token_configured": st.config.sub_service_token.is_some(),
    })))
}

#[derive(Deserialize)]
struct UpdateBody {
    sub_public_url: Option<String>,
    /// Адрес, по которому панель видна снаружи. По нему к ней обращаются
    /// сабка и агенты нод — localhost здесь означает, что установочные
    /// инструкции нерабочие, хотя выглядят правильно.
    panel_public_url: Option<String>,
}

async fn update(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Value>> {
    let values = [
        ("subscription.public_url", body.sub_public_url),
        ("panel.public_url", body.panel_public_url),
    ];
    // Validate both first: an invalid second field must not leave a partial update.
    for (_, value) in &values {
        if let Some(url) = value {
            validate_public_origin(url)?;
        }
    }
    let sub = match values[0].1.as_deref() {
        Some(v) if v.trim().is_empty() => st.config.sub_public_url.clone(),
        Some(v) => v.to_string(),
        None => public_sub_url(&st).await,
    };
    let panel = match values[1].1.as_deref() {
        Some(v) if v.trim().is_empty() => st.config.panel_url.clone(),
        Some(v) => v.to_string(),
        None => panel_public_url(&st).await,
    };
    if let (Ok(sub), Ok(panel)) = (
        reqwest::Url::parse(sub.trim()),
        reqwest::Url::parse(panel.trim()),
    ) {
        if sub.host_str() == panel.host_str() {
            return Err(Error::bad("Для панели и подписки нужны разные домены"));
        }
    }
    let mut tx = st.pool.begin().await?;
    for (key, value) in values {
        let Some(url) = value else { continue };
        sqlx::query("INSERT INTO settings (key,value,updated_by,updated_at) VALUES ($1,$2,$3,now()) ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value,updated_by=EXCLUDED.updated_by,updated_at=now()")
          .bind(key).bind(Value::String(url.trim().trim_end_matches('/').to_string())).bind(admin.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Выпускает новый токен и отзывает предыдущий.
///
/// Отзыв делаем в той же транзакции: частичный уникальный индекс не допускает
/// двух действующих токенов одного вида, и без отзыва вставка просто упадёт.
/// Это осознанно — два живых токена означают, что один из них потеряли.
async fn issue_token(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
) -> Result<Json<Value>> {
    let token = sn_core::auth::generate_token();
    let hash = sn_core::auth::token_hash(&token);
    let prefix: String = token.chars().take(8).collect();

    let mut tx = st.pool.begin().await?;

    sqlx::query(
        "UPDATE service_tokens SET revoked_at = now()
          WHERE kind = 'sub' AND revoked_at IS NULL",
    )
    .execute(&mut *tx)
    .await?;

    let token_id: i64 = sqlx::query_scalar(
        "INSERT INTO service_tokens (kind, name, token_hash, prefix, created_by)
         VALUES ('sub', 'Сервис подписок', $1, $2, $3)
         RETURNING id",
    )
    .bind(&hash)
    .bind(&prefix)
    .bind(admin.id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'sub_service.issue_token', 'service_token', $2)",
    )
    .bind(admin.id)
    .bind(token_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let panel_url = panel_public_url(&st).await;
    let sub_url = public_sub_url(&st).await;

    Ok(Json(json!({
        // Единственный раз, когда токен виден целиком: в базе только хеш.
        "token": token,
        "prefix": prefix,
        "install": build_install(&panel_url, &sub_url, &token),
    })))
}

fn validate_public_origin(input: &str) -> Result<()> {
    if input.trim().is_empty() {
        return Ok(());
    } // clearing restores environment defaults
    let u = reqwest::Url::parse(input.trim())
        .map_err(|_| Error::bad("Укажите полный HTTPS-адрес домена"))?;
    let host = u.host_str().unwrap_or_default();
    if !matches!(u.scheme(), "http" | "https")
        || host.is_empty()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || !matches!(u.path(), "" | "/")
        || host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback() || ip.is_unspecified())
            .unwrap_or(false)
        || host == "[::1]"
        || input.chars().any(|c| c.is_whitespace() && c != ' ')
        || input.trim().contains(' ')
    {
        return Err(Error::bad("Нужен внешний HTTP(S)-адрес без пути, параметров, логина и пароля; localhost недоступен клиентам"));
    }
    Ok(())
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}
fn build_install(panel_url: &str, sub_url: &str, token: &str) -> Value {
    let panel = panel_url.trim_end_matches('/');
    let sub = sub_url.trim_end_matches('/');
    let caddy=format!("# Добавьте блок в существующий /etc/caddy/Caddyfile.\n{sub} {{\n\tencode zstd gzip\n\treverse_proxy 127.0.0.1:8081\n\theader -Server\n}}\n# Проверка: caddy validate --config /etc/caddy/Caddyfile\n# Применение: systemctl reload caddy\n");
    let command=format!("env PANEL_URL={} SUB_PUBLIC_URL={} SUB_SERVICE_TOKEN={} bash -c 'set -euo pipefail; command -v curl >/dev/null || {{ apt-get update -qq; apt-get install -y curl ca-certificates; }}; curl -fsSL \"$PANEL_URL/install-sub.sh\" | bash'",quote(panel),quote(sub),quote(token));
    let env=format!("SUB_MODE=api\nPANEL_URL={panel}\nSUB_PUBLIC_URL={sub}\nSUB_SERVICE_TOKEN={token}\nSUB_BIND=127.0.0.1:8081\nRUST_LOG=sn_sub=info\n");
    let systemd="[Unit]\nDescription=STEALTHNET subscription service\nAfter=network-online.target\nWants=network-online.target\n[Service]\nEnvironmentFile=/etc/sn-sub/env\nExecStart=/usr/local/bin/sn-sub\nRestart=always\nRestartSec=5\nDynamicUser=yes\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=yes\nPrivateTmp=yes\n[Install]\nWantedBy=multi-user.target\n";
    let image = std::env::var("SUB_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let compose=image.map(|image|format!("services:\n  sub:\n    image: {}\n    command: sn-sub\n    restart: unless-stopped\n    ports: [\"127.0.0.1:8081:8081\"]\n    environment:\n      SUB_MODE: api\n      SUB_BIND: 0.0.0.0:8081\n      PANEL_URL: {}\n      SUB_PUBLIC_URL: {}\n      SUB_SERVICE_TOKEN: {}\n",json!(image),json!(panel),json!(sub),json!(token)));
    json!({"panel_url":panel,"sub_public_url":sub,"same_host":{
        "needs_token":false,"compose":"# В каталоге исходников панели, где находится docker-compose.yml:\ndocker compose up -d sub\ndocker compose exec sub curl -fsS http://127.0.0.1:8081/ready\n# Для Caddy в контейнере upstream: sub:8081 (уже задан в deploy/Caddyfile).",
        "systemd":"# При установке панели через install.sh сервис уже установлен.\nsystemctl status sn-sub --no-pager\ncurl -fsS http://127.0.0.1:8081/ready\n# Если служба выключена:\nsystemctl enable --now sn-sub\n# Журнал: journalctl -u sn-sub -n 50 --no-pager",
        "caddy":caddy,"one_liner":"# Повторная установка не нужна: sn-sub входит в установку панели."
    },"separate":{"needs_token":true,"compose":compose,"systemd":systemd,"env":env,"caddy":caddy,"one_liner":command}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origins_reject_paths_credentials_and_loopback() {
        for bad in [
            "https://u:p@example.com",
            "https://sub.example.com/path",
            "https://sub.example.com?x=1",
            "https://sub.example.com#x",
            "http://localhost",
            "http://127.0.0.1",
            "http://[::1]",
            "ftp://example.com",
            "https://sub.example.com\nOTHER=value",
        ] {
            assert!(validate_public_origin(bad).is_err(), "{bad}");
        }
        for ok in [
            "https://sub.example.com",
            "https://sub.example.com/",
            "https://sub.example.com:8443",
            "",
        ] {
            assert!(validate_public_origin(ok).is_ok(), "{ok}");
        }
    }
    #[test]
    fn placement_uses_actual_modes_and_private_secret_file() {
        let v = build_install(
            "https://panel.example.com",
            "https://sub.example.com",
            "test-token",
        );
        assert!(!v["same_host"]["needs_token"].as_bool().unwrap());
        assert!(v["same_host"]["systemd"]
            .as_str()
            .unwrap()
            .contains("/ready"));
        assert!(!v["separate"]["systemd"]
            .as_str()
            .unwrap()
            .contains("test-token"));
        assert!(v["separate"]["env"]
            .as_str()
            .unwrap()
            .contains("SUB_MODE=api"));
        assert!(v["separate"]["one_liner"]
            .as_str()
            .unwrap()
            .contains("set -euo pipefail"));
        assert!(v["separate"]["caddy"]
            .as_str()
            .unwrap()
            .contains("127.0.0.1:8081"));
    }
}

/// The browser cannot assess a separate subscription domain without CORS.
/// Probe its readiness from the panel and require the service's actual payload.
pub async fn public_health(State(st):State<AppState>,_a:CurrentAdmin)->Result<Json<Value>>{
    let url=public_sub_url(&st).await;
    if url.trim().is_empty(){return Ok(Json(json!({"status":"unconfigured"})));}
    let http=reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|Error::Internal(e.to_string()))?;
    let (ready,status)=match http.get(format!("{}/ready",url.trim_end_matches('/'))).send().await{
        Ok(r)=>{let status=r.status();let ready=status.is_success()&&r.json::<Value>().await.ok().is_some_and(|v|v["status"]=="ready");(ready,Some(status.as_u16()))},
        Err(_)=>(false,None)
    };
    Ok(Json(json!({"status":if ready{"ready"}else{"unavailable"},"http_status":status,"checked_at":chrono::Utc::now()})))
}

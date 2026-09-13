//! Bounds unauthenticated login work before password hashing or passkey allocation.
use axum::http::HeaderMap;
use sn_core::{Error, Result};
use std::net::IpAddr;

use crate::state::AppState;

pub async fn response_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    response
}

pub async fn auth_attempt(
    st: &AppState,
    headers: &HeaderMap,
    peer: IpAddr,
    account: Option<&str>,
) -> Result<()> {
    let ip = crate::admin_routes::trusted_client_ip(headers, peer);
    // Shared between password and passkey endpoints: switching endpoints must not
    // bypass the peer limit. Account limits also bound distributed guessing.
    consume(st, &format!("ip:{ip}"), 30, 60).await?;
    if let Some(name) = account {
        consume(st, &format!("account:{}", name.trim().to_lowercase()), 30, 300).await?;
    }
    Ok(())
}

pub async fn webhook_attempt(st: &AppState, headers: &HeaderMap, peer: IpAddr) -> Result<()> {
    let ip = crate::admin_routes::trusted_client_ip(headers, peer);
    consume(st, &format!("payment-webhook:{ip}"), 300, 60).await
}

async fn consume(st: &AppState, identity: &str, limit: i32, seconds: i64) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let count: i32 = sqlx::query_scalar(
        "INSERT INTO admin_auth_limits(bucket,window_start,hits) VALUES($1,$2,1)
         ON CONFLICT(bucket,window_start) DO UPDATE
            SET hits=LEAST(admin_auth_limits.hits+1,1000000) RETURNING hits",
    )
    .bind(sn_core::auth::token_hash(identity))
    .bind(now / seconds * seconds)
    .fetch_one(&st.pool)
    .await?;
    // Indexed expiry keeps anonymous attempts from growing into an indefinite log.
    sqlx::query("DELETE FROM admin_auth_limits WHERE window_start<$1")
        .bind(now - 3600).execute(&st.pool).await?;
    if count > limit { return Err(Error::TooManyRequests); }
    Ok(())
}

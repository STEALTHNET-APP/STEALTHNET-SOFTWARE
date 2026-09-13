//! Personal profile and owner-only staff management. No public registration.
use crate::state::{AppState, CurrentAdmin};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sn_core::{auth, Error, Result};
use sqlx::Row;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/profile", get(profile).patch(profile_update))
        .route("/api/team", get(list).post(create))
        .route("/api/team/{id}", axum::routing::patch(update))
        .route("/api/team/{id}/password", post(reset_password))
        .route("/api/team/{id}/sessions", axum::routing::delete(revoke))
}

fn username(value: &str) -> Result<&str> {
    let value = value.trim();
    if !(3..=64).contains(&value.len())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.@-".contains(&b))
    {
        return Err(Error::bad("Логин: 3–64 латинских символа, цифры, . _ @ -"));
    }
    Ok(value)
}
fn password(value: &str) -> Result<()> {
    if !(12..=128).contains(&value.chars().count()) {
        return Err(Error::bad("Пароль: 12–128 символов"));
    }
    Ok(())
}
fn email(value: Option<&str>) -> Result<Option<&str>> {
    let value = value.map(str::trim).filter(|s| !s.is_empty());
    if value
        .is_some_and(|s| s.len() > 254 || !s.contains('@') || s.chars().any(char::is_whitespace))
    {
        return Err(Error::bad("Проверьте адрес электронной почты"));
    }
    Ok(value)
}
fn view(r: &sqlx::postgres::PgRow) -> Value {
    json!({"id":r.get::<i64,_>("id"),"username":r.get::<String,_>("username"),
        "email":r.get::<Option<String>,_>("email"),"role":r.get::<String,_>("role"),
        "is_active":r.get::<bool,_>("is_active"),"totp_enabled":r.get::<bool,_>("totp_enabled"),
        "created_at":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
        "last_login_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("last_login_at")})
}
const FIELDS: &str = "id,username,email,role,is_active,totp_secret IS NOT NULL AS totp_enabled,created_at,last_login_at";

async fn profile(CurrentAdmin(a): CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let r = sqlx::query(&format!("SELECT {FIELDS} FROM admins WHERE id=$1"))
        .bind(a.id)
        .fetch_one(&st.pool)
        .await?;
    Ok(Json(view(&r)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileBody {
    username: String,
    email: Option<String>,
    current_password: String,
}
async fn profile_update(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<ProfileBody>,
) -> Result<Json<Value>> {
    let name = username(&b.username)?;
    let email = email(b.email.as_deref())?;
    if b.current_password.len() > 1024 {
        return Err(Error::bad("Пароль слишком длинный"));
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM admins WHERE id=$1")
        .bind(a.id)
        .fetch_one(&st.pool)
        .await?;
    let (candidate, stored) = (b.current_password, hash.clone());
    if !tokio::task::spawn_blocking(move || auth::verify_password(&candidate, &stored))
        .await
        .map_err(|_| Error::Internal("password task failed".into()))?
    {
        return Err(Error::bad("Неверный текущий пароль"));
    }
    let mut tx = st.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE admins SET username=$2,email=$3 WHERE id=$1 AND password_hash=$4 AND is_active",
    )
    .bind(a.id)
    .bind(name)
    .bind(email)
    .bind(hash)
    .execute(&mut *tx)
    .await?;
    if n.rows_affected() != 1 {
        return Err(Error::Conflict("Аккаунт изменён. Войдите заново".into()));
    }
    audit(&mut tx, a.id, a.id, "admin.profile", json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
async fn list(CurrentAdmin(a): CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    if a.role != "owner" {
        return Err(Error::Forbidden);
    }
    let rows = sqlx::query(&format!(
        "SELECT {FIELDS} FROM admins ORDER BY created_at,id"
    ))
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!(rows.iter().map(view).collect::<Vec<_>>())))
}
// Lock all staff rows in a stable order: concurrent owners cannot remove the
// final owner, and a just-revoked owner cannot use an already-started request.
async fn owner_lock(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> Result<()> {
    let rows = sqlx::query("SELECT id,role,is_active FROM admins ORDER BY id FOR UPDATE")
        .fetch_all(&mut **tx)
        .await?;
    if !rows.iter().any(|r| {
        r.get::<i64, _>("id") == id
            && r.get::<String, _>("role") == "owner"
            && r.get::<bool, _>("is_active")
    }) {
        return Err(Error::Forbidden);
    }
    Ok(())
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: i64,
    id: i64,
    action: &str,
    payload: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id,payload) VALUES('admin',$1,$2,'admin',$3,$4)").bind(actor).bind(action).bind(id).bind(payload).execute(&mut **tx).await?;
    Ok(())
}
async fn credentials_revoke(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM admin_sessions WHERE admin_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "UPDATE api_tokens SET revoked_at=now() WHERE created_by=$1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBody {
    username: String,
    email: Option<String>,
    password: String,
    role: String,
}
async fn create(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<CreateBody>,
) -> Result<Json<Value>> {
    if a.role != "owner" {
        return Err(Error::Forbidden);
    }
    let name = username(&b.username)?;
    let email = email(b.email.as_deref())?;
    password(&b.password)?;
    if !auth::valid_role(&b.role) {
        return Err(Error::bad("Неизвестная роль"));
    }
    let hash = tokio::task::spawn_blocking(move || auth::hash_password(&b.password))
        .await
        .map_err(|_| Error::Internal("password task failed".into()))??;
    let mut tx = st.pool.begin().await?;
    owner_lock(&mut tx, a.id).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO admins(username,email,password_hash,role) VALUES($1,$2,$3,$4) RETURNING id",
    )
    .bind(name)
    .bind(email)
    .bind(hash)
    .bind(&b.role)
    .fetch_one(&mut *tx)
    .await?;
    audit(&mut tx, a.id, id, "admin.create", json!({"role":b.role})).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateBody {
    role: String,
    is_active: bool,
}
async fn update(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<UpdateBody>,
) -> Result<Json<Value>> {
    if a.role != "owner" {
        return Err(Error::Forbidden);
    }
    if !auth::valid_role(&b.role) {
        return Err(Error::bad("Неизвестная роль"));
    }
    if id == a.id && (!b.is_active || b.role != "owner") {
        return Err(Error::bad(
            "Нельзя отключить свой аккаунт или снять с себя роль владельца",
        ));
    }
    let mut tx = st.pool.begin().await?;
    owner_lock(&mut tx, a.id).await?;
    let old = sqlx::query("SELECT role,is_active FROM admins WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    sqlx::query("UPDATE admins SET role=$2,is_active=$3 WHERE id=$1")
        .bind(id)
        .bind(&b.role)
        .bind(b.is_active)
        .execute(&mut *tx)
        .await?;
    if old.get::<String, _>("role") != b.role || old.get::<bool, _>("is_active") != b.is_active {
        credentials_revoke(&mut tx, id).await?;
    }
    audit(
        &mut tx,
        a.id,
        id,
        "admin.access",
        json!({"role":b.role,"is_active":b.is_active}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordBody {
    password: String,
}
async fn reset_password(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PasswordBody>,
) -> Result<Json<Value>> {
    if a.role != "owner" {
        return Err(Error::Forbidden);
    }
    if id == a.id {
        return Err(Error::bad("Смените свой пароль в профиле"));
    }
    password(&b.password)?;
    let hash = tokio::task::spawn_blocking(move || auth::hash_password(&b.password))
        .await
        .map_err(|_| Error::Internal("password task failed".into()))??;
    let mut tx = st.pool.begin().await?;
    owner_lock(&mut tx, a.id).await?;
    let n = sqlx::query("UPDATE admins SET password_hash=$2 WHERE id=$1")
        .bind(id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    if n.rows_affected() != 1 {
        return Err(Error::NotFound);
    }
    credentials_revoke(&mut tx, id).await?;
    audit(&mut tx, a.id, id, "admin.password.reset", json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
async fn revoke(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    if a.role != "owner" {
        return Err(Error::Forbidden);
    }
    if id == a.id {
        return Err(Error::bad("Управляйте своими сессиями в профиле"));
    }
    let mut tx = st.pool.begin().await?;
    owner_lock(&mut tx, a.id).await?;
    sqlx::query("SELECT id FROM admins WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    credentials_revoke(&mut tx, id).await?;
    audit(&mut tx, a.id, id, "admin.sessions.revoke", json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_staff_credentials_and_rejects_profile_role_injection() {
        for s in ["ab", " x/y ", "<admin>", "админ"] {
            assert!(username(s).is_err());
        }
        assert_eq!(username(" support.one ").unwrap(), "support.one");
        assert!(password("12345678").is_err());
        assert!(password("test-account-password").is_ok());
        assert!(serde_json::from_value::<ProfileBody>(
            json!({"username":"support","email":null,"current_password":"test","role":"owner"})
        )
        .is_err());
    }
}

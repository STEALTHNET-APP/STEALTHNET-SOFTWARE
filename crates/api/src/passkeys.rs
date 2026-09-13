//! Вход без пароля (WebAuthn / passkeys).
//!
//! Здесь, в отличие от одноразовых кодов, своя реализация исключена:
//! WebAuthn — это CBOR, ключи COSE и разбор аттестации. Ошибка в таком
//! коде не видна на глаз и стоит доступа к панели, поэтому берём
//! проверенную библиотеку.
//!
//! Смысл passkey в том, что закрытый ключ не покидает устройство и не
//! может быть подобран или выужен фишингом: браузер подписывает вызов
//! только для того домена, на котором ключ создан.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use webauthn_rs::prelude::*;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

/// Незавершённые церемонии. Между «начать» и «закончить» проходят
/// секунды, поэтому держим в памяти: переживать перезапуск процесса
/// такому состоянию незачем, а в базе оно бы копилось мусором.
#[derive(Clone, Default)]
pub struct Ceremonies {
    reg: Arc<Mutex<HashMap<String, (PasskeyRegistration, std::time::Instant)>>>,
    auth: Arc<Mutex<HashMap<String, (PasskeyAuthentication, i64, std::time::Instant)>>>,
}

/// Церемония живёт минуту: столько человек тратит на прикосновение к
/// ключу, а брошенные записи не должны накапливаться.
const CEREMONY_TTL: std::time::Duration = std::time::Duration::from_secs(60);

impl Ceremonies {
    fn put_reg(&self, key: String, state: PasskeyRegistration) {
        let mut m = self.reg.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|_, (_, at)| at.elapsed() < CEREMONY_TTL);
        m.insert(key, (state, std::time::Instant::now()));
    }
    fn take_reg(&self, key: &str) -> Option<PasskeyRegistration> {
        let mut m = self.reg.lock().unwrap_or_else(|e| e.into_inner());
        m.remove(key).filter(|(_,at)|at.elapsed()<CEREMONY_TTL).map(|(s,_)|s)
    }
    fn put_auth(&self, key: String, state: PasskeyAuthentication, admin_id: i64) {
        let mut m = self.auth.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|_, (_, _, at)| at.elapsed() < CEREMONY_TTL);
        m.insert(key, (state, admin_id, std::time::Instant::now()));
    }
    fn take_auth(&self, key: &str) -> Option<(PasskeyAuthentication, i64)> {
        let mut m = self.auth.lock().unwrap_or_else(|e| e.into_inner());
        m.remove(key).filter(|(_,_,at)|at.elapsed()<CEREMONY_TTL).map(|(s,id,_)|(s,id))
    }
}

pub fn passkey_routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/passkeys/register", post(register_start))
        .route("/api/admin/passkeys/register/finish", post(register_finish))
        .route("/api/admin/passkeys/{id}", axum::routing::delete(passkey_delete))
        .route("/api/auth/passkey", post(login_start))
        .route("/api/auth/passkey/finish", post(login_finish))
        .route("/api/auth/passkey/available", get(available))
}

/// Собирает WebAuthn под адрес панели.
///
/// Ключ привязан к домену: браузер не подпишет вызов для чужого сайта —
/// именно поэтому passkey нельзя выудить фишингом. Обратная сторона —
/// адрес панели должен быть настоящим, иначе ключи не заработают.
async fn webauthn(st: &AppState) -> Result<Webauthn> {
    let url = crate::sub_service::panel_public_url_pub(st).await;
    let parsed = Url::parse(&url).map_err(|_| {
        Error::bad("публичный адрес панели не разбирается — задайте его в настройках")
    })?;
    let rp_id = parsed
        .host_str()
        .ok_or_else(|| Error::bad("в адресе панели нет домена"))?
        .to_string();

    WebauthnBuilder::new(&rp_id, &parsed)
        .and_then(|b| b.rp_name("STEALTHNET").build())
        .map_err(|_| Error::bad("Для входа по ключу укажите корректный HTTPS-адрес панели в настройках"))
}

/// Есть ли у кого-то ключи. Панель по этому решает, показывать ли
/// кнопку входа без пароля — предлагать её при отсутствии ключей значит
/// вести человека в тупик.
async fn available(State(st): State<AppState>) -> Result<Json<Value>> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_passkeys")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    Ok(Json(json!({ "available": n > 0 })))
}

async fn register_start(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
) -> Result<Json<Value>> {
    let w = webauthn(&st).await?;

    // Уже зарегистрированные исключаем: иначе на одном устройстве
    // заведётся второй ключ, и список превратится в кашу.
    let existing: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT credential_id FROM admin_passkeys WHERE admin_id = $1")
            .bind(admin.id)
            .fetch_all(&st.pool)
            .await?;
    let exclude: Vec<CredentialID> = existing.into_iter().map(|v| v.into()).collect();

    let user_id = Uuid::from_u128(admin.id as u128);
    let (challenge, state) = w
        .start_passkey_registration(user_id, &admin.username, &admin.username, Some(exclude))
        .map_err(|_| Error::bad("Для входа по ключу укажите корректный HTTPS-адрес панели в настройках"))?;

    let key = format!("reg:{}", admin.id);
    st.ceremonies.put_reg(key, state);

    Ok(Json(json!({ "options": challenge })))
}

#[derive(Deserialize)]
struct FinishReg {
    label: Option<String>,
    credential: RegisterPublicKeyCredential,
}

async fn register_finish(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<FinishReg>,
) -> Result<Json<Value>> {
    let w = webauthn(&st).await?;
    let state = st
        .ceremonies
        .take_reg(&format!("reg:{}", admin.id))
        .ok_or_else(|| Error::bad("время на подтверждение истекло — начните заново"))?;

    let key = w
        .finish_passkey_registration(&b.credential, &state)
        .map_err(|e| Error::bad(format!("ключ не принят: {e}")))?;

    // Храним сериализованный ключ целиком: в нём не только открытая
    // часть, но и счётчик с флагами, которые нужны при проверке.
    let blob = serde_json::to_vec(&key).map_err(|e| Error::Internal(e.to_string()))?;

    sqlx::query(
        "INSERT INTO admin_passkeys (admin_id, credential_id, public_key, label)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(admin.id)
    .bind(key.cred_id().as_ref())
    .bind(&blob)
    .bind(b.label.as_deref().unwrap_or("Ключ"))
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn passkey_delete(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM admin_passkeys WHERE id = $1 AND admin_id = $2")
        .bind(id)
        .bind(admin.id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct LoginStart {
    username: String,
}

async fn login_start(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(b): Json<LoginStart>,
) -> Result<Json<Value>> {
    if b.username.len() > 254 { return Err(Error::Unauthorized); }
    crate::security::auth_attempt(&st, &headers, peer.ip(), Some(&b.username)).await?;
    let w = webauthn(&st).await?;

    let row = sqlx::query(
        "SELECT a.id, array_agg(p.public_key) AS keys
           FROM admins a JOIN admin_passkeys p ON p.admin_id = a.id
          WHERE lower(a.username) = lower($1) AND a.is_active
          GROUP BY a.id",
    )
    .bind(b.username.trim())
    .fetch_optional(&st.pool)
    .await?;

    // Отвечаем одинаково на «нет такого» и «нет ключей»: иначе по
    // ответу перебирают существующие логины.
    let Some(row) = row else {
        return Err(Error::bad("вход по ключу для этого логина недоступен"));
    };

    let admin_id: i64 = row.get("id");
    let blobs: Vec<Vec<u8>> = row.try_get("keys").unwrap_or_default();
    let keys: Vec<Passkey> = blobs
        .iter()
        .filter_map(|b| serde_json::from_slice(b).ok())
        .collect();

    if keys.is_empty() {
        return Err(Error::bad("вход по ключу для этого логина недоступен"));
    }

    let (challenge, state) = w
        .start_passkey_authentication(&keys)
        .map_err(|_| Error::bad("Для входа по ключу укажите корректный HTTPS-адрес панели в настройках"))?;

    let ticket = sn_core::auth::generate_token();
    st.ceremonies.put_auth(ticket.clone(), state, admin_id);

    Ok(Json(json!({ "ticket": ticket, "options": challenge })))
}

#[derive(Deserialize)]
struct LoginFinish {
    ticket: String,
    credential: PublicKeyCredential,
}

async fn login_finish(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(b): Json<LoginFinish>,
) -> Result<Json<Value>> {
    crate::security::auth_attempt(&st, &headers, peer.ip(), None).await?;
    let w = webauthn(&st).await?;
    let (state, admin_id) = st
        .ceremonies
        .take_auth(&b.ticket)
        .ok_or_else(|| Error::bad("время на подтверждение истекло — начните заново"))?;

    let result = w
        .finish_passkey_authentication(&b.credential, &state)
        .map_err(|e| Error::bad(format!("ключ не подошёл: {e}")))?;

    let mut tx=st.pool.begin().await?;
    let stored=sqlx::query("SELECT public_key,sign_count FROM admin_passkeys WHERE credential_id=$1 AND admin_id=$2 FOR UPDATE")
        .bind(result.cred_id().as_ref()).bind(admin_id).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
    let count=stored.get::<i64,_>("sign_count");
    if (count>0 || result.counter()>0) && i64::from(result.counter())<=count {
        return Err(Error::bad("счётчик ключа не вырос — начните вход заново"));
    }
    let mut key: Passkey=serde_json::from_slice(&stored.get::<Vec<u8>,_>("public_key"))
        .map_err(|e|Error::Internal(e.to_string()))?;
    key.update_credential(&result).ok_or(Error::Unauthorized)?;
    let blob=serde_json::to_vec(&key).map_err(|e|Error::Internal(e.to_string()))?;
    sqlx::query("UPDATE admin_passkeys SET public_key=$2,sign_count=$3,last_used_at=now() WHERE credential_id=$1 AND admin_id=$4")
        .bind(result.cred_id().as_ref()).bind(blob).bind(i64::from(result.counter())).bind(admin_id).execute(&mut *tx).await?;
    tx.commit().await?;

    let (admin, token) = sn_core::auth::issue_session(&st.pool, admin_id).await?;
    Ok(Json(json!({ "token": token, "admin": admin })))
}

//! Аутентификация администраторов.
//!
//! Сессии — непрозрачные токены в БД, а не JWT: их можно отозвать мгновенно,
//! что важно для админки (уволили сотрудника — вышибли сессию).
//! В базе лежит только SHA-256 от токена, сам токен видит лишь клиент.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::{Error, Pool, Result};

pub const SESSION_TTL_DAYS: i64 = 14;

/// Хэш пароля (argon2id) для хранения в БД.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::Internal(format!("хэширование пароля: {e}")))
}

pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Случайный токен сессии (32 байта энтропии в hex).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Admin {
    pub id: i64,
    pub username: String,
    pub role: String,
}

/// Проверяет логин/пароль и заводит сессию. Возвращает токен для клиента.
/// Вход. `code` — одноразовый код, если у администратора включён TOTP.
///
/// Ошибку «нужен код» отдаём отдельным видом: панель по ней показывает
/// поле ввода, а не сообщает «неверный пароль» человеку, который ввёл
/// верный пароль.
/// Выдаёт сессию администратору, чья личность уже подтверждена другим
/// способом — например ключом passkey. Пароль здесь не проверяется:
/// вызывать эту функцию можно только после успешной проверки.
pub async fn issue_session(pool: &Pool, admin_id: i64) -> Result<(Admin, String)> {
    let mut tx=pool.begin().await?;
    let row: Option<(i64, String, String, bool)> = sqlx::query_as(
        "SELECT id, username, role, is_active FROM admins WHERE id = $1 FOR UPDATE",
    )
    .bind(admin_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((id, username, role, is_active)) = row else {
        return Err(Error::Unauthorized);
    };
    if !is_active {
        return Err(Error::Unauthorized);
    }

    let token = generate_token();
    sqlx::query(
        "INSERT INTO admin_sessions (admin_id, token_hash, expires_at)
         VALUES ($1, $2, now() + ($3 || ' days')::interval)",
    )
    .bind(id)
    .bind(token_hash(&token))
    .bind(SESSION_TTL_DAYS.to_string())
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE admins SET last_login_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok((Admin { id, username, role }, token))
}

pub async fn login(
    pool: &Pool,
    username: &str,
    password: &str,
    code: Option<&str>,
) -> Result<(Admin, String)> {
    let row: Option<(i64, String, String, String, bool, Option<String>)> = sqlx::query_as(
        "SELECT id, username, role, password_hash, is_active, totp_secret
           FROM admins WHERE lower(username) = lower($1)",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    // Bound memory/CPU, keep Argon2 off the async worker, and do equal password
    // work for an unknown account instead of exposing existence through timing.
    static PASSWORD_WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        hash_password(&generate_token()).expect("Argon2 dummy hash")
    });
    let permit = PASSWORD_WORK.try_acquire().map_err(|_| Error::TooManyRequests)?;
    let stored = row.as_ref().map(|r| r.3.clone());
    let password = password.to_owned();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        verify_password(&password, stored.as_deref().unwrap_or(&DUMMY_HASH))
    }).await.map_err(|_| Error::Internal("password verification task failed".into()))?;

    let Some((id, username, role, hash, is_active, totp_secret)) = row else {
        return Err(Error::Unauthorized);
    };
    if !is_active || !valid {
        return Err(Error::Unauthorized);
    }

    // Serialize session creation with staff deactivation/password changes.
    // A password checked before a reset must not create a fresh session after it.
    let mut tx=pool.begin().await?;
    let current=sqlx::query_as::<_,(String,String,bool,Option<String>)>("SELECT password_hash,role,is_active,totp_secret FROM admins WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::Unauthorized)?;
    if !current.2 || current.0!=hash || current.1!=role || current.3!=totp_secret {return Err(Error::Unauthorized);}

    // Код проверяем только после пароля: иначе по ответу можно было бы
    // узнать, у кого включена двухфакторная, не зная пароля.
    if let Some(secret) = totp_secret.as_deref().filter(|s| !s.is_empty()) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let code = code.ok_or_else(|| Error::bad("нужен код из приложения-аутентификатора"))?;
        let step = crate::totp::verified_step(secret, code, now)
            .ok_or_else(|| Error::bad("неверный код подтверждения"))? as i64;
        let used = sqlx::query("UPDATE admins SET totp_last_used_step=$3 WHERE id=$1 AND totp_secret=$2 AND is_active AND (totp_last_used_step IS NULL OR totp_last_used_step<$3)")
            .bind(id).bind(secret).bind(step).execute(&mut *tx).await?;
        if used.rows_affected() != 1 {
            return Err(Error::bad("Код уже использован. Дождитесь нового кода в приложении"));
        }
    }

    let token = generate_token();
    sqlx::query(
        "INSERT INTO admin_sessions (admin_id, token_hash, expires_at)
         VALUES ($1, $2, now() + ($3 || ' days')::interval)",
    )
    .bind(id)
    .bind(token_hash(&token))
    .bind(SESSION_TTL_DAYS.to_string())
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE admins SET last_login_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok((Admin { id, username, role }, token))
}

/// Находит администратора по токену сессии. Просроченные сессии не проходят.
pub async fn admin_by_token(pool: &Pool, token: &str) -> Result<Admin> {
    let row: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT a.id, a.username, a.role
           FROM admin_sessions s
           JOIN admins a ON a.id = s.admin_id
          WHERE s.token_hash = $1 AND s.expires_at > now() AND a.is_active",
    )
    .bind(token_hash(token))
    .fetch_optional(pool)
    .await?;

    row.map(|(id, username, role)| Admin { id, username, role })
        .ok_or(Error::Unauthorized)
}

/// Integration credentials inherit the current rights of their active creator.
/// A deleted/disabled administrator or a revoked token cannot authenticate.
pub async fn api_admin_by_token(pool: &Pool, token: &str) -> Result<(Admin, i64, Vec<String>)> {
    let row: Option<(i64, i64, String, String, Vec<String>)> = sqlx::query_as(
        "SELECT t.id, a.id, a.username, a.role, t.scopes
           FROM api_tokens t JOIN admins a ON a.id=t.created_by
          WHERE t.token_hash=$1 AND t.revoked_at IS NULL AND a.is_active",
    )
    .bind(token_hash(token))
    .fetch_optional(pool)
    .await?;
    row.map(|(token_id, id, username, role, scopes)| (Admin { id, username, role }, token_id, scopes))
        .ok_or(Error::Unauthorized)
}

pub fn valid_role(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "support" | "readonly")
}

pub async fn logout(pool: &Pool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM admin_sessions WHERE token_hash = $1")
        .bind(token_hash(token))
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn пароль_проверяется_и_не_хранится_в_открытую() {
        let hash = hash_password("secret123").unwrap();
        assert!(!hash.contains("secret123"));
        assert!(verify_password("secret123", &hash));
        assert!(!verify_password("secret124", &hash));
    }

    #[test]
    fn одинаковые_пароли_дают_разные_хэши() {
        // соль случайна — иначе по базе видно, у кого пароли совпадают
        assert_ne!(hash_password("a").unwrap(), hash_password("a").unwrap());
    }

    #[test]
    fn токены_не_повторяются_и_хэшируются() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(token_hash(&a).len(), 32);
        assert_ne!(token_hash(&a), token_hash(&b));
    }
}

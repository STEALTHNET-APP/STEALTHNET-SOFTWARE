//! Единственная регистрация для бота и Mini App, включая одновременный первый вход.
use crate::{Error,Pool,Result};
pub async fn ensure(pool:&Pool, telegram_id:i64, username:Option<&str>) -> Result<i64> {
    if telegram_id <= 0 { return Err(Error::Unauthorized); }
    let mut tx=pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(telegram_id).execute(&mut *tx).await?;
    let existing:Option<(i64,bool)>=sqlx::query_as("SELECT c.id,c.deleted_at IS NOT NULL FROM clients c JOIN client_identities i ON i.client_id=c.id WHERE i.kind='telegram' AND i.value=$1")
        .bind(telegram_id.to_string()).fetch_optional(&mut *tx).await?;
    if let Some((id,deleted))=existing {
        if deleted { return Err(Error::bad("Аккаунт удалён. Обратитесь в поддержку")); }
        tx.commit().await?; return Ok(id);
    }
    let username=username.filter(|s| !s.trim().is_empty()).map(|s| s.chars().take(64).collect::<String>()).unwrap_or_else(||format!("tg{telegram_id}"));
    let id:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id,status) VALUES($1,$2,'expired') RETURNING id")
        .bind(username).bind(uuid::Uuid::new_v4().simple().to_string()).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO client_identities(client_id,kind,value,is_verified) VALUES($1,'telegram',$2,true)")
        .bind(id).bind(telegram_id.to_string()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO subscriptions(client_id,expires_at,device_limit) VALUES($1,NULL,1)").bind(id).execute(&mut *tx).await?;
    tx.commit().await?; Ok(id)
}

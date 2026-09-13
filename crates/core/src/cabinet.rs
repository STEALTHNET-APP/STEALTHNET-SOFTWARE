//! High-entropy reusable client codes. Never reuse subscription/admin tokens.
use crate::{Error, Pool, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub fn generate_code() -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut random = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut random);
    let mut out = String::from("SN");
    for (i, byte) in random.iter().enumerate() {
        if i % 4 == 0 { out.push('-'); }
        out.push(ALPHABET[(byte & 31) as usize] as char);
    }
    out
}
pub fn normalize_code(raw: &str) -> Option<String> {
    if raw.len() > 100 { return None; }
    let normalized: String = raw.chars().filter(|c| !c.is_ascii_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).collect();
    let code = normalized.strip_prefix("SN")?;
    (code.len() == 20 && code.bytes().all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))).then_some(normalized)
}
pub fn code_hash(raw: &str) -> Option<Vec<u8>> {
    let code = normalize_code(raw)?;
    Some(Sha256::digest(format!("stealthnet:client-code:v1:{code}").as_bytes()).to_vec())
}
fn code_key() -> Result<Vec<u8>> {
    std::env::var("CABINET_CODE_KEY").ok().and_then(|v|hex::decode(v.trim()).ok())
        .filter(|v|v.len()==32).ok_or_else(||Error::Internal("CABINET_CODE_KEY must be 32 random bytes encoded as hex".into()))
}
fn seal_with_key(raw:&str,key:&[u8])->Result<Vec<u8>> {
    use ring::aead::{Aad,LessSafeKey,Nonce,UnboundKey,AES_256_GCM};
    let normalized=normalize_code(raw).ok_or(Error::Unauthorized)?;
    let canonical=format!("SN-{}",normalized[2..].as_bytes().chunks(4).map(|p|std::str::from_utf8(p).unwrap()).collect::<Vec<_>>().join("-"));
    let key=LessSafeKey::new(UnboundKey::new(&AES_256_GCM,key).map_err(|_|Error::Internal("Invalid cabinet encryption key".into()))?);
    let mut nonce=[0u8;12];rand::rngs::OsRng.fill_bytes(&mut nonce);
    let mut encrypted=canonical.as_bytes().to_vec();
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce),Aad::from(code_hash(raw).unwrap()),&mut encrypted).map_err(|_|Error::Internal("Cannot encrypt cabinet code".into()))?;
    let mut out=vec![1];out.extend(nonce);out.extend(encrypted);Ok(out)
}
fn open_with_key(sealed:&[u8],hash:&[u8],key:&[u8])->Result<String>{
    use ring::aead::{Aad,LessSafeKey,Nonce,UnboundKey,AES_256_GCM};
    let invalid=||Error::Internal("Cannot decrypt cabinet code".into());
    if sealed.len()!=56||sealed[0]!=1{return Err(invalid());}
    let key=LessSafeKey::new(UnboundKey::new(&AES_256_GCM,key).map_err(|_|invalid())?);
    let mut encrypted=sealed[13..].to_vec();
    let plain=key.open_in_place(Nonce::assume_unique_for_key(sealed[1..13].try_into().unwrap()),Aad::from(hash),&mut encrypted).map_err(|_|invalid())?;
    let code=String::from_utf8(plain.to_vec()).map_err(|_|invalid())?;
    if code_hash(&code).as_deref()!=Some(hash){return Err(invalid());}Ok(code)
}
pub fn seal_code(raw:&str)->Result<Vec<u8>>{seal_with_key(raw,&code_key()?)}
/// Only call after authenticating the customer, never from a public/config route.
pub async fn reveal_code(pool:&Pool,client:i64)->Result<Option<String>>{
    reveal_code_as(pool,client,"client",client).await
}
pub async fn reveal_code_as(pool:&Pool,client:i64,actor_kind:&str,actor_id:i64)->Result<Option<String>>{
    let row:Option<(Vec<u8>,Option<Vec<u8>>)>=sqlx::query_as("SELECT code_hash,code_sealed FROM cabinet_credentials WHERE client_id=$1").bind(client).fetch_optional(pool).await?;
    let Some((hash,Some(sealed)))=row else{return Ok(None)};
    let code=open_with_key(&sealed,&hash,&code_key()?)?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id) VALUES($2,$3,'cabinet.code_viewed','client',$1)").bind(client).bind(actor_kind).bind(actor_id).execute(pool).await?;
    Ok(Some(code))
}
/// Issue or deliberately replace a code for an already authenticated Telegram client.
pub async fn issue_from_telegram(pool: &Pool, client: i64, replace: bool) -> Result<String> {
    issue_code_as(pool,client,replace,"client",client).await
}
pub async fn issue_code_as(pool:&Pool,client:i64,replace:bool,actor_kind:&str,actor_id:i64)->Result<String>{
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM clients WHERE id=$1 AND deleted_at IS NULL FOR UPDATE").bind(client).fetch_one(&mut *tx).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinet_credentials WHERE client_id=$1)").bind(client).fetch_one(&mut *tx).await?;
    if exists && !replace { return Err(Error::Conflict("Код уже выпущен. Для нового кода подтвердите замену".into())); }
    let code = generate_code();
    sqlx::query("INSERT INTO cabinet_credentials(client_id,code_hash,code_sealed,saved_at) VALUES($1,$2,$3,now()) ON CONFLICT(client_id) DO UPDATE SET code_hash=EXCLUDED.code_hash,code_sealed=EXCLUDED.code_sealed,registration_hash=NULL,saved_at=now(),rotated_at=now()")
        .bind(client).bind(code_hash(&code).unwrap()).bind(seal_code(&code)?).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM cabinet_sessions WHERE client_id=$1").bind(client).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id) VALUES($2,$3,'cabinet.code_issued','client',$1)").bind(client).bind(actor_kind).bind(actor_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(code)
}
pub async fn create_link(pool:&Pool,client:i64,installation:uuid::Uuid)->Result<String>{
    let token=uuid::Uuid::new_v4().simple().to_string();
    let mut tx=pool.begin().await?;
    sqlx::query("DELETE FROM cabinet_links WHERE client_id=$1 OR expires_at<now()").bind(client).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO cabinet_links(token_hash,client_id,installation_id) VALUES($1,$2,$3)").bind(crate::auth::token_hash(&token)).bind(client).bind(installation).execute(&mut *tx).await?;
    tx.commit().await?;Ok(token)
}
pub async fn link_info(pool:&Pool,token:&str)->Result<(String,String)>{
    if token.len()!=32||!token.bytes().all(|b|b.is_ascii_hexdigit()){return Err(Error::NotFound);}
    sqlx::query_as("SELECT i.public_url,c.username FROM cabinet_links l JOIN clients c ON c.id=l.client_id JOIN cabinet_installations i ON i.id=l.installation_id WHERE l.token_hash=$1 AND l.expires_at>now() AND l.used_at IS NULL AND i.revoked_at IS NULL AND c.deleted_at IS NULL")
        .bind(crate::auth::token_hash(token)).fetch_optional(pool).await?.ok_or_else(||Error::bad("Ссылка привязки истекла. Создайте новую в кабинете"))
}
/// Only an empty shell can be reconciled. Two used accounts are never silently merged.
pub async fn confirm_link(pool:&Pool,telegram_client:i64,token:&str)->Result<i64>{
    link_info(pool,token).await?;
    let mut tx=pool.begin().await?;
    let target:i64=sqlx::query_scalar("SELECT l.client_id FROM cabinet_links l JOIN cabinet_installations i ON i.id=l.installation_id WHERE l.token_hash=$1 AND l.expires_at>now() AND l.used_at IS NULL AND i.revoked_at IS NULL FOR UPDATE OF l, i").bind(crate::auth::token_hash(token)).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let locked=sqlx::query("SELECT id FROM clients WHERE id=ANY($1) AND deleted_at IS NULL ORDER BY id FOR UPDATE").bind(vec![target,telegram_client]).fetch_all(&mut *tx).await?;
    if locked.len()!=if target==telegram_client {1} else {2} {return Err(Error::NotFound);}
    let mut destination=target;
    if target!=telegram_client {
        let target_linked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM client_identities WHERE client_id=$1 AND kind='telegram')").bind(target).fetch_one(&mut *tx).await?;
        if target_linked{return Err(Error::bad("К кабинету уже привязан другой Telegram"));}
        let activity_sql="SELECT EXISTS(SELECT 1 FROM payments WHERE client_id=$1) OR EXISTS(SELECT 1 FROM subscriptions WHERE client_id=$1 AND (tariff_id IS NOT NULL OR expires_at IS NOT NULL)) OR EXISTS(SELECT 1 FROM devices WHERE client_id=$1) OR EXISTS(SELECT 1 FROM tickets WHERE client_id=$1) OR EXISTS(SELECT 1 FROM partners WHERE client_id=$1)";
        let tg_used:bool=sqlx::query_scalar(activity_sql).bind(telegram_client).fetch_one(&mut *tx).await?;
        let target_used:bool=sqlx::query_scalar(activity_sql).bind(target).fetch_one(&mut *tx).await?;
        let tg_code:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinet_credentials WHERE client_id=$1)").bind(telegram_client).fetch_one(&mut *tx).await?;
        if !tg_used&&!tg_code {
            let r=sqlx::query("UPDATE client_identities SET client_id=$1 WHERE client_id=$2 AND kind='telegram'").bind(target).bind(telegram_client).execute(&mut *tx).await?;
            if r.rows_affected()!=1{return Err(Error::bad("Telegram не подтверждён"));}
            sqlx::query("UPDATE clients SET deleted_at=now() WHERE id=$1").bind(telegram_client).execute(&mut *tx).await?;
        } else if !target_used&&!tg_code {
            // Keep the existing Telegram history; migrate only the unused website identity.
            sqlx::query("UPDATE cabinet_credentials SET client_id=$1 WHERE client_id=$2").bind(telegram_client).bind(target).execute(&mut *tx).await?;
            sqlx::query("UPDATE cabinet_sessions SET client_id=$1 WHERE client_id=$2").bind(telegram_client).bind(target).execute(&mut *tx).await?;
            sqlx::query("UPDATE clients SET deleted_at=now() WHERE id=$1").bind(target).execute(&mut *tx).await?;
            destination=telegram_client;
        } else {return Err(Error::bad("Уже существуют два аккаунта с данными или кодом доступа. Получите код существующего аккаунта в Mini App; подписки не объединены"));}
    }
    sqlx::query("UPDATE cabinet_links SET used_at=now() WHERE token_hash=$1").bind(crate::auth::token_hash(token)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id) VALUES('client',$1,'cabinet.telegram_linked','client',$2)").bind(telegram_client).bind(destination).execute(&mut *tx).await?;
    tx.commit().await?;Ok(destination)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn encrypted_code_is_authenticated_and_randomized(){
        let code=generate_code();let hash=code_hash(&code).unwrap();let key=[41u8;32];
        let a=seal_with_key(&code,&key).unwrap();let b=seal_with_key(&code,&key).unwrap();
        assert_ne!(a,b);assert!(!a.windows(code.len()).any(|v|v==code.as_bytes()));
        assert_eq!(open_with_key(&a,&hash,&key).unwrap(),code);
        assert!(open_with_key(&a,&hash,&[42u8;32]).is_err());
        assert!(open_with_key(&a,&code_hash(&generate_code()).unwrap(),&key).is_err());
        for i in 0..a.len(){let mut corrupt=a.clone();corrupt[i]^=1;assert!(open_with_key(&corrupt,&hash,&key).is_err());}
        assert!(open_with_key(&a[..55],&hash,&key).is_err());
    }
    #[test] fn generated_codes_have_fixed_entropy_and_roundtrip() {
        let mut seen=std::collections::HashSet::new();
        for _ in 0..1024 { let code=generate_code(); assert_eq!(code.len(),27); assert!(seen.insert(code.clone())); assert_eq!(code_hash(&code),code_hash(&code.to_lowercase().replace('-'," "))); }
    }
    #[test] fn short_ids_and_unicode_are_not_credentials() {
        for value in ["", "SN-1234", "123456", "SN-OOOO-OOOO-OOOO-OOOO-OOOO", "SN-１２３４-１２３４-１２３４-１２３４-１２３４"] { assert!(code_hash(value).is_none()); }
    }
}

/// Resolve only an installed, recently responding customer gateway. Never use the panel URL.
pub async fn miniapp_url(pool: &crate::Pool) -> crate::Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT i.public_url || '/app/' FROM cabinet_installations i JOIN settings s ON s.key='cabinet.config' JOIN settings b ON b.key='bot.miniapp_enabled' WHERE s.value->'enabled'='true'::jsonb AND b.value='true'::jsonb AND s.value->>'miniapp_installation_id'=i.id::text AND i.revoked_at IS NULL AND i.token_hash IS NOT NULL AND i.last_seen_at>now()-interval '2 minutes'")
        .fetch_optional(pool).await?)
}

#[cfg(test)]
#[path = "cabinet_tests.rs"]
mod integration_tests;

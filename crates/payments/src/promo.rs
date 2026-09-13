//! Reserve a discount while issuing an invoice; consume it only on successful payment.
use sn_core::{Error,Result};
use sqlx::{PgConnection,Row};

pub async fn reserve(conn: &mut PgConnection, client: i64, tariff: i64, id: i64, amount: i64, days: i32, currency: &str) -> Result<(i64,i32)> {
    sqlx::query("SELECT id FROM clients WHERE id=$1 FOR UPDATE").bind(client).fetch_one(&mut *conn).await?;
    let p=sqlx::query("SELECT kind::text AS kind,value,currency,per_client_limit,first_purchase_only,
        max_uses,used_count,tariff_id,is_active AND (valid_from IS NULL OR valid_from<=now()) AND (valid_until IS NULL OR valid_until>now()) AS valid
        FROM promo_codes WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *conn).await?.ok_or_else(||Error::bad("промокод не найден"))?;
    if !p.get::<bool,_>("valid") || p.get::<Option<i64>,_>("tariff_id").is_some_and(|t|t!=tariff) {return Err(Error::bad("промокод не действует для этого тарифа"));}
    let counts=sqlx::query("SELECT
        (SELECT count(*) FROM payments WHERE promo_code_id=$1 AND status='pending' AND (expires_at IS NULL OR expires_at>now())) AS reserved,
        (SELECT count(*) FROM payments WHERE promo_code_id=$1 AND client_id=$2 AND status='pending' AND (expires_at IS NULL OR expires_at>now()))+
        (SELECT count(*) FROM promo_redemptions WHERE promo_code_id=$1 AND client_id=$2) AS mine,
        EXISTS(SELECT 1 FROM payments WHERE client_id=$2 AND (status='success' OR (status='pending' AND (expires_at IS NULL OR expires_at>now())))) AS prior")
        .bind(id).bind(client).fetch_one(&mut *conn).await?;
    if p.get::<Option<i32>,_>("max_uses").is_some_and(|max| i64::from(p.get::<i32,_>("used_count"))+counts.get::<i64,_>("reserved")>=i64::from(max)) {return Err(Error::bad("лимит промокода исчерпан или зарезервирован в неоплаченных счетах"));}
    if counts.get::<i64,_>("mine")>=i64::from(p.get::<i32,_>("per_client_limit")) {return Err(Error::bad("промокод уже применён или зарезервирован вашим счётом"));}
    if p.get::<bool,_>("first_purchase_only") && counts.get::<bool,_>("prior") {return Err(Error::bad("промокод действует только на первую покупку; проверьте ранее созданные счета"));}
    let value=i64::from(p.get::<i32,_>("value"));
    match p.get::<String,_>("kind").as_str() {
        "percent"=>Ok(((i128::from(amount)-i128::from(amount)*i128::from(value)/100).max(0) as i64,days)),
        "days"=>Ok((amount,days.checked_add(value as i32).ok_or_else(||Error::bad("слишком большой срок"))?)),
        _=>{
            if p.get::<Option<String>,_>("currency").as_deref()!=Some(currency) {return Err(Error::bad("валюта фиксированной скидки не совпадает с валютой счёта"));}
            Ok(((amount-value).max(0),days))
        }
    }
}

pub async fn consume(conn:&mut PgConnection,payment:i64)->Result<()> {
    let p:Option<(i64,i64)>=sqlx::query_as("SELECT promo_code_id,client_id FROM payments WHERE id=$1 AND promo_code_id IS NOT NULL").bind(payment).fetch_optional(&mut *conn).await?;
    if let Some((promo,client))=p {
        sqlx::query("SELECT id FROM promo_codes WHERE id=$1 FOR UPDATE").bind(promo).fetch_one(&mut *conn).await?;
        let inserted=sqlx::query("INSERT INTO promo_redemptions(promo_code_id,client_id,payment_id) SELECT $1,$2,$3 WHERE NOT EXISTS(SELECT 1 FROM promo_redemptions WHERE payment_id=$3)")
            .bind(promo).bind(client).bind(payment).execute(&mut *conn).await?;
        if inserted.rows_affected()>0 {sqlx::query("UPDATE promo_codes SET used_count=used_count+1 WHERE id=$1").bind(promo).execute(&mut *conn).await?;}
        sqlx::query("DELETE FROM bot_promo_hold WHERE client_id=$1 AND promo_id=$2").bind(client).bind(promo).execute(&mut *conn).await?;
    }
    Ok(())
}

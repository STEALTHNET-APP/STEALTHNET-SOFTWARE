//! Partner accounting shares the payment transaction and locks one partner wallet.
use crate::Result;
use sqlx::{PgConnection, Row};

pub async fn accrue(conn: &mut PgConnection, payment_id: i64) -> Result<()> {
    let row = sqlx::query("SELECT p.id, p.currency AS primary_currency, pay.currency,
            round(pay.amount_minor::numeric*p.share_percent/100)::bigint AS commission
        FROM payments pay JOIN clients c ON c.id=pay.client_id
        JOIN partners p ON p.id=c.referred_by
        WHERE pay.id=$1 AND pay.status='success' AND pay.amount_minor>0 AND p.is_active
          AND p.client_id IS DISTINCT FROM c.id FOR UPDATE OF p")
        .bind(payment_id).fetch_optional(&mut *conn).await?;
    let Some(r) = row else { return Ok(()) };
    let id: i64 = r.get("id");
    let amount: i64 = r.get("commission");
    let currency: String = r.get("currency");
    let inserted = sqlx::query("INSERT INTO partner_commissions
            (partner_id,payment_id,amount_minor,base_amount_minor,currency) VALUES($1,$2,$3,$3,$4)
            ON CONFLICT(payment_id) DO NOTHING")
        .bind(id).bind(payment_id).bind(amount).bind(&currency).execute(&mut *conn).await?;
    if inserted.rows_affected() > 0 && currency == r.get::<String,_>("primary_currency") {
        sqlx::query("UPDATE partners SET balance_minor=balance_minor+$2 WHERE id=$1")
            .bind(id).bind(amount).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Called after the refund total is updated, with the payment row still locked.
pub async fn adjust_refund(conn: &mut PgConnection, payment_id: i64) -> Result<()> {
    let row = sqlx::query("SELECT p.id,p.currency AS primary_currency,pc.currency,pc.amount_minor,
            round(pc.base_amount_minor::numeric * GREATEST(0,pay.amount_minor-
                COALESCE((pay.metadata->>'refunded_minor')::bigint,0)) / NULLIF(pay.amount_minor,0))::bigint AS remaining
        FROM partner_commissions pc JOIN partners p ON p.id=pc.partner_id
        JOIN payments pay ON pay.id=pc.payment_id WHERE pc.payment_id=$1 FOR UPDATE OF p,pc")
        .bind(payment_id).fetch_optional(&mut *conn).await?;
    let Some(r) = row else { return Ok(()) };
    let remaining: i64 = r.get::<Option<i64>,_>("remaining").unwrap_or(0);
    let delta = r.get::<i64,_>("amount_minor") - remaining;
    sqlx::query("UPDATE partner_commissions SET amount_minor=$2 WHERE payment_id=$1")
        .bind(payment_id).bind(remaining).execute(&mut *conn).await?;
    if r.get::<String,_>("currency") == r.get::<String,_>("primary_currency") {
        // A refund after a payout creates a debt, deducted from future earnings.
        sqlx::query("UPDATE partners SET balance_minor=balance_minor-$2 WHERE id=$1")
            .bind(r.get::<i64,_>("id")).bind(delta).execute(&mut *conn).await?;
    }
    Ok(())
}

/// One referral account per client, shared by the bot and Mini App.
pub async fn ensure(pool: &crate::Pool, client_id: i64, percent: f64) -> Result<Option<(i64,String,f64)>> {
    let currency = crate::money::service_currency(pool).await;
    let mut tx = pool.begin().await?;
    let name: Option<String> = sqlx::query_scalar("SELECT username FROM clients WHERE id=$1 AND deleted_at IS NULL FOR UPDATE")
        .bind(client_id).fetch_optional(&mut *tx).await?;
    let Some(name) = name else { return Ok(None) };
    let existing = sqlx::query("SELECT id,slug,share_percent::float8 AS pct FROM partners WHERE client_id=$1 ORDER BY id LIMIT 1")
        .bind(client_id).fetch_optional(&mut *tx).await?;
    if let Some(r) = existing {
        tx.commit().await?;
        return Ok(Some((r.get("id"),r.get("slug"),r.get("pct"))));
    }
    // An administrator may already use c{id} for a channel. Never take over its slug.
    let slug = format!("c{client_id}_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let id: i64 = sqlx::query_scalar("INSERT INTO partners(client_id,title,slug,share_percent,currency) VALUES($1,$2,$3,$4,$5) RETURNING id")
        .bind(client_id).bind(name).bind(&slug).bind(percent).bind(currency).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some((id,slug,percent)))
}

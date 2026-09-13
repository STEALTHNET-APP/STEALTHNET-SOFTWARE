//! Shared addon catalog and entitlement lifecycle for Telegram and Mini App.
use crate::{Error, Pool, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, Row};

#[derive(Debug, Deserialize, Serialize)]
pub struct Package {
    #[serde(default)] pub id: Option<i64>,
    pub kind: String,
    pub quantity: i32,
    pub currency: String,
    pub amount_minor: i64,
    pub stars_minor: Option<i64>,
    #[serde(default = "yes")] pub is_active: bool,
}
fn yes() -> bool { true }
impl Package {
    pub fn title(&self) -> String {
        if self.kind == "traffic" { format!("+{} ГБ трафика", self.quantity) }
        else { format!("+{} {}", self.quantity, device_word(self.quantity)) }
    }
    pub fn validate(&self, currency: &str) -> Result<()> {
        if !matches!(self.kind.as_str(), "traffic" | "devices") || self.quantity < 1
            || self.quantity > if self.kind == "devices" {100} else {100000}
            || self.amount_minor < 1 || self.amount_minor > 100_000_000_000
            || self.stars_minor.is_some_and(|n| n < 1 || n > 1_000_000_000)
            || self.currency != currency { return Err(Error::bad("проверьте объём и цену докупки в валюте проекта")); }
        Ok(())
    }
}
fn device_word(n: i32) -> &'static str {
    if (11..=14).contains(&(n%100)) { "устройств" }
    else { match n%10 { 1=>"устройство",2..=4=>"устройства",_=>"устройств" } }
}
pub async fn set_catalog(conn: &mut PgConnection, tariff: i64, packs: &[Package], currency: &str) -> Result<()> {
    if packs.len() > 30 { return Err(Error::bad("не больше 30 пакетов на тариф")); }
    let mut ids = std::collections::HashSet::new();
    for p in packs { p.validate(currency)?; if p.id.is_some_and(|id| !ids.insert(id)) {return Err(Error::bad("повторяющийся пакет"));} }
    // Removed packages are retired, so previously issued invoices remain readable.
    sqlx::query("UPDATE tariff_addons SET is_active=false WHERE tariff_id=$1").bind(tariff).execute(&mut *conn).await?;
    for (order,p) in packs.iter().enumerate() {
        if let Some(id) = p.id {
            let n = sqlx::query("UPDATE tariff_addons SET kind=$3,quantity=$4,currency=$5,amount_minor=$6,stars_minor=$7,is_active=$8,sort_order=$9 WHERE id=$1 AND tariff_id=$2")
                .bind(id).bind(tariff).bind(&p.kind).bind(p.quantity).bind(&p.currency).bind(p.amount_minor).bind(p.stars_minor).bind(p.is_active).bind(order as i32).execute(&mut *conn).await?;
            if n.rows_affected()!=1 {return Err(Error::bad("пакет не принадлежит этому тарифу"));}
        } else {
            sqlx::query("INSERT INTO tariff_addons(tariff_id,kind,quantity,currency,amount_minor,stars_minor,is_active,sort_order) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(tariff).bind(&p.kind).bind(p.quantity).bind(&p.currency).bind(p.amount_minor).bind(p.stars_minor).bind(p.is_active).bind(order as i32).execute(&mut *conn).await?;
        }
    }
    Ok(())
}
pub async fn catalog(pool: &Pool, client: i64) -> Result<Value> {
    refresh(pool, client).await?;
    let cur = crate::money::service_currency(pool).await;
    let rows = sqlx::query("SELECT a.*, s.expires_at, CASE WHEN a.kind='traffic' THEN LEAST(s.expires_at,s.traffic_reset_at) ELSE s.expires_at END AS until
        FROM tariff_addons a JOIN tariffs t ON t.id=a.tariff_id JOIN subscriptions s ON s.tariff_id=t.id AND s.is_current JOIN clients c ON c.id=s.client_id
        WHERE s.client_id=$1 AND c.deleted_at IS NULL AND c.status NOT IN ('disabled') AND s.canceled_at IS NULL
        AND (s.expires_at IS NULL OR s.expires_at>now()) AND t.is_active AND a.is_active AND a.currency=$2
        AND (a.kind='devices' OR s.traffic_limit_bytes IS NOT NULL) ORDER BY a.sort_order,a.id")
        .bind(client).bind(&cur).fetch_all(pool).await?;
    let items: Vec<Value> = rows.iter().map(|r| {
        let p=Package {id:Some(r.get("id")),kind:r.get("kind"),quantity:r.get("quantity"),currency:cur.clone(),amount_minor:r.get("amount_minor"),stars_minor:r.get("stars_minor"),is_active:true};
        json!({"id":p.id,"kind":p.kind,"quantity":p.quantity,"title":p.title(),"amount_minor":p.amount_minor,"stars_minor":p.stars_minor,"currency":cur,"expires_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("until")})
    }).collect();
    let grants: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'quantity',CASE WHEN kind='traffic' THEN quantity/1073741824 ELSE quantity END,'expires_at',expires_at,'pending',activated_at IS NULL) FROM subscription_addons WHERE client_id=$1 AND ended_at IS NULL ORDER BY id")
        .bind(client).fetch_all(pool).await?;
    Ok(json!({"items":items,"currency":cur,"grants":grants}))
}

/// Caller always locks the client before touching its subscription, including worker tasks.
pub async fn maintain(conn: &mut PgConnection, client: i64, reset: bool) -> Result<()> {
    sqlx::query("SELECT id FROM clients WHERE id=$1 FOR UPDATE").bind(client).fetch_optional(&mut *conn).await?;
    sqlx::query("SELECT id FROM subscriptions WHERE client_id=$1 AND is_current FOR UPDATE").bind(client).fetch_optional(&mut *conn).await?;
    // An explicit reset also consumes traffic addons. Normal expiry never clears usage.
    sqlx::query("WITH ended AS (UPDATE subscription_addons a SET ended_at=now() WHERE client_id=$1 AND ended_at IS NULL AND activated_at IS NOT NULL AND
        (expires_at<=now() OR ($2 AND kind='traffic') OR (kind='traffic' AND EXISTS(SELECT 1 FROM subscriptions s WHERE s.id=a.subscription_id AND s.reset_strategy<>'no_reset' AND s.traffic_reset_at<=now())))
        RETURNING subscription_id,kind,quantity), totals AS (SELECT subscription_id, COALESCE(sum(quantity) FILTER(WHERE kind='devices'),0)::int AS devices, COALESCE(sum(quantity) FILTER(WHERE kind='traffic'),0)::bigint AS traffic FROM ended GROUP BY subscription_id)
        UPDATE subscriptions s SET device_limit=GREATEST(1,s.device_limit-t.devices),traffic_limit_bytes=CASE WHEN s.traffic_limit_bytes IS NULL THEN NULL ELSE GREATEST(0,s.traffic_limit_bytes-t.traffic) END FROM totals t WHERE s.id=t.subscription_id")
        .bind(client).bind(reset).execute(&mut *conn).await?;
    // Advance over missed cycles in one pass while holding the same subscription lock.
    let due:Option<(i64,String,Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as("SELECT id,reset_strategy::text,traffic_reset_at FROM subscriptions WHERE client_id=$1 AND is_current AND ($2 OR (reset_strategy<>'no_reset' AND (traffic_reset_at<=now() OR traffic_reset_at IS NULL)))")
        .bind(client).bind(reset).fetch_optional(&mut *conn).await?;
    if let Some((id,strategy,at))=due {
        let now=chrono::Utc::now(); let mut next=at.unwrap_or(now);
        if strategy!="no_reset" {
            loop { next=match strategy.as_str(){"day"=>next+chrono::Duration::days(1),"week"=>next+chrono::Duration::days(7),_=>next.checked_add_months(chrono::Months::new(1)).unwrap_or(now+chrono::Duration::days(30))}; if next>now{break;} }
        }
        sqlx::query("UPDATE subscriptions SET traffic_used_bytes=CASE WHEN $2 THEN 0 ELSE traffic_used_bytes END,traffic_reset_at=$3 WHERE id=$1")
            .bind(id).bind(reset || at.is_some()).bind(if strategy=="no_reset"{None}else{Some(next)}).execute(&mut *conn).await?;
    }
    activate_pending(conn,client).await?;
    sqlx::query("UPDATE clients c SET status='active' FROM subscriptions s WHERE c.id=$1 AND s.client_id=c.id AND s.is_current AND c.status='limited' AND (s.expires_at IS NULL OR s.expires_at>now()) AND (s.traffic_limit_bytes IS NULL OR s.traffic_used_bytes<s.traffic_limit_bytes)")
        .bind(client).execute(&mut *conn).await?;
    Ok(())
}
pub async fn refresh(pool:&Pool,client:i64)->Result<()> { let mut tx=pool.begin().await?;maintain(&mut tx,client,false).await?;tx.commit().await?;Ok(()) }

pub async fn activate_pending(conn:&mut PgConnection,client:i64)->Result<()> {
    sqlx::query("WITH activated AS (UPDATE subscription_addons a SET subscription_id=s.id,activated_at=now(),expires_at=CASE WHEN a.kind='traffic' THEN LEAST(s.expires_at,s.traffic_reset_at) ELSE s.expires_at END
        FROM subscriptions s, clients c WHERE a.client_id=$1 AND a.activated_at IS NULL AND a.ended_at IS NULL AND s.client_id=a.client_id AND s.is_current AND c.id=s.client_id AND c.deleted_at IS NULL AND c.status<>'disabled'
        AND s.canceled_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>now()) AND (a.kind='devices' OR s.traffic_limit_bytes IS NOT NULL) RETURNING s.id,a.kind,a.quantity),
        totals AS (SELECT id,COALESCE(sum(quantity) FILTER(WHERE kind='devices'),0)::int AS devices,COALESCE(sum(quantity) FILTER(WHERE kind='traffic'),0)::bigint AS traffic FROM activated GROUP BY id)
        UPDATE subscriptions s SET device_limit=s.device_limit+t.devices,traffic_limit_bytes=s.traffic_limit_bytes+t.traffic FROM totals t WHERE s.id=t.id")
        .bind(client).execute(&mut *conn).await?;
    Ok(())
}
/// After replacing the base tariff limits, restore paid, unexpired grants without extending their expiry.
pub async fn restore_after_plan(conn:&mut PgConnection,client:i64)->Result<()> {
    sqlx::query("UPDATE subscriptions s SET device_limit=s.device_limit+COALESCE((SELECT sum(quantity)::int FROM subscription_addons a WHERE a.subscription_id=s.id AND a.ended_at IS NULL AND a.activated_at IS NOT NULL AND a.kind='devices'),0),traffic_limit_bytes=s.traffic_limit_bytes+COALESCE((SELECT sum(quantity)::bigint FROM subscription_addons a WHERE a.subscription_id=s.id AND a.ended_at IS NULL AND a.activated_at IS NOT NULL AND a.kind='traffic'),0) WHERE s.client_id=$1 AND s.is_current")
        .bind(client).execute(&mut *conn).await?;
    activate_pending(conn,client).await
}

#[cfg(test)] mod tests { use super::*;
 #[test] fn validate_packages(){let mut p=Package{id:None,kind:"devices".into(),quantity:1,currency:"USD".into(),amount_minor:199,stars_minor:None,is_active:true};assert!(p.validate("USD").is_ok());p.quantity=101;assert!(p.validate("USD").is_err());p.quantity=2;assert!(p.validate("RUB").is_err());p.amount_minor=0;assert!(p.validate("USD").is_err());assert_eq!(p.title(),"+2 устройства");}
}

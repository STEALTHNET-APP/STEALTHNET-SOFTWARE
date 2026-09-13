use crate::{Registry, Invoice, InvoiceRequest};
use sn_core::{Error,Pool,Result};
use serde_json::{json,Value};
use sqlx::Row;

impl Registry {
    /// Both customer surfaces call this method; no caller-supplied price or entitlement.
    pub async fn create_addon_payment(&self,pool:&Pool,client:i64,package:i64,provider_id:&str,currency:&str)->Result<(i64,Invoice)> {
        self.create_addon_payment_returning(pool,client,package,provider_id,currency,None).await
    }
    pub async fn create_addon_payment_returning(&self,pool:&Pool,client:i64,package:i64,provider_id:&str,currency:&str,return_url:Option<String>)->Result<(i64,Invoice)> {
        sn_core::money::check_purchase_currency(pool,currency,provider_id).await?;
        let owner_currency=sn_core::money::service_currency(pool).await;
        let provider=self.enabled_for_currency(pool,currency).await.into_iter().find(|p|p.id()==provider_id).ok_or_else(||Error::bad("способ оплаты недоступен"))?;
        let mut tx=pool.begin().await?;
        sn_core::addons::maintain(&mut tx,client,false).await?;
        let row=sqlx::query("SELECT a.*,s.id AS subscription_id FROM tariff_addons a JOIN tariffs t ON t.id=a.tariff_id JOIN subscriptions s ON s.tariff_id=t.id AND s.is_current JOIN clients c ON c.id=s.client_id
            WHERE a.id=$1 AND s.client_id=$2 AND a.is_active AND t.is_active AND c.deleted_at IS NULL AND c.status<>'disabled' AND s.canceled_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>now()) AND a.currency=$3
            AND (a.kind='devices' OR s.traffic_limit_bytes IS NOT NULL)")
            .bind(package).bind(client).bind(&owner_currency).fetch_optional(&mut *tx).await?.ok_or_else(||Error::bad("докупка недоступна: проверьте действующий тариф"))?;
        let pack=sn_core::addons::Package{id:Some(package),kind:row.get("kind"),quantity:row.get("quantity"),currency:row.get("currency"),amount_minor:row.get("amount_minor"),stars_minor:row.get("stars_minor"),is_active:true};
        let amount=if currency==owner_currency {pack.amount_minor} else if currency=="XTR" {pack.stars_minor.ok_or_else(||Error::bad("цена в Stars не задана"))?}else{return Err(Error::bad("валюта не поддерживается"));};
        let title=pack.title();
        // Lock on the client serializes both bot and API checkouts. A link still being issued is never duplicated.
        if let Some(existing)=sqlx::query("SELECT id,pay_url,provider_txid,provider_payload FROM payments WHERE client_id=$1 AND kind='addon' AND status='pending' AND expires_at>now() AND provider=$2 AND currency=$3 AND amount_minor=$4 AND addon_snapshot->>'package_id'=$5 AND addon_snapshot->>'kind'=$6 AND (addon_snapshot->>'units')::int=$7 ORDER BY id DESC LIMIT 1")
            .bind(client).bind(provider_id).bind(currency).bind(amount).bind(package.to_string()).bind(&pack.kind).bind(pack.quantity).fetch_optional(&mut *tx).await? {
            let url:Option<String>=existing.get("pay_url");
            let Some(url)=url else{return Err(Error::bad("счёт уже создаётся, повторите через несколько секунд"));};
            let id=existing.get("id");let invoice=Invoice{pay_url:url,external_id:existing.get("provider_txid"),expires_in_minutes:60,payload:existing.get::<Option<Value>,_>("provider_payload").unwrap_or(Value::Null)};
            tx.commit().await?;return Ok((id,invoice));
        }
        let quantity=if pack.kind=="traffic"{i64::from(pack.quantity)*1073741824}else{i64::from(pack.quantity)};
        let snapshot=json!({"package_id":package,"kind":pack.kind,"quantity":quantity,"units":pack.quantity,"title":title});
        let id:i64=sqlx::query_scalar("INSERT INTO payments(client_id,subscription_id,tariff_id,kind,status,amount_minor,currency,provider,addon_snapshot,expires_at) VALUES($1,$2,$3,'addon','pending',$4,$5,$6,$7,now()+interval '1 hour') RETURNING id")
            .bind(client).bind(row.get::<i64,_>("subscription_id")).bind(row.get::<i64,_>("tariff_id")).bind(amount).bind(currency).bind(provider_id).bind(snapshot).fetch_one(&mut *tx).await?;
        let telegram_id=sqlx::query_scalar::<_,String>("SELECT value FROM client_identities WHERE client_id=$1 AND kind='telegram'").bind(client).fetch_optional(&mut *tx).await?.and_then(|v|v.parse().ok());
        tx.commit().await?;
        let req=InvoiceRequest{payment_id:id,client_id:client,telegram_id,amount_minor:amount,currency:currency.into(),description:title,return_url};
        match provider.create_invoice(&req).await {
            Ok(invoice)=>{
                sqlx::query("UPDATE payments SET pay_url=$2,provider_txid=COALESCE($3,provider_txid),provider_payload=$4,expires_at=now()+($5||' minutes')::interval WHERE id=$1")
                    .bind(id).bind(&invoice.pay_url).bind(&invoice.external_id).bind(&invoice.payload).bind(invoice.expires_in_minutes.to_string()).execute(pool).await?;
                Ok((id,invoice))
            },Err(e)=>{sqlx::query("UPDATE payments SET status='failed',error_message=$2 WHERE id=$1 AND status='pending'").bind(id).bind(e.to_string()).execute(pool).await?;Err(e)}
        }
    }
}

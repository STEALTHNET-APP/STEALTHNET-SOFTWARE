use sn_core::{addons,Result,Pool};
use sn_payments::{Registry,WebhookOutcome,PaymentStatus};
use serde_json::Value;
const GB:i64=1073741824;
async fn snapshot(pool:&Pool,client:i64)->(i32,i64,i64,chrono::DateTime<chrono::Utc>){sqlx::query_as("SELECT device_limit,traffic_limit_bytes,traffic_used_bytes,expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(pool).await.unwrap()}
async fn settle(reg:&Registry,pool:&Pool,id:i64)->Result<()> {reg.apply_outcome(pool,"manual",&WebhookOutcome{external_event_id:None,payment_id:Some(id),provider_txid:None,status:PaymentStatus::Success,amount_minor:None,currency:Some("USD".into()),error:None,raw:Value::Null}).await?;Ok(())}
#[tokio::test]
async fn addon_checkout_fulfillment_and_lifecycle()->Result<()> {
    let Ok(url)=std::env::var("SN_ADDON_TEST_DB") else{return Ok(())};
    let pool=sn_core::db::connect(&url).await?;
    let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await?;assert!(db.starts_with("sn_audit_"),"test database required");
    let reg=Registry::from_env();reg.sync_to_db(&pool).await?;
    sqlx::query("UPDATE payment_providers SET is_enabled=true,enabled_currencies=ARRAY['USD'] WHERE id='manual'").execute(&pool).await?;
    sqlx::query("INSERT INTO settings(key,value) VALUES('billing.currency','\"USD\"') ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value").execute(&pool).await?;
    let tag=uuid::Uuid::new_v4().simple().to_string();
    let tariff:i64=sqlx::query_scalar("INSERT INTO tariffs(code,title,device_limit,traffic_limit_bytes,reset_strategy) VALUES($1,'Addon test',2,$2,'month') RETURNING id").bind(&tag).bind(100*GB).fetch_one(&pool).await?;
    let client:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES('qa_addons',$1) RETURNING id").bind(&tag).fetch_one(&pool).await?;
    let sub:i64=sqlx::query_scalar("INSERT INTO subscriptions(client_id,tariff_id,expires_at,device_limit,traffic_limit_bytes,traffic_used_bytes,traffic_reset_at) VALUES($1,$2,now()+interval '30 days',2,$3,$4,now()+interval '7 days') RETURNING id").bind(client).bind(tariff).bind(100*GB).bind(80*GB).fetch_one(&pool).await?;
    let mut conn=pool.acquire().await?;
    addons::set_catalog(&mut conn,tariff,&[
        addons::Package{id:None,kind:"traffic".into(),quantity:50,currency:"USD".into(),amount_minor:299,stars_minor:None,is_active:true},
        addons::Package{id:None,kind:"devices".into(),quantity:2,currency:"USD".into(),amount_minor:199,stars_minor:None,is_active:true}],"USD").await?;
    drop(conn);
    let ids:Vec<i64>=sqlx::query_scalar("SELECT id FROM tariff_addons WHERE tariff_id=$1 ORDER BY id").bind(tariff).fetch_all(&pool).await?;
    let initial=snapshot(&pool,client).await;
    assert_eq!(addons::catalog(&pool,client).await?["items"].as_array().unwrap().len(),2);
    let (traffic,_)=reg.create_addon_payment(&pool,client,ids[0],"manual","USD").await?;
    let (same,_)=reg.create_addon_payment(&pool,client,ids[0],"manual","USD").await?;assert_eq!(traffic,same);
    assert_eq!(snapshot(&pool,client).await,initial,"pending must not grant resources");
    assert!(reg.create_addon_payment(&pool,client,ids[0],"manual","EUR").await.is_err());
    let mut underpay=WebhookOutcome{external_event_id:None,payment_id:Some(traffic),provider_txid:None,status:PaymentStatus::Success,amount_minor:Some(1),currency:Some("USD".into()),error:None,raw:Value::Null};
    assert!(reg.apply_outcome(&pool,"manual",&underpay).await.is_err());assert_eq!(snapshot(&pool,client).await,initial);
    underpay.amount_minor=Some(299);reg.apply_outcome(&pool,"manual",&underpay).await?;
    settle(&reg,&pool,traffic).await?;
    let got=snapshot(&pool,client).await;assert_eq!(got,(2,150*GB,80*GB,initial.3));
    let (device,_)=reg.create_addon_payment(&pool,client,ids[1],"manual","USD").await?;
    // Both package deletion and repricing must leave existing invoices valid.
    sqlx::query("UPDATE tariff_addons SET amount_minor=999,quantity=10,is_active=false WHERE id=$1").bind(ids[1]).execute(&pool).await?;
    let (a,b)=tokio::join!(settle(&reg,&pool,device),settle(&reg,&pool,device));a?;b?;
    assert_eq!(snapshot(&pool,client).await,(4,150*GB,80*GB,initial.3));
    let device_expiry:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscription_addons WHERE payment_id=$1").bind(device).fetch_one(&pool).await?;
    let payment:i64=sqlx::query_scalar("INSERT INTO payments(client_id,tariff_id,kind,status,amount_minor,currency,period_days,provider) VALUES($1,$2,'renewal','pending',100,'USD',30,'manual') RETURNING id").bind(client).bind(tariff).fetch_one(&pool).await?;
    settle(&reg,&pool,payment).await?;
    let after=snapshot(&pool,client).await;assert_eq!((after.0,after.1,after.2),(4,150*GB,80*GB));assert!(after.3>initial.3);
    let expiry:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscription_addons WHERE payment_id=$1").bind(device).fetch_one(&pool).await?;assert_eq!(expiry,device_expiry);
    // Passing the addon deadline after an early renewal removes only the extra slots.
    sqlx::query("UPDATE subscription_addons SET expires_at=now()-interval '1 minute' WHERE payment_id=$1").bind(device).execute(&pool).await?;
    addons::refresh(&pool,client).await?;addons::refresh(&pool,client).await?;
    assert_eq!(snapshot(&pool,client).await.0,2);
    // A scheduled reset removes extra traffic and resets usage once, leaving base limits.
    sqlx::query("UPDATE subscriptions SET traffic_reset_at=now()-interval '70 days' WHERE id=$1").bind(sub).execute(&pool).await?;
    addons::refresh(&pool,client).await?;let got=snapshot(&pool,client).await;assert_eq!((got.0,got.1,got.2),(2,100*GB,0));
    let next:bool=sqlx::query_scalar("SELECT traffic_reset_at>now() FROM subscriptions WHERE id=$1").bind(sub).fetch_one(&pool).await?;assert!(next);
    // Limited customers can restore service with a traffic package.
    sqlx::query("UPDATE subscriptions SET traffic_used_bytes=traffic_limit_bytes WHERE id=$1").bind(sub).execute(&pool).await?;
    sqlx::query("UPDATE clients SET status='limited' WHERE id=$1").bind(client).execute(&pool).await?;
    let (topup,_)=reg.create_addon_payment(&pool,client,ids[0],"manual","USD").await?;settle(&reg,&pool,topup).await?;
    let status:String=sqlx::query_scalar("SELECT status::text FROM clients WHERE id=$1").bind(client).fetch_one(&pool).await?;assert_eq!(status,"active");
    // Manual resets consume extra traffic just like scheduled resets.
    let mut tx=pool.begin().await?;addons::maintain(&mut tx,client,true).await?;tx.commit().await?;assert_eq!(snapshot(&pool,client).await.1,100*GB);
    // Paid after expiry: saved pending, no accidental renewal. Activated on next paid term.
    let (late,_)=reg.create_addon_payment(&pool,client,ids[0],"manual","USD").await?;
    sqlx::query("UPDATE subscriptions SET expires_at=now()-interval '1 day' WHERE id=$1").bind(sub).execute(&pool).await?;
    assert!(reg.create_addon_payment(&pool,client,ids[0],"manual","USD").await.is_err());
    settle(&reg,&pool,late).await?;let pending:bool=sqlx::query_scalar("SELECT activated_at IS NULL FROM subscription_addons WHERE payment_id=$1").bind(late).fetch_one(&pool).await?;assert!(pending);
    let payment:i64=sqlx::query_scalar("INSERT INTO payments(client_id,tariff_id,kind,status,amount_minor,currency,period_days,provider) VALUES($1,$2,'renewal','pending',100,'USD',30,'manual') RETURNING id").bind(client).bind(tariff).fetch_one(&pool).await?;
    settle(&reg,&pool,payment).await?;assert_eq!(snapshot(&pool,client).await.1,150*GB);
    // Unlimited subscriptions do not offer or accept traffic addons, including stale direct requests.
    sqlx::query("UPDATE subscriptions SET traffic_limit_bytes=NULL WHERE id=$1").bind(sub).execute(&pool).await?;
    assert!(addons::catalog(&pool,client).await?["items"].as_array().unwrap().is_empty());
    assert!(reg.create_addon_payment(&pool,client,ids[0],"manual","USD").await.is_err());
    sqlx::query("UPDATE subscription_addons SET expires_at=now()-interval '1 minute' WHERE client_id=$1 AND kind='traffic'").bind(client).execute(&pool).await?;
    addons::refresh(&pool,client).await?;
    let limit:Option<i64>=sqlx::query_scalar("SELECT traffic_limit_bytes FROM subscriptions WHERE id=$1").bind(sub).fetch_one(&pool).await?;assert_eq!(limit,None,"unlimited stays unlimited after expiry");
    sqlx::query("UPDATE clients SET deleted_at=now() WHERE id=$1").bind(client).execute(&pool).await?;
    sqlx::query("UPDATE tariffs SET is_active=false,is_visible=false WHERE id=$1").bind(tariff).execute(&pool).await?;
    println!("Addon lifecycle: dedup, amount, snapshot, concurrency, renewal, expiry, reset, limited, late payment, unlimited passed");
    Ok(())
}

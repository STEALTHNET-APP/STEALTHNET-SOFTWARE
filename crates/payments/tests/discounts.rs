use sn_payments::{Registry,PaymentStatus,WebhookOutcome};
use serde_json::Value;
use sqlx::Row;

#[tokio::test]
#[ignore = "Requires SN_TEST_DATABASE_URL pointing to an isolated audit database"]
async fn discounts_and_free_periods_are_atomic_and_bound_to_the_invoice() {
    let pool=sqlx::PgPool::connect(&std::env::var("SN_TEST_DATABASE_URL").unwrap()).await.unwrap();
    let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
    assert!(db.starts_with("sn_audit_"));
    let key=uuid::Uuid::new_v4().simple().to_string();
    let tariff:i64=sqlx::query_scalar("INSERT INTO tariffs(code,title) VALUES($1,$1) RETURNING id").bind(&key).fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO tariff_prices(tariff_id,period_days,currency,amount_minor) VALUES($1,3,'USD',0)").bind(tariff).execute(&pool).await.unwrap();
    let mut clients=Vec::new();
    for i in 0..2 {
        let id:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES($1,$1) RETURNING id").bind(format!("{key}_{i}")).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO subscriptions(client_id,tariff_id,expires_at) VALUES($1,$2,now())").bind(id).bind(tariff).execute(&pool).await.unwrap();clients.push(id);
    }
    let (a,b)=tokio::join!(sn_core::billing::activate_free(&pool,clients[0],tariff,3),sn_core::billing::activate_free(&pool,clients[0],tariff,3));
    assert_eq!(usize::from(a.is_ok())+usize::from(b.is_ok()),1,"only one concurrent free activation");
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM payments WHERE client_id=$1 AND tariff_id=$2").bind(clients[0]).bind(tariff).fetch_one(&pool).await.unwrap();assert_eq!(count,1);
    let reset:bool=sqlx::query_scalar("SELECT traffic_reset_at>now() FROM subscriptions WHERE client_id=$1 AND is_current").bind(clients[0]).fetch_one(&pool).await.unwrap();assert!(reset);
    let promo:i64=sqlx::query_scalar("INSERT INTO promo_codes(code,kind,value,max_uses) VALUES($1,'percent',25,1) RETURNING id").bind(&key).fetch_one(&pool).await.unwrap();
    let reg=Registry::from_env();reg.sync_to_db(&pool).await.unwrap();
    sqlx::query("UPDATE payment_providers SET is_enabled=true,enabled_currencies=NULL WHERE id='manual'").execute(&pool).await.unwrap();
    let (a,b)=tokio::join!(
        reg.create_payment(&pool,"manual",clients[0],tariff,30,101,"USD","QA",None,Some(promo)),
        reg.create_payment(&pool,"manual",clients[1],tariff,30,101,"USD","QA",None,Some(promo)));
    assert_eq!(usize::from(a.is_ok())+usize::from(b.is_ok()),1,"one remaining promo use cannot be oversold");
    let pid=a.or(b).unwrap().0;
    let r=sqlx::query("SELECT amount_minor,discount_minor,promo_code_id FROM payments WHERE id=$1").bind(pid).fetch_one(&pool).await.unwrap();
    assert_eq!(r.get::<i64,_>("amount_minor"),76);assert_eq!(r.get::<i64,_>("discount_minor"),25);assert_eq!(r.get::<i64,_>("promo_code_id"),promo);
    let outcome=WebhookOutcome{external_event_id:None,payment_id:Some(pid),provider_txid:None,status:PaymentStatus::Success,amount_minor:Some(76),currency:Some("USD".into()),error:None,raw:Value::Null};
    reg.apply_outcome(&pool,"manual",&outcome).await.unwrap();reg.apply_outcome(&pool,"manual",&outcome).await.unwrap();
    let used:i32=sqlx::query_scalar("SELECT used_count FROM promo_codes WHERE id=$1").bind(promo).fetch_one(&pool).await.unwrap();assert_eq!(used,1);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM promo_redemptions WHERE payment_id=$1").bind(pid).fetch_one(&pool).await.unwrap();assert_eq!(count,1);
    let zero:i64=sqlx::query_scalar("INSERT INTO promo_codes(code,kind,value,max_uses) VALUES($1,'percent',100,1) RETURNING id").bind(format!("{key}z")).fetch_one(&pool).await.unwrap();
    let (free,invoice)=reg.create_payment(&pool,"manual",clients[0],tariff,30,500,"USD","QA",None,Some(zero)).await.unwrap();
    assert_eq!(invoice.payload["free"],true);
    let success:bool=sqlx::query_scalar("SELECT status='success' AND amount_minor=0 FROM payments WHERE id=$1").bind(free).fetch_one(&pool).await.unwrap();assert!(success);
    sqlx::query("UPDATE tariffs SET is_active=false WHERE id=$1").bind(tariff).execute(&pool).await.unwrap();
    assert!(reg.create_payment(&pool,"manual",clients[1],tariff,30,500,"USD","QA",None,None).await.is_err());
    assert!(sn_core::billing::activate_free(&pool,clients[1],tariff,3).await.is_err());
    sqlx::query("DELETE FROM promo_redemptions WHERE promo_code_id=ANY($1)").bind(vec![promo,zero]).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM payments WHERE client_id=ANY($1)").bind(&clients).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM clients WHERE id=ANY($1)").bind(&clients).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM promo_codes WHERE id=ANY($1)").bind(vec![promo,zero]).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM tariffs WHERE id=$1").bind(tariff).execute(&pool).await.unwrap();
}

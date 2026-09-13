use serde_json::json;
use sn_payments::providers::stars::validate_checkout;

#[tokio::test]
#[ignore = "Requires SN_TEST_DATABASE_URL pointing to an isolated audit database"]
async fn stars_checkout_validates_invoice_before_charge() {
    let pool=sqlx::PgPool::connect(&std::env::var("SN_TEST_DATABASE_URL").unwrap()).await.unwrap();
    let db: String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
    assert!(db.starts_with("sn_audit_"),"refusing to write outside an isolated audit database");
    let slug=uuid::Uuid::new_v4().simple().to_string();
    let cid:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES($1,$1) RETURNING id")
        .bind(&slug).fetch_one(&pool).await.unwrap();
    let tg=(9_000_000_000i64+cid).to_string();
    sqlx::query("INSERT INTO client_identities(client_id,kind,value) VALUES($1,'telegram',$2)").bind(cid).bind(&tg).execute(&pool).await.unwrap();
    let pid:i64=sqlx::query_scalar("INSERT INTO payments(client_id,amount_minor,currency,provider,status,expires_at) VALUES($1,50,'XTR','stars','pending',now()+interval '1 hour') RETURNING id")
        .bind(cid).fetch_one(&pool).await.unwrap();
    let q=json!({"invoice_payload":pid.to_string(),"total_amount":50,"currency":"XTR","from":{"id":tg.parse::<i64>().unwrap()}});
    assert!(validate_checkout(&pool,&q).await.unwrap());
    for (key,value) in [("invoice_payload",json!("bad")),("total_amount",json!(49)),("total_amount",json!(0)),("currency",json!("USD")),("from",json!({"id":1}))] {
        let mut bad=q.clone();bad[key]=value;
        assert!(!validate_checkout(&pool,&bad).await.unwrap(),"{key}={}",bad[key]);
    }
    sqlx::query("UPDATE payments SET provider='manual' WHERE id=$1").bind(pid).execute(&pool).await.unwrap();
    assert!(!validate_checkout(&pool,&q).await.unwrap());
    sqlx::query("UPDATE payments SET provider='stars',status='success' WHERE id=$1").bind(pid).execute(&pool).await.unwrap();
    assert!(!validate_checkout(&pool,&q).await.unwrap());
    sqlx::query("UPDATE payments SET status='pending',expires_at=now()-interval '1 second' WHERE id=$1").bind(pid).execute(&pool).await.unwrap();
    assert!(!validate_checkout(&pool,&q).await.unwrap());
    sqlx::query("DELETE FROM payments WHERE id=$1").bind(pid).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM clients WHERE id=$1").bind(cid).execute(&pool).await.unwrap();
}

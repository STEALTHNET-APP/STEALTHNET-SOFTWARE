use super::*;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
#[ignore = "Requires SN_TEST_DATABASE_URL pointing to an isolated audit database"]
async fn free_access_without_purchases_or_payment_providers() {
    let url = std::env::var("SN_TEST_DATABASE_URL").unwrap();
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&admin).await.unwrap();
    assert!(database.contains("audit"), "use an isolated audit database");
    let schema = format!("checkout_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
    for table in ["settings", "tariffs", "tariff_prices", "payments", "clients", "subscriptions", "subscription_addons", "client_squads", "tariff_squads", "client_identities", "promo_codes", "payment_providers"] {
        sqlx::query(&format!("CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)"))
            .execute(&admin).await.unwrap();
    }
    let search_path = format!("SET search_path TO {schema}, public");
    let pool = PgPoolOptions::new().max_connections(6).after_connect(move |conn, _| {
        let sql = search_path.clone();
        Box::pin(async move { sqlx::query(&sql).execute(conn).await?; Ok(()) })
    }).connect(&url).await.unwrap();
    // Catch assertion panics so even a failed check cleans up its private schema.
    let task_pool = pool.clone();
    let result = tokio::spawn(async move { check_checkout(task_pool).await }).await;
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();
    admin.close().await;
    result.unwrap();
}

async fn check_checkout(pool: sn_core::Pool) {
    let st = AppState {
        ceremonies: Default::default(), pool: pool.clone(),
        payments: std::sync::Arc::new(sn_payments::Registry::from_env()),
        config: sn_core::Config {
            database_url: String::new(), api_bind: "127.0.0.1:0".into(), sub_bind: "127.0.0.1:0".into(),
            sub_public_url: "https://sub.example.test".into(), brand_name: "Checkout QA".into(),
            web_root: String::new(), sub_mode: "db".into(), panel_url: "https://panel.example.test".into(), sub_service_token: None,
        },
    };
    sqlx::query("INSERT INTO settings(key,value) VALUES('billing.currency','\"RUB\"'),('cabinet.config','{\"enabled\":true,\"shop_enabled\":false}'),('bot.miniapp_shop','false')")
        .execute(&pool).await.unwrap();
    st.payments.sync_to_db(&pool).await.unwrap();
    sqlx::query("UPDATE payment_providers SET is_enabled=false").execute(&pool).await.unwrap();
    let trial: i64 = sqlx::query_scalar("INSERT INTO tariffs(code,title,is_trial) VALUES('trial','Trial',true) RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let paid: i64 = sqlx::query_scalar("INSERT INTO tariffs(code,title) VALUES('paid','Paid') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO tariff_prices(tariff_id,period_days,currency,amount_minor) VALUES($1,3,'RUB',0),($1,30,'RUB',100),($2,30,'RUB',100),($2,30,'USD',0)")
        .bind(trial).bind(paid).execute(&pool).await.unwrap();
    assert!(free_access_available(&st).await.unwrap());
    assert_eq!(app_config(State(st.clone())).await.unwrap().0["free_access_available"], true);
    let public = tariff_catalog(&st, None, false).await.unwrap();
    assert_eq!(public["tariffs"].as_array().unwrap().len(), 1);
    assert_eq!(public["tariffs"][0]["prices"].as_array().unwrap().len(), 1);
    assert!(public["methods"].as_array().unwrap().is_empty());

    fn customer(id: i64, cabinet: bool) -> Customer { Customer {id, cabinet, saved: true, installation: None} }
    fn body(tariff_id: i64, days: i32) -> PayBody { PayBody {tariff_id, days, provider: "free".into(), currency: "RUB".into(), promo: String::new()} }
    for cabinet in [true, false] {
        let id: i64 = sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES('Trial QA',$1) RETURNING id")
            .bind(uuid::Uuid::new_v4().simple().to_string()).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO subscriptions(client_id,expires_at) VALUES($1,now()-interval '1 day')")
            .bind(id).execute(&pool).await.unwrap();
        let catalog = app_tariffs(State(st.clone()), customer(id,cabinet)).await.unwrap().0;
        assert_eq!(catalog["tariffs"].as_array().unwrap().len(), 1);
        let quote = app_quote(State(st.clone()), customer(id,cabinet), Json(body(trial,3))).await.unwrap().0;
        assert_eq!(quote["amount_minor"], 0);
        if cabinet {
            let unsaved = Customer {saved:false, ..customer(id,true)};
            assert!(app_pay(State(st.clone()),unsaved,Json(body(trial,3))).await.is_err());
        }
        // A pending invoice or a forged "free" provider cannot bypass the paid switch.
        sqlx::query("INSERT INTO payments(client_id,tariff_id,status,amount_minor,currency,period_days,provider,pay_url,expires_at) VALUES($1,$2,'pending',100,'RUB',30,'free','https://example.test/pay',now()+interval '1 hour')")
            .bind(id).bind(paid).execute(&pool).await.unwrap();
        assert!(app_pay(State(st.clone()),customer(id,cabinet),Json(body(paid,30))).await.is_err());
        assert!(app_quote(State(st.clone()),customer(id,cabinet),Json(body(paid,30))).await.is_err());
        assert!(sn_core::billing::activate_free(&pool,id,paid,30).await.is_err(), "a zero in another currency is not free access");
        assert!(app_pay(State(st.clone()),customer(id,cabinet),Json(body(trial,30))).await.is_err());
        sqlx::query("UPDATE tariffs SET is_visible=false WHERE id=$1").bind(trial).execute(&pool).await.unwrap();
        assert!(app_pay(State(st.clone()),customer(id,cabinet),Json(body(trial,3))).await.is_err());
        sqlx::query("UPDATE tariffs SET is_visible=true WHERE id=$1").bind(trial).execute(&pool).await.unwrap();
        let result = app_pay(State(st.clone()),customer(id,cabinet),Json(body(trial,3))).await.unwrap().0;
        assert_eq!(result["free"], true);
        let active: bool = sqlx::query_scalar("SELECT tariff_id=$2 AND expires_at>now()+interval '2 days' FROM subscriptions WHERE client_id=$1 AND is_current")
            .bind(id).bind(trial).fetch_one(&pool).await.unwrap();
        assert!(active);
        assert!(app_pay(State(st.clone()),customer(id,cabinet),Json(body(trial,3))).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE client_id=$1 AND tariff_id=$2 AND status='success'")
            .bind(id).bind(trial).fetch_one(&pool).await.unwrap();
        assert_eq!(count,1);
        let catalog = app_tariffs(State(st.clone()),customer(id,cabinet)).await.unwrap().0;
        assert!(catalog["tariffs"].as_array().unwrap().is_empty());
    }
    sqlx::query("UPDATE settings SET value='{\"enabled\":true,\"shop_enabled\":true}' WHERE key='cabinet.config'")
        .execute(&pool).await.unwrap();
    assert!(purchases_enabled(&st,true).await.unwrap());
    assert!(!purchases_enabled(&st,false).await.unwrap(), "Mini App has its own purchase switch");
    sqlx::query("UPDATE tariffs SET is_active=false WHERE id=$1").bind(trial).execute(&pool).await.unwrap();
    assert!(!free_access_available(&st).await.unwrap());
}

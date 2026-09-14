use sqlx::{postgres::PgPoolOptions, Row};

#[tokio::test]
#[ignore = "Requires SN_TEST_DATABASE_URL pointing to an isolated audit database"]
async fn default_apps_on_install_and_upgrade_preserve_customization() {
    let pool = PgPoolOptions::new().max_connections(1)
        .connect(&std::env::var("SN_TEST_DATABASE_URL").unwrap()).await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool).await.unwrap();
    assert!(database.contains("audit"), "use an isolated audit database");
    // PostgreSQL creates a new identity sequence for this empty temporary copy.
    sqlx::query("CREATE TEMP TABLE subscription_page_apps (LIKE public.subscription_page_apps INCLUDING ALL)")
        .execute(&pool).await.unwrap();
    let migration = include_str!("../../../db/migrations/048_default_subscription_apps.sql");
    sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    let counts: Vec<(String, i64)> = sqlx::query_as("SELECT platform,count(*) FROM subscription_page_apps WHERE is_active GROUP BY platform ORDER BY platform")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(counts, [("android".into(),4),("ios".into(),4),("linux".into(),2),("macos".into(),3),("windows".into(),3)]);
    let incomplete: i64 = sqlx::query_scalar("SELECT count(*) FROM subscription_page_apps WHERE store_url IS NULL OR store_url NOT LIKE 'https://%'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(incomplete,0);
    sqlx::query("TRUNCATE subscription_page_apps").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO subscription_page_apps(platform,name,sort_order,is_active,store_url,deeplink,guide,icon_svg) VALUES('ios',' hAPP ',71,false,'https://example.test/custom','custom://{{URL}}','My guide','<svg/>'),('linux','Custom app',1,true,'https://example.test/app',NULL,NULL,NULL)")
        .execute(&pool).await.unwrap();
    for _ in 0..2 { sqlx::raw_sql(migration).execute(&pool).await.unwrap(); }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM subscription_page_apps").fetch_one(&pool).await.unwrap();
    assert_eq!(count,17);
    let row = sqlx::query("SELECT * FROM subscription_page_apps WHERE name=' hAPP '").fetch_one(&pool).await.unwrap();
    assert!(!row.get::<bool,_>("is_active"));
    assert_eq!(row.get::<i32,_>("sort_order"),71);
    assert_eq!(row.get::<String,_>("store_url"),"https://example.test/custom");
    assert_eq!(row.get::<String,_>("deeplink"),"custom://{{URL}}");
    assert_eq!(row.get::<String,_>("guide"),"My guide");
    assert_eq!(row.get::<String,_>("icon_svg"),"<svg/>");
    pool.close().await;
}

use sn_payments::Registry;
use sqlx::{postgres::PgPoolOptions, Row};

#[tokio::test]
#[ignore = "Requires SN_TEST_DATABASE_URL pointing to an isolated audit database"]
async fn manual_defaults_cover_all_currencies_and_preserve_operator_settings() {
    let pool = PgPoolOptions::new().max_connections(1)
        .connect(&std::env::var("SN_TEST_DATABASE_URL").unwrap()).await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool).await.unwrap();
    assert!(database.contains("audit"), "use an isolated audit database");
    // The single connection shadows the real registry with a disposable table.
    // No production settings, provider keys or existing rows are copied or changed.
    sqlx::query("CREATE TEMP TABLE payment_providers (LIKE public.payment_providers INCLUDING ALL)")
        .execute(&pool).await.unwrap();

    let registry = Registry::from_env();
    registry.sync_to_db(&pool).await.unwrap();
    let enabled: Vec<String> = sqlx::query_scalar("SELECT id FROM payment_providers WHERE is_enabled ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(enabled, ["manual"]);
    for currency in ["USD", "EUR", "RUB", "UAH", "KZT", "TRY", "GBP"] {
        let choices = registry.enabled_for_currency(&pool, currency).await;
        assert_eq!(choices.iter().map(|p| p.id()).collect::<Vec<_>>(), ["manual"], "{currency}");
    }
    assert!(registry.enabled_for_currency(&pool, "XTR").await.is_empty());

    sqlx::query("UPDATE payment_providers SET title='Custom transfer',is_enabled=false,enabled_currencies=ARRAY['RUB'],sort_order=17,note='Keep this' WHERE id='manual'")
        .execute(&pool).await.unwrap();
    registry.sync_to_db(&pool).await.unwrap();
    let row = sqlx::query("SELECT * FROM payment_providers WHERE id='manual'")
        .fetch_one(&pool).await.unwrap();
    assert!(!row.get::<bool, _>("is_enabled"));
    assert_eq!(row.get::<Vec<String>, _>("enabled_currencies"), ["RUB"]);
    assert_eq!(row.get::<String, _>("title"), "Custom transfer");
    assert_eq!(row.get::<i32, _>("sort_order"), 17);
    assert_eq!(row.get::<String, _>("note"), "Keep this");
    assert!(registry.enabled_for_currency(&pool, "RUB").await.is_empty());

    sqlx::query("UPDATE payment_providers SET is_enabled=true WHERE id='manual'")
        .execute(&pool).await.unwrap();
    registry.sync_to_db(&pool).await.unwrap();
    assert_eq!(registry.enabled_for_currency(&pool, "RUB").await.len(), 1);
    assert!(registry.enabled_for_currency(&pool, "USD").await.is_empty());
    pool.close().await;
}

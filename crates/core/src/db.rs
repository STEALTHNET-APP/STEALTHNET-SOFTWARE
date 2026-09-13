use sqlx::postgres::PgPoolOptions;

pub async fn connect(url: &str) -> crate::Result<crate::Pool> {
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(std::time::Duration::from_secs(8))
        .connect(url)
        .await?;
    Ok(pool)
}

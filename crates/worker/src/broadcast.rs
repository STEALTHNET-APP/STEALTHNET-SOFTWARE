//! Durable broadcast queue. One sender holds an advisory transaction lock;
//! delivery counters and retry reasons remain visible across worker restarts.
use serde_json::json;
use sn_core::{
    telegram_send::{self, Delivery},
    Pool, Result,
};
use sqlx::Row;
const BATCH: i64 = 100;
const LOCK: i64 = 0x534e42524f4144;

pub async fn tick(pool: &Pool, token: &str) -> Result<()> {
    tick_at(pool, token, telegram_send::API).await
}
async fn tick_at(pool: &Pool, token: &str, endpoint: &str) -> Result<()> {
    let mut guard = pool.begin().await?;
    if !sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock($1)")
        .bind(LOCK)
        .fetch_one(&mut *guard)
        .await?
    {
        return Ok(());
    }
    let row = sqlx::query(
        "UPDATE broadcasts SET status='sending',started_at=COALESCE(started_at,now())
        WHERE id=(SELECT id FROM broadcasts WHERE status IN ('scheduled','sending')
        AND (scheduled_at IS NULL OR scheduled_at<=now()) AND (retry_at IS NULL OR retry_at<=now())
        ORDER BY COALESCE(retry_at,scheduled_at,created_at),id LIMIT 1)
        RETURNING id,body,button_text,button_url,segment,recipients_prepared,photo_id",
    )
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else { return Ok(()) };
    let id: i64 = r.get("id");
    let body: String = r.get("body");
    let photo = telegram_send::load_photo(pool, r.get("photo_id")).await?;
    let label: Option<String> = r.get("button_text");
    let url: Option<String> = r.get("button_url");
    let keyboard = match (label, url) {
        (Some(t), Some(u)) if !t.is_empty() && !u.is_empty() => {
            Some(json!({"inline_keyboard":[[{"text":t,"url":u}]]}))
        }
        _ => None,
    };
    if !r.get::<bool, _>("recipients_prepared") {
        let segment: serde_json::Value = r.get("segment");
        let kind = segment["kind"].as_str().unwrap_or("all");
        let mut tx = pool.begin().await?;
        // API cancellation may race preparation; lock and verify before materializing.
        let running: bool =
            sqlx::query_scalar("SELECT status='sending' FROM broadcasts WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if !running {
            return Ok(());
        }
        sqlx::query("INSERT INTO broadcast_deliveries(broadcast_id,client_id)
          SELECT $1,co.id FROM client_overview co WHERE EXISTS(SELECT 1 FROM client_identities i WHERE i.client_id=co.id AND i.kind='telegram')
          AND ($2='all' OR co.status::text=$2 OR ($2='trial' AND EXISTS(SELECT 1 FROM subscriptions su JOIN tariffs t ON t.id=su.tariff_id WHERE su.client_id=co.id AND su.is_current AND t.is_trial))) ON CONFLICT DO NOTHING")
            .bind(id).bind(kind).execute(&mut *tx).await?;
        sqlx::query("UPDATE broadcasts SET recipients_prepared=true,total_count=(SELECT count(*) FROM broadcast_deliveries WHERE broadcast_id=$1) WHERE id=$1").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
    }
    // A removed identity cannot leave a permanent pending delivery behind.
    sqlx::query("UPDATE broadcast_deliveries d SET error='Telegram аккаунт больше не привязан' WHERE broadcast_id=$1 AND sent_at IS NULL AND error IS NULL AND NOT EXISTS(SELECT 1 FROM client_overview co JOIN client_identities i ON i.client_id=co.id AND i.kind='telegram' WHERE co.id=d.client_id)").bind(id).execute(pool).await?;
    let rows=sqlx::query("SELECT d.client_id,d.attempts,i.value AS chat_id,co.username,co.tariff_code,co.expires_at
      FROM broadcast_deliveries d JOIN client_overview co ON co.id=d.client_id
      JOIN LATERAL (SELECT value FROM client_identities WHERE client_id=d.client_id AND kind='telegram' ORDER BY id LIMIT 1) i ON true
      WHERE d.broadcast_id=$1 AND d.sent_at IS NULL AND d.error IS NULL ORDER BY d.client_id LIMIT $2").bind(id).bind(BATCH).fetch_all(pool).await?;
    let http = telegram_send::client()?;
    for r in rows {
        let running: bool =
            sqlx::query_scalar("SELECT status='sending' FROM broadcasts WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await?;
        if !running {
            break;
        }
        let client: i64 = r.get("client_id");
        let expires: Option<chrono::DateTime<chrono::Utc>> = r.get("expires_at");
        let text = telegram_send::personalize(
            &body,
            r.get("username"),
            r.get::<Option<&str>, _>("tariff_code").unwrap_or("—"),
            &expires
                .map(|d| d.format("%d.%m.%Y").to_string())
                .unwrap_or_else(|| "бессрочно".into()),
        );
        let result = match r.get::<String, _>("chat_id").parse::<i64>() {
            Ok(chat) => {
                telegram_send::send_with_photo(
                    &http,
                    endpoint,
                    token,
                    chat,
                    &text,
                    &keyboard,
                    photo.as_ref(),
                )
                .await
            }
            Err(_) => Delivery::Permanent("Некорректный Telegram ID".into()),
        };
        match result {
            Delivery::Sent => {
                sqlx::query("UPDATE broadcast_deliveries SET sent_at=now(),attempts=attempts+1 WHERE broadcast_id=$1 AND client_id=$2").bind(id).bind(client).execute(pool).await?;
                sqlx::query("UPDATE broadcasts SET last_error=NULL,retry_at=NULL WHERE id=$1")
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            Delivery::Permanent(e) => failed(pool, id, client, &e).await?,
            Delivery::Stop(e) => {
                tracing::warn!(broadcast=id,error=%e,"рассылка остановлена");
                sqlx::query("UPDATE broadcasts SET status='failed',last_error=$2,retry_at=NULL,finished_at=now() WHERE id=$1 AND status='sending'").bind(id).bind(e).execute(pool).await?;
                break;
            }
            Delivery::Retry { seconds, error } => {
                if r.get::<i32, _>("attempts") >= 7 {
                    failed(pool, id, client, &format!("После 8 попыток: {error}")).await?;
                } else {
                    sqlx::query("UPDATE broadcast_deliveries SET attempts=attempts+1 WHERE broadcast_id=$1 AND client_id=$2").bind(id).bind(client).execute(pool).await?;
                    sqlx::query("UPDATE broadcasts SET last_error=$2,retry_at=now()+($3 * interval '1 second') WHERE id=$1 AND status='sending'").bind(id).bind(&error).bind(seconds as i64).execute(pool).await?;
                    tracing::warn!(broadcast=id,error=%error,retry_seconds=seconds,"отложен повтор рассылки");
                    break;
                }
            }
        }
        progress(pool, id).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    progress(pool, id).await?;
    sqlx::query("UPDATE broadcasts SET status='sent',finished_at=now(),retry_at=NULL WHERE id=$1 AND status='sending' AND NOT EXISTS(SELECT 1 FROM broadcast_deliveries WHERE broadcast_id=$1 AND sent_at IS NULL AND error IS NULL)").bind(id).execute(pool).await?;
    guard.commit().await?;
    Ok(())
}
async fn failed(pool: &Pool, id: i64, client: i64, error: &str) -> Result<()> {
    sqlx::query("UPDATE broadcast_deliveries SET error=$3,attempts=attempts+1 WHERE broadcast_id=$1 AND client_id=$2").bind(id).bind(client).bind(error).execute(pool).await?;
    Ok(())
}
async fn progress(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query("UPDATE broadcasts SET sent_count=(SELECT count(*) FROM broadcast_deliveries WHERE broadcast_id=$1 AND sent_at IS NOT NULL),failed_count=(SELECT count(*) FROM broadcast_deliveries WHERE broadcast_id=$1 AND error IS NOT NULL) WHERE id=$1").bind(id).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod integration {
    use super::*;
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use std::future::IntoFuture;
    use std::sync::{
        atomic::{AtomicU16, AtomicUsize, Ordering},
        Arc,
    };
    #[derive(Clone)]
    struct Mock {
        code: Arc<AtomicU16>,
        calls: Arc<AtomicUsize>,
    }
    async fn respond(
        State(s): State<Mock>,
        Json(body): Json<serde_json::Value>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        assert_eq!(body["parse_mode"], "HTML");
        assert!(body["chat_id"].as_i64().is_some());
        s.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let code = s.code.load(Ordering::SeqCst);
        (
            StatusCode::from_u16(code).unwrap(),
            Json(match code {
                200 => json!({"ok":true,"result":{"message_id":1}}),
                429 => {
                    json!({"ok":false,"error_code":429,"description":"rate limited","parameters":{"retry_after":120}})
                }
                _ => {
                    json!({"ok":false,"error_code":code,"description":if code==400{"Bad Request: BUTTON_URL_INVALID"}else{"blocked"}})
                }
            }),
        )
    }
    async fn respond_photo(
        State(s): State<Mock>,
        headers: axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> (StatusCode, Json<serde_json::Value>) {
        let kind = headers.get("content-type").unwrap().to_str().unwrap();
        assert!(kind.starts_with("multipart/form-data; boundary="));
        let body = String::from_utf8_lossy(&body);
        for part in [
            "name=\"chat_id\"",
            "123456789",
            "name=\"caption\"",
            "Hello QA Alice",
            "name=\"photo\"",
            "image/png",
            "name=\"reply_markup\"",
            "https://example.test",
            "name=\"parse_mode\"",
            "HTML",
        ] {
            assert!(body.contains(part), "missing multipart field {part}");
        }
        s.calls.fetch_add(1, Ordering::SeqCst);
        (
            StatusCode::OK,
            Json(json!({"ok":true,"result":{"message_id":2}})),
        )
    }
    async fn campaign(pool: &Pool, segment: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO broadcasts(title,body,status,segment) VALUES('QA','Hello {name}','scheduled',jsonb_build_object('kind',$1::text)) RETURNING id").bind(segment).fetch_one(pool).await.unwrap()
    }
    async fn state(pool: &Pool, id: i64) -> (String, i32, i32) {
        sqlx::query_as("SELECT status::text,sent_count,failed_count FROM broadcasts WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    #[tokio::test]
    #[ignore = "requires SN_TEST_DATABASE_URL; uses a fresh temporary schema and local Telegram mock"]
    async fn queue_delivery_errors_retries_cancellation_and_concurrency() {
        let url = std::env::var("SN_TEST_DATABASE_URL")
            .expect("SN_TEST_DATABASE_URL must point to an isolated test database");
        let base = sn_core::db::connect(&url).await.unwrap();
        let schema = format!(
            "sn_broadcast_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&base)
            .await
            .unwrap();
        let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options.options([("search_path", format!("{schema},public"))]))
            .await
            .unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../db/migrations");
        let mut files = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "sql"))
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            sqlx::raw_sql(&std::fs::read_to_string(&file).unwrap())
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        }
        sqlx::query("INSERT INTO clients(username,short_id) VALUES('QA Alice','queue_alice')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO client_identities(client_id,kind,value) SELECT id,'telegram','123456789' FROM clients WHERE short_id='queue_alice'").execute(&pool).await.unwrap();
        let mock = Mock {
            code: Arc::new(AtomicU16::new(200)),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/botQA/sendMessage", post(respond))
                    .route("/botQA/sendPhoto", post(respond_photo))
                    .with_state(mock.clone()),
            )
            .into_future(),
        );
        let id = campaign(&pool, "all").await;
        let (a, b) = tokio::join!(
            tick_at(&pool, "QA", &endpoint),
            tick_at(&pool, "QA", &endpoint)
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(state(&pool, id).await, ("sent".into(), 1, 0));
        assert_eq!(mock.calls.load(Ordering::SeqCst), 1);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(mock.calls.load(Ordering::SeqCst), 1);
        let empty = campaign(&pool, "trial").await;
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(state(&pool, empty).await, ("sent".into(), 0, 0));
        mock.code.store(400, Ordering::SeqCst);
        let bad = campaign(&pool, "all").await;
        let next = campaign(&pool, "all").await;
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(state(&pool, bad).await.0, "failed");
        let error: String = sqlx::query_scalar("SELECT last_error FROM broadcasts WHERE id=$1")
            .bind(bad)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(error.contains("BUTTON_URL_INVALID"));
        mock.code.store(200, Ordering::SeqCst);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(state(&pool, next).await.0, "sent");
        mock.code.store(429, Ordering::SeqCst);
        let retry = campaign(&pool, "all").await;
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        let calls = mock.calls.load(Ordering::SeqCst);
        let delay: bool = sqlx::query_scalar(
            "SELECT retry_at>now()+interval '100 seconds' FROM broadcasts WHERE id=$1",
        )
        .bind(retry)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(delay);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(mock.calls.load(Ordering::SeqCst), calls);
        let other = campaign(&pool, "all").await;
        mock.code.store(200, Ordering::SeqCst);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(state(&pool, other).await.0, "sent");
        sqlx::query("UPDATE broadcasts SET retry_at=now() WHERE id=$1")
            .bind(retry)
            .execute(&pool)
            .await
            .unwrap();
        mock.code.store(403, Ordering::SeqCst);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(state(&pool, retry).await, ("sent".into(), 0, 1));
        let canceled = campaign(&pool, "all").await;
        sqlx::query("UPDATE broadcasts SET status='canceled' WHERE id=$1")
            .bind(canceled)
            .execute(&pool)
            .await
            .unwrap();
        let calls = mock.calls.load(Ordering::SeqCst);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(mock.calls.load(Ordering::SeqCst), calls);
        let photo_id: String=sqlx::query_scalar("INSERT INTO broadcast_media(id,content_type,data,width,height) VALUES(gen_random_uuid(),'image/png',decode('89504e470d0a1a0a','hex'),1,1) RETURNING id::text").fetch_one(&pool).await.unwrap();
        let photo_job = campaign(&pool, "all").await;
        sqlx::query("UPDATE broadcasts SET photo_id=$2::text::uuid,button_text='Open',button_url='https://example.test' WHERE id=$1").bind(photo_job).bind(photo_id).execute(&pool).await.unwrap();
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(state(&pool, photo_job).await, ("sent".into(), 1, 0));
        assert_eq!(mock.calls.load(Ordering::SeqCst), calls + 1);
        tick_at(&pool, "QA", &endpoint).await.unwrap();
        assert_eq!(mock.calls.load(Ordering::SeqCst), calls + 1);
        server.abort();
        pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&base)
            .await
            .unwrap();
        base.close().await;
    }
}

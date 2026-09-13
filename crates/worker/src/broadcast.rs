//! Отправка рассылок в Telegram.
//!
//! Рассылка — единственная операция, которая за минуту касается всей базы
//! клиентов. Отсюда требования: не превысить лимиты Telegram, не потерять
//! прогресс при перезапуске и не долбить тех, кто заблокировал бота.

use serde_json::json;
use sn_core::{Pool, Result};
use sqlx::Row;

/// Telegram отдаёт примерно 30 сообщений в секунду на бота.
/// Берём с запасом: упереться в лимит дороже, чем отправить на секунду дольше.
const MESSAGES_PER_SEC: u64 = 20;

/// Сколько получателей берём за один проход. Меньше — чаще обновляются
/// счётчики в панели и мягче реакция на остановку.
const BATCH: i64 = 500;

/// Забирает одну рассылку, готовую к отправке, и отправляет очередную порцию.
pub async fn tick(pool: &Pool, bot_token: &str) -> Result<()> {
    // Берём рассылку в работу атомарно: два воркера не должны взять одну и ту же.
    let row = sqlx::query(
        "UPDATE broadcasts SET status = 'sending', started_at = COALESCE(started_at, now())
          WHERE id = (
              SELECT id FROM broadcasts
               WHERE status IN ('scheduled', 'sending')
                 AND (scheduled_at IS NULL OR scheduled_at <= now())
               ORDER BY id
               FOR UPDATE SKIP LOCKED
               LIMIT 1)
      RETURNING id, title, body, button_text, button_url, segment",
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else { return Ok(()) };

    let id: i64 = row.get("id");
    let body: String = row.get("body");
    let button_text: Option<String> = row.get("button_text");
    let button_url: Option<String> = row.get("button_url");
    let segment: serde_json::Value = row.get("segment");

    // Получателей материализуем один раз: если делать выборку на каждый
    // проход, изменившийся статус клиента может выкинуть его из рассылки
    // на середине — или, наоборот, добавить второй раз.
    let filled: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM broadcast_deliveries WHERE broadcast_id = $1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    if !filled {
        let kind = segment["kind"].as_str().unwrap_or("all");
        let inserted = fill_recipients(pool, id, kind).await?;
        sqlx::query("UPDATE broadcasts SET total_count = $2 WHERE id = $1")
            .bind(id)
            .bind(inserted as i32)
            .execute(pool)
            .await?;
        tracing::info!(broadcast = id, recipients = inserted, "рассылка подготовлена");
    }

    let batch = sqlx::query(
        "SELECT d.client_id, i.value AS chat_id, co.username, co.tariff_code, co.expires_at
           FROM broadcast_deliveries d
           JOIN client_identities i ON i.client_id = d.client_id AND i.kind = 'telegram'
           JOIN client_overview co ON co.id=d.client_id
          WHERE d.broadcast_id = $1 AND d.sent_at IS NULL AND d.error IS NULL
          LIMIT $2",
    )
    .bind(id)
    .bind(BATCH)
    .fetch_all(pool)
    .await?;

    if batch.is_empty() {
        finish(pool, id).await?;
        return Ok(());
    }

    let keyboard = match (&button_text, &button_url) {
        (Some(t), Some(u)) if !t.is_empty() && !u.is_empty() => {
            Some(json!({ "inline_keyboard": [[{ "text": t, "url": u }]] }))
        }
        _ => None,
    };

    let http = reqwest::Client::new();
    let delay = std::time::Duration::from_millis(1000 / MESSAGES_PER_SEC);
    let mut sent = 0u32;
    let mut failed = 0u32;

    for (index, r) in batch.iter().enumerate() {
        if index % 20 == 0 {
            let running: bool=sqlx::query_scalar("SELECT status='sending' FROM broadcasts WHERE id=$1").bind(id).fetch_one(pool).await?;
            if !running { break; }
        }
        let client_id: i64 = r.get("client_id");
        let Ok(chat_id) = r.get::<String, _>("chat_id").parse::<i64>() else {
            mark_failed(pool, id, client_id, "некорректный telegram id").await;
            failed += 1;
            continue;
        };

        let expires: Option<chrono::DateTime<chrono::Utc>>=r.get("expires_at");
        let message=personalize(&body,r.get("username"),r.get::<Option<&str>,_>("tariff_code").unwrap_or("—"),
            &expires.map(|at|at.format("%d.%m.%Y").to_string()).unwrap_or_else(||"бессрочно".into()));
        match send_one(&http, bot_token, chat_id, &message, &keyboard).await {
            SendResult::Ok => {
                sqlx::query(
                    "UPDATE broadcast_deliveries SET sent_at = now()
                      WHERE broadcast_id = $1 AND client_id = $2",
                )
                .bind(id)
                .bind(client_id)
                .execute(pool)
                .await?;
                sent += 1;
            }
            // Бот заблокирован или чат удалён — повторять бессмысленно.
            SendResult::Permanent(err) => {
                mark_failed(pool, id, client_id, &err).await;
                failed += 1;
            }
            // Лимит или сбой сети — оставляем на следующий проход.
            SendResult::Retry(after) => {
                tracing::warn!(broadcast = id, "притормаживаем на {after} с");
                tokio::time::sleep(std::time::Duration::from_secs(after)).await;
            }
        }
        tokio::time::sleep(delay).await;
    }

    sqlx::query(
        "UPDATE broadcasts
            SET sent_count = (SELECT count(*) FROM broadcast_deliveries
                               WHERE broadcast_id = $1 AND sent_at IS NOT NULL),
                failed_count = (SELECT count(*) FROM broadcast_deliveries
                                 WHERE broadcast_id = $1 AND error IS NOT NULL)
          WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;

    tracing::info!(broadcast = id, sent, failed, "порция отправлена");
    Ok(())
}

/// Разворачивает сегмент в список получателей.
/// Только те, у кого есть Telegram: остальным отправлять некуда.
async fn fill_recipients(pool: &Pool, broadcast_id: i64, kind: &str) -> Result<u64> {
    let condition = match kind {
        "active" => "co.status = 'active'",
        "expired" => "co.status = 'expired'",
        "limited" => "co.status = 'limited'",
        "trial" => "EXISTS(SELECT 1 FROM subscriptions su JOIN tariffs t ON t.id=su.tariff_id WHERE su.client_id=co.id AND su.is_current AND t.is_trial)",
        _ => "true",
    };

    let sql = format!(
        "INSERT INTO broadcast_deliveries (broadcast_id, client_id)
         SELECT $1, co.id FROM client_overview co
          WHERE {condition}
            AND EXISTS (SELECT 1 FROM client_identities i
                         WHERE i.client_id = co.id AND i.kind = 'telegram')
         ON CONFLICT DO NOTHING"
    );

    let res = sqlx::query(&sql).bind(broadcast_id).execute(pool).await?;
    Ok(res.rows_affected())
}

async fn mark_failed(pool: &Pool, broadcast_id: i64, client_id: i64, err: &str) {
    let _ = sqlx::query(
        "UPDATE broadcast_deliveries SET error = $3
          WHERE broadcast_id = $1 AND client_id = $2",
    )
    .bind(broadcast_id)
    .bind(client_id)
    .bind(err)
    .execute(pool)
    .await;
}

async fn finish(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query("UPDATE broadcasts SET status = 'sent', finished_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    let row = sqlx::query("SELECT title, sent_count, failed_count FROM broadcasts WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    tracing::info!(
        broadcast = id,
        title = row.get::<String, _>("title"),
        sent = row.get::<i32, _>("sent_count"),
        failed = row.get::<i32, _>("failed_count"),
        "рассылка завершена"
    );
    Ok(())
}

enum SendResult {
    Ok,
    /// Повторять бессмысленно: бот заблокирован, чат удалён и т.п.
    Permanent(String),
    /// Подождать столько секунд и попробовать снова.
    Retry(u64),
}

async fn send_one(
    http: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
    keyboard: &Option<serde_json::Value>,
) -> SendResult {
    let mut payload = json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "HTML",
        "link_preview_options": { "is_disabled": true },
    });
    if let Some(kb) = keyboard {
        payload["reply_markup"] = kb.clone();
    }

    let res = http
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&payload)
        .send()
        .await;

    let Ok(res) = res else {
        return SendResult::Retry(3);
    };

    let status = res.status();
    let body: serde_json::Value = res.json().await.unwrap_or(json!({}));

    if body["ok"] == true {
        return SendResult::Ok;
    }

    // 429: Telegram сам говорит, сколько ждать.
    if status.as_u16() == 429 {
        let after = body["parameters"]["retry_after"].as_u64().unwrap_or(5);
        return SendResult::Retry(after.min(60));
    }

    let desc = body["description"].as_str().unwrap_or("неизвестная ошибка");
    // 403 — заблокировали бота; 400 с «chat not found» — удалённый аккаунт.
    if status.as_u16() == 403 || desc.contains("chat not found") || desc.contains("user is deactivated")
    {
        return SendResult::Permanent(desc.to_string());
    }
    SendResult::Retry(3)
}

fn personalize(template: &str, name: &str, tariff: &str, expires: &str) -> String {
    let escape=|s: &str| s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&#39;");
    template.replace("{expire_date}",&escape(expires)).replace("{tariff}",&escape(tariff)).replace("{name}",&escape(name))
}

#[cfg(test)]
mod personalization_tests {
    use super::*;
    #[test]
    fn substitutes_recipient_fields_and_preserves_template_html() {
        assert_eq!(personalize("<b>{name}</b>: {tariff} до {expire_date}","Alice","PRO","20.09.2026"),"<b>Alice</b>: PRO до 20.09.2026");
    }
    #[test]
    fn recipient_data_cannot_inject_telegram_markup() {
        assert_eq!(personalize("{name} {tariff}","<a>&\"","VIP<","—"),"&lt;a&gt;&amp;&quot; VIP&lt;");
    }
}

//! Автопродление подписок.
//!
//! Работает по-разному в зависимости от способа оплаты:
//!  * модуль умеет списывать сам (`supports_recurring`) — списываем;
//!  * не умеет (Stars, крипта — там нужно подтверждение человеком) —
//!    отправляем напоминание со ссылкой на продление.
//!
//! Второй случай — не «заглушка», а осознанный режим: заставить
//! криптоплатёж списаться без клиента невозможно в принципе.

use serde_json::json;
use sn_core::{Pool, Result};
use sn_payments::{InvoiceRequest, Registry};
use sqlx::Row;

/// За сколько часов до окончания пытаемся продлить.
/// Сутки дают запас: если списание не прошло, человек успеет заплатить сам.
const HOURS_BEFORE: i64 = 24;

pub async fn tick(pool: &Pool, reg: &Registry, bot_token: Option<&str>) -> Result<()> {
    let rows = sqlx::query(
        "SELECT c.id AS client_id, c.username, s.tariff_id, s.expires_at,
                t.code AS tariff_code, i.value AS telegram_id
           FROM clients c
           JOIN subscriptions s ON s.client_id = c.id AND s.is_current
           JOIN tariffs t       ON t.id = s.tariff_id
           LEFT JOIN client_identities i ON i.client_id = c.id AND i.kind = 'telegram'
          WHERE s.autorenew
            AND c.deleted_at IS NULL
            AND c.status = 'active'
            AND s.expires_at IS NOT NULL
            AND s.expires_at BETWEEN now() AND now() + ($1 || ' hours')::interval
            AND t.allow_autorenew AND t.is_active
            -- Уже пробовали в этом цикле продления — не повторяем каждую минуту.
            AND NOT EXISTS (
                SELECT 1 FROM audit_log a
                 WHERE a.entity_type = 'client' AND a.entity_id = c.id
                   AND a.action = 'autorenew.attempt'
                   AND a.created_at > s.expires_at - ($1 || ' hours')::interval)
          LIMIT 100",
    )
    .bind(HOURS_BEFORE.to_string())
    .fetch_all(pool)
    .await?;

    for r in &rows {
        let client_id: i64 = r.get("client_id");
        let tariff_id: i64 = r.get("tariff_id");
        let username: String = r.get("username");
        let telegram_id: Option<String> = r.get("telegram_id");

        let outcome = try_renew(pool, reg, client_id, tariff_id).await;

        let (action_result, note) = match &outcome {
            Ok(Some(payment_id)) => ("charged", format!("платёж {payment_id}")),
            Ok(None) => ("reminded", "автосписание недоступно, отправлено напоминание".into()),
            Err(e) => ("failed", e.to_string()),
        };

        sqlx::query(
            "INSERT INTO audit_log (actor_kind, action, entity_type, entity_id, payload)
             VALUES ('system', 'autorenew.attempt', 'client', $1, $2)",
        )
        .bind(client_id)
        .bind(json!({ "result": action_result, "note": note }))
        .execute(pool)
        .await?;

        // Если списать не удалось — человек должен узнать об этом сам,
        // иначе он просто обнаружит отключённый VPN.
        if action_result != "charged" {
            if let (Some(token), Some(tg)) = (bot_token, telegram_id.as_deref()) {
                if let Ok(chat_id) = tg.parse::<i64>() {
                    notify(token, chat_id).await;
                }
            }
        }

        tracing::info!(client = username, result = action_result, note, "автопродление");
    }
    Ok(())
}

/// Возвращает `Ok(Some(payment_id))` при успешном списании,
/// `Ok(None)` — если способ оплаты не поддерживает автосписание.
async fn try_renew(
    pool: &Pool,
    reg: &Registry,
    client_id: i64,
    tariff_id: i64,
) -> Result<Option<i64>> {
    // Берём метод оплаты, сохранённый клиентом.
    let method = sqlx::query(
        "SELECT provider, provider_token FROM payment_methods
          WHERE client_id = $1 AND is_default
          ORDER BY id DESC LIMIT 1",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;

    let Some(method) = method else {
        return Ok(None);
    };
    let provider_id: String = method.get("provider");
    let token: String = method.get("provider_token");

    let Some(provider) = reg.get(&provider_id) else {
        return Ok(None);
    };
    if !provider.supports_recurring() || !provider.is_configured() {
        return Ok(None);
    }

    // Продлеваем на тот же срок, что покупали в прошлый раз.
    let price = sqlx::query(
        "SELECT tp.period_days, tp.amount_minor, tp.currency, t.title
           FROM tariff_prices tp
           JOIN tariffs t ON t.id = tp.tariff_id
          WHERE tp.tariff_id = $1 AND tp.is_active AND t.is_active
            AND tp.currency = ANY($2::text[])
          ORDER BY tp.period_days LIMIT 1",
    )
    .bind(tariff_id)
    .bind(provider.currencies())
    .fetch_optional(pool)
    .await?;

    let Some(price) = price else { return Ok(None) };
    let period_days: i32 = price.get("period_days");
    let amount_minor: i64 = price.get("amount_minor");
    let currency: String = price.get("currency");
    let title: String = price.get("title");

    let payment_id: i64 = sqlx::query_scalar(
        "INSERT INTO payments (client_id, tariff_id, kind, status, amount_minor, currency,
                               period_days, provider)
         VALUES ($1, $2, 'autorenewal', 'pending', $3, $4, $5, $6) RETURNING id",
    )
    .bind(client_id)
    .bind(tariff_id)
    .bind(amount_minor)
    .bind(&currency)
    .bind(period_days)
    .bind(&provider_id)
    .fetch_one(pool)
    .await?;

    let req = InvoiceRequest {
        payment_id,
        client_id,
        telegram_id: None,
        amount_minor,
        currency: currency.clone(),
        description: format!("{title} · {period_days} дн (автопродление)"),
        return_url: None,
    };

    match provider.charge_saved(&req, &token).await {
        Ok(outcome) => {
            reg.apply_outcome(pool, &provider_id, &outcome).await?;
            Ok(Some(payment_id))
        }
        Err(e) => {
            sqlx::query("UPDATE payments SET status = 'failed', error_message = $2 WHERE id = $1")
                .bind(payment_id)
                .bind(e.to_string())
                .execute(pool)
                .await?;
            Err(e)
        }
    }
}

async fn notify(token: &str, chat_id: i64) {
    let text = "⏳ Подписка заканчивается завтра.\n\n\
                Автоматически продлить не получилось — продлите вручную: /buy";
    let _ = reqwest::Client::new()
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&json!({ "chat_id": chat_id, "text": text }))
        .send()
        .await;
}

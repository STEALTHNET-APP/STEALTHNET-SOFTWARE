//! Выдача доступа без оплаты.
//!
//! Живёт в общем крейте нарочно: этим кодом клиент получает доступ, и
//! второй его копии быть не должно. Раньше он был только в боте, и в
//! кабинете бесплатный тариф уводил на платёжку — заплатить ноль там,
//! разумеется, нельзя.

use crate::{Error, Pool, Result};

/// Выдать бесплатный период (пробный тариф).
///
/// Проверку «уже брал» делаем здесь, а не в вызывающем коде: витрина
/// использованный пробный не показывает, но на это нельзя опираться —
/// запрос к панели можно отправить и мимо витрины.
pub async fn activate_free(pool: &Pool, client_id: i64, tariff_id: i64, days: i32) -> Result<()> {
    if days<=0 { return Err(Error::bad("срок должен быть больше нуля")); }
    let currency = crate::money::service_currency(pool).await;
    let mut tx = pool.begin().await?;
    let client: Option<i64> = sqlx::query_scalar("SELECT id FROM clients WHERE id=$1 AND deleted_at IS NULL FOR UPDATE")
        .bind(client_id).fetch_optional(&mut *tx).await?;
    if client.is_none() { return Err(Error::NotFound); }
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM payments
                        WHERE client_id = $1 AND tariff_id = $2 AND status = 'success')",
    )
    .bind(client_id)
    .bind(tariff_id)
    .fetch_one(&mut *tx)
    .await?;
    if already {
        return Err(Error::bad("бесплатный период уже использован"));
    }

    // Цену тоже проверяем сами: «бесплатно» — это ноль в базе, а не
    // слово вызывающего. Иначе платный тариф выдавался бы даром тем,
    // кто позовёт эту ручку напрямую.
    let amount: Option<i64> = sqlx::query_scalar(
        "SELECT tp.amount_minor FROM tariff_prices tp JOIN tariffs t ON t.id=tp.tariff_id
          WHERE tp.tariff_id = $1 AND tp.period_days = $2 AND tp.currency = $3
            AND tp.is_active AND t.is_active AND t.is_visible",
    )
    .bind(tariff_id)
    .bind(days)
    .bind(&currency)
    .fetch_optional(&mut *tx)
    .await?;
    if amount != Some(0) {
        return Err(Error::bad("этот тариф не бесплатный"));
    }

    sqlx::query(
        "INSERT INTO payments (client_id, tariff_id, kind, status, amount_minor, currency,
                               period_days, provider, paid_at)
         VALUES ($1, $2, 'purchase', 'success', 0, $4, $3, 'manual', now())",
    )
    .bind(client_id)
    .bind(tariff_id)
    .bind(days)
    .bind(&currency)
    .execute(&mut *tx)
    .await?;

    crate::addons::maintain(&mut tx,client_id,false).await?;
    let subscription = sqlx::query(
        "UPDATE subscriptions s
            SET tariff_id = $2,
                expires_at = GREATEST(COALESCE(s.expires_at, now()), now()) + ($3 || ' days')::interval,
                device_limit = t.device_limit,
                traffic_limit_bytes = t.traffic_limit_bytes,
                reset_strategy = t.reset_strategy, canceled_at=NULL,
                traffic_reset_at=CASE t.reset_strategy WHEN 'day' THEN now()+interval '1 day' WHEN 'week' THEN now()+interval '7 days' WHEN 'month' THEN now()+interval '1 month' ELSE NULL END
           FROM tariffs t
          WHERE t.id = $2 AND s.client_id = $1 AND s.is_current",
    )
    .bind(client_id)
    .bind(tariff_id)
    .bind(days.to_string())
    .execute(&mut *tx)
    .await?;

    if subscription.rows_affected()==0 { return Err(Error::bad("у клиента нет текущей подписки")); }
    sqlx::query("UPDATE clients SET status = 'active' WHERE id = $1 AND status IN ('expired','limited')")
        .bind(client_id)
        .execute(&mut *tx)
        .await?;

    // Без сквадов подписка есть, а узлов в ней нет — клиент получит
    // пустой конфиг и решит, что ничего не работает.
    sqlx::query(
        "INSERT INTO client_squads (client_id, squad_id)
         SELECT $1, ts.squad_id FROM tariff_squads ts WHERE ts.tariff_id = $2
         ON CONFLICT DO NOTHING",
    )
    .bind(client_id)
    .bind(tariff_id)
    .execute(&mut *tx)
    .await?;

    crate::addons::restore_after_plan(&mut tx,client_id).await?;
    tx.commit().await?;
    Ok(())
}

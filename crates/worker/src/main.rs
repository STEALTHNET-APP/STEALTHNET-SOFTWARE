//! Фоновые задачи: статусы, сброс трафика, просроченные счета, напоминания.
//!
//! Воркер ничего не решает сам — он лишь приводит хранимые данные в
//! соответствие с тем, что уже вычисляет `effective_status()` в SQL.
//! Поэтому его падение не открывает доступ бесплатно: выдача подписок
//! и конфиги для нод и без него считают статус на лету.

mod autorenew;
mod broadcast;

use chrono::Utc;
use serde_json::json;
use sn_core::{Config, Pool, Result};
use sqlx::Row;

/// Как часто крутим цикл. Минуты достаточно: все задачи идемпотентны,
/// а частый опрос базы ради подписок, живущих месяцами, бессмыслен.
const TICK_SECS: u64 = 60;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        // sn_core здесь обязателен: именно там логируются ошибки БД.
        // Без него «внутренняя ошибка» в ответе не имеет следа в журнале.
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sn_worker=info,sn_core=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = sn_core::db::connect(&config.database_url).await?;
    let payments = sn_payments::Registry::from_env();
    let bot_token = std::env::var("BOT_TOKEN").ok().filter(|t| !t.is_empty());

    tracing::info!("воркер запущен, цикл каждые {TICK_SECS} с");

    if let Some(token)=bot_token.clone() {
        let alerts_pool=pool.clone();let alerts_config=config.clone();
        tokio::spawn(async move {
            let mut timer=tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                timer.tick().await;
                if let Err(e)=sn_core::alerts::scan_nodes(&alerts_pool).await {tracing::warn!(error=%e,"диагностика нод для группы");}
                if let Err(e)=sn_core::alerts::tick(&alerts_pool,&token,&alerts_config.brand_name,&alerts_config.panel_url).await {tracing::warn!(error=%e,"доставка уведомлений команды");}
            }
        });
    }

    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(TICK_SECS));
    loop {
        ticker.tick().await;

        // Каждая задача изолирована: ошибка одной не должна останавливать остальные.
        if let Err(e) = sync_statuses(&pool).await {
            tracing::error!(error = %e, "синхронизация статусов");
            let _=sn_core::alerts::incident(&pool,"job:sync_statuses","Синхронизация статусов",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
        } else { let _=sn_core::alerts::incident(&pool,"job:sync_statuses","Синхронизация статусов",None).await; }
        if let Err(e) = reset_traffic(&pool).await {
            tracing::error!(error = %e, "сброс трафика");
            let _=sn_core::alerts::incident(&pool,"job:reset_traffic","Сброс трафика",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
        } else { let _=sn_core::alerts::incident(&pool,"job:reset_traffic","Сброс трафика",None).await; }
        if let Err(e) = payments.expire_stale(&pool).await {
            tracing::error!(error = %e, "просроченные счета");
            let _=sn_core::alerts::incident(&pool,"job:payments_expire_stale","Обработка просроченных счетов",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
        } else { let _=sn_core::alerts::incident(&pool,"job:payments_expire_stale","Обработка просроченных счетов",None).await; }
        // Чистим отчёты тем же тактом, что и партиции: обе задачи
        // редкие и обе про то, чтобы база не пухла.
        if let Err(e) = prune_reports(&pool).await {
            tracing::warn!(error = %e, "не почистил отчёты");
            let _=sn_core::alerts::incident(&pool,"job:prune_reports","Очистка отчётов",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
        } else { let _=sn_core::alerts::incident(&pool,"job:prune_reports","Очистка отчётов",None).await; }
        if let Err(e) = ensure_partitions(&pool).await {
            tracing::error!(error = %e, "обслуживание партиций");
            let _=sn_core::alerts::incident(&pool,"job:ensure_partitions","Обслуживание базы",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
        } else { let _=sn_core::alerts::incident(&pool,"job:ensure_partitions","Обслуживание базы",None).await; }
        if let Some(token) = &bot_token {
            if let Err(e) = notify_expiring(&pool, token, &config).await {
                tracing::error!(error = %e, "напоминания об окончании");
                let _=sn_core::alerts::incident(&pool,"job:notify_expiring","Напоминания клиентам",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
            } else { let _=sn_core::alerts::incident(&pool,"job:notify_expiring","Напоминания клиентам",None).await; }
            if let Err(e) = broadcast::tick(&pool, token).await {
                tracing::error!(error = %e, "рассылка");
                let _=sn_core::alerts::incident(&pool,"job:broadcast_tick","Рассылки",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
            } else { let _=sn_core::alerts::incident(&pool,"job:broadcast_tick","Рассылки",None).await; }
        }
        if let Err(e) = autorenew::tick(&pool, &payments, bot_token.as_deref()).await {
            tracing::error!(error = %e, "автопродление");
            let _=sn_core::alerts::incident(&pool,"job:autorenew_tick","Автопродление",Some("Фоновая задача завершилась с ошибкой. Подробности в журнале сервиса.")).await;
        } else { let _=sn_core::alerts::incident(&pool,"job:autorenew_tick","Автопродление",None).await; }
    }
}

/// Приводит `clients.status` к тому, что уже считает `effective_status()`.
async fn sync_statuses(pool: &Pool) -> Result<()> {
    let rows = sqlx::query(
        "SELECT id, stored_status::text AS stored, actual_status::text AS actual
           FROM clients_needing_status_sync LIMIT 5000",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    for r in &rows {
        let id: i64 = r.get("id");
        let actual: String = r.get("actual");
        sqlx::query("UPDATE clients SET status = $2::client_status WHERE id = $1")
            .bind(id)
            .bind(&actual)
            .execute(pool)
            .await?;

        sqlx::query(
            "INSERT INTO audit_log (actor_kind, action, entity_type, entity_id, payload)
             VALUES ('system', 'client.status_sync', 'client', $1, $2)",
        )
        .bind(id)
        .bind(json!({ "from": r.get::<String, _>("stored"), "to": actual }))
        .execute(pool)
        .await?;
    }

    tracing::info!(count = rows.len(), "статусы синхронизированы");
    Ok(())
}

/// Сброс счётчика трафика по стратегии тарифа.
///
/// `traffic_reset_at` хранит момент следующего сброса. Считаем от него,
/// а не от «сейчас»: иначе сброс уползал бы вперёд на время простоя воркера.
async fn reset_traffic(pool: &Pool) -> Result<()> {
    let ids:Vec<i64>=sqlx::query_scalar("SELECT client_id FROM subscriptions WHERE is_current AND reset_strategy<>'no_reset' AND (traffic_reset_at IS NULL OR traffic_reset_at<=now()) UNION SELECT client_id FROM subscription_addons WHERE ended_at IS NULL AND (expires_at<=now() OR activated_at IS NULL)").fetch_all(pool).await?;
    for id in ids { sn_core::addons::refresh(pool,id).await?; }

    Ok(())
}

/// Партиции создаём заранее: таблица без нужной партиции отклоняет вставку,
/// и сбор трафика ломается ровно первого числа, молча.
/// Чистка отчётов из журнала доступа.
///
/// Журнал посещений не должен копиться вечно: чем дольше он лежит, тем
/// хуже последствия утечки. Срок задаётся в настройках.
async fn prune_reports(pool: &Pool) -> Result<()> {
    sqlx::query("DELETE FROM team_notifications WHERE status IN ('sent','failed','canceled') AND created_at<now()-interval '30 days'").execute(pool).await?;
    let days: i64 = sqlx::query_scalar::<_, Option<serde_json::Value>>(
        "SELECT value FROM settings WHERE key = 'reports.retention_days'",
    )
    .fetch_optional(pool)
    .await?
    .flatten()
    .and_then(|v| v.as_i64())
    .unwrap_or(30)
    .clamp(1, 365);

    let a = sqlx::query("DELETE FROM torrent_reports WHERE day < (now() AT TIME ZONE 'UTC')::date - $1::int")
        .bind(days as i32)
        .execute(pool)
        .await?;
    let b = sqlx::query("DELETE FROM http_domain_stats WHERE day < (now() AT TIME ZONE 'UTC')::date - $1::int")
        .bind(days as i32)
        .execute(pool)
        .await?;

    if a.rows_affected() + b.rows_affected() > 0 {
        tracing::info!(
            torrents = a.rows_affected(),
            domains = b.rows_affected(),
            days,
            "старые отчёты удалены"
        );
    }
    Ok(())
}

async fn ensure_partitions(pool: &Pool) -> Result<()> {
    // Раз в сутки достаточно; вызов идемпотентен, лишний раз не навредит.
    let hour = Utc::now().format("%H").to_string();
    if hour != "03" {
        return Ok(());
    }
    sqlx::query("SELECT ensure_partitions(1, 6)")
        .execute(pool)
        .await?;
    tracing::info!("партиции проверены");
    Ok(())
}

/// Русское склонение после числа: 1 день, 2 дня, 5 дней.
///
/// Пороги напоминаний — обычный список, и однажды в него добавят 5 или 7.
/// Жёстко вписанное «дня» тогда молча превратится в «через 5 дня»: ошибка
/// не сломает отправку, а просто будет каждый день уходить клиентам.
fn склонение(n: i64, один: &'static str, два: &'static str, много: &'static str) -> &'static str {
    let (сотни, десятки) = (n.abs() % 100, n.abs() % 10);
    if (11..=14).contains(&сотни) {
        много
    } else if десятки == 1 {
        один
    } else if (2..=4).contains(&десятки) {
        два
    } else {
        много
    }
}

/// Напоминание об окончании подписки за 3 дня и за сутки.
///
/// Отметку о напоминании кладём в `subscriptions.updated_at`? Нет —
/// нужен отдельный след, иначе человек получит сообщение каждую минуту.
/// Используем `audit_log` как журнал уже отправленного.
async fn notify_expiring(pool: &Pool, bot_token: &str, cfg: &Config) -> Result<()> {
    let settings=sn_core::bot_config::load(pool).await?;
    if !sn_core::bot_config::flag(&settings,"bot.notify_enabled",true) {return Ok(());}
    let thresholds=sn_core::bot_config::notification_days(&settings);
    for (index,days) in thresholds.iter().copied().enumerate() {
        let lower=if index==0 {0} else {thresholds[index-1]};
        let rows = sqlx::query(
            "SELECT c.id, c.username, i.value AS tg, s.expires_at
               FROM clients c
               JOIN client_identities i ON i.client_id = c.id AND i.kind = 'telegram'
               JOIN subscriptions s     ON s.client_id = c.id AND s.is_current
              WHERE c.deleted_at IS NULL
                AND c.status = 'active'
                AND s.expires_at IS NOT NULL
                AND s.expires_at > now() + ($2 || ' days')::interval
                AND s.expires_at <= now() + ($1 || ' days')::interval
                -- Уже напоминали про этот порог — второй раз не шлём.
                --
                -- Два условия, и оба нужны. Первое привязано к сроку:
                -- после настоящего продления напомнить снова правильно.
                -- Но оно же и подводит — любой сдвиг `expires_at` (админ
                -- поправил дату, выдали доступ заново, доплатили пару
                -- дней) открывает окно заново, и человеку прилетает
                -- второе «осталось 3 дня» следом за первым. Поэтому
                -- второе условие — просто «не чаще раза в полсуток».
                AND NOT EXISTS (
                    SELECT 1 FROM audit_log a
                     WHERE a.entity_type = 'client' AND a.entity_id = c.id
                       AND a.action = 'notify.expiring'
                       AND a.payload->>'days' = $1
                       AND (a.created_at > s.expires_at - ($1 || ' days')::interval
                            OR a.created_at > now() - interval '12 hours')
                )
              LIMIT 200",
        )
        .bind(days.to_string())
        .bind(lower.to_string())
        .fetch_all(pool)
        .await?;

        for r in &rows {
            let client_id: i64 = r.get("id");
            let Ok(chat_id) = r.get::<String, _>("tg").parse::<i64>() else {
                continue;
            };
            let expires: chrono::DateTime<Utc> = r.get("expires_at");

            // Формулировка та же, что в кабинете: человеку приходит одно
            // и то же напоминание, откуда бы он ни смотрел.
            let text = if days == 1 {
                format!(
                    "⏳ Доступ заканчивается завтра, {}.\n\n\
                     Продлите заранее — так не прервётся доступ в интернет: /buy",
                    expires.format("%d.%m")
                )
            } else {
                format!(
                    "📅 Доступ заканчивается через {days} {} — {}.\n\n\
                     Продлите заранее — так не прервётся доступ в интернет: /buy",
                    склонение(days, "день", "дня", "дней"),
                    expires.format("%d.%m")
                )
            };

            let template=sn_core::bot_config::text(&settings,"bot.notify_text","");
            let text=if template.is_empty() {text} else {template.replace("{days}",&days.to_string()).replace("{date}",&expires.format("%d.%m.%Y").to_string())};
            let sent = send_message(bot_token, chat_id, &text).await;

            // Отметку ставим только при успешной отправке: иначе человек
            // не получит напоминание вовсе, а мы будем считать, что получил.
            if sent {
                sqlx::query(
                    "INSERT INTO audit_log (actor_kind, action, entity_type, entity_id, payload)
                     VALUES ('system', 'notify.expiring', 'client', $1, $2)",
                )
                .bind(client_id)
                .bind(json!({ "days": days.to_string() }))
                .execute(pool)
                .await?;
            }
        }

        if !rows.is_empty() {
            tracing::info!(days, count = rows.len(), "напоминания отправлены");
        }
    }
    let _ = cfg;
    Ok(())
}

async fn send_message(token: &str, chat_id: i64, text: &str) -> bool {
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    match reqwest::Client::new()
        .post(&url)
        .json(&json!({ "chat_id": chat_id, "text": text }))
        .send()
        .await
    {
        Ok(r) => r.status().is_success(),
        Err(e) => {
            tracing::warn!(error = %e.without_url(), chat_id, "не отправил напоминание");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn склонение_дней() {
        // Пороги напоминаний однажды расширят, и «через 5 дня» уйдёт
        // клиентам молча — проверяем всю таблицу окончаний.
        let д = |n| склонение(n, "день", "дня", "дней");
        assert_eq!(д(1), "день");
        assert_eq!(д(2), "дня");
        assert_eq!(д(4), "дня");
        assert_eq!(д(5), "дней");
        assert_eq!(д(7), "дней");
        // Одиннадцать-четырнадцать — исключение из общего правила.
        assert_eq!(д(11), "дней");
        assert_eq!(д(14), "дней");
        assert_eq!(д(21), "день");
        assert_eq!(д(22), "дня");
        assert_eq!(д(0), "дней");
    }
}

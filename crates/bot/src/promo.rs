//! Промокоды: проверка и применение к цене.
//!
//! Коды заводит администратор в панели, здесь их только проверяют. Вся
//! арифметика — в одном месте: и бот, и панель должны считать одинаково,
//! иначе клиент увидит одну сумму, а спишется другая.
//!
//! Проверка честная и подробная: «код не подошёл» заставляет человека
//! гадать, а «этот код только на первую покупку» — нет.

use chrono::Utc;
use sn_core::{Pool, Result};
use sqlx::Row;

/// Что даёт код.
#[derive(Debug, Clone, PartialEq)]
pub enum Discount {
    /// Процент от цены.
    Percent(i32),
    /// Фиксированная сумма в минорных единицах.
    Fixed(i64),
    /// Дни сверх оплаченного периода. Цену не меняет.
    Days(i32),
}

/// Годный код вместе с тем, что он даёт.
#[derive(Debug, Clone)]
pub struct Valid {
    pub id: i64,
    pub code: String,
    pub discount: Discount,
    pub currency: Option<String>,
}

impl Discount {
    /// Цена после скидки.
    ///
    /// Ниже нуля не уходим: отрицательная сумма — это не «бесплатно», а
    /// платёжка, которая откажет, и клиент без объяснений.
    pub fn apply(&self, amount: i64) -> i64 {
        match self {
            Discount::Percent(p) => (amount - amount * (*p as i64) / 100).max(0),
            Discount::Fixed(f) => (amount - f).max(0),
            Discount::Days(_) => amount,
        }
    }

    /// Сколько дней добавить сверх периода.
    pub fn bonus_days(&self) -> i32 {
        match self {
            Discount::Days(d) => *d,
            _ => 0,
        }
    }

    /// Как объяснить выгоду человеку.
    pub fn describe(&self) -> String {
        match self {
            Discount::Percent(p) => format!("−{p}%"),
            Discount::Fixed(f) => format!("−{}", *f as f64 / 100.0),
            Discount::Days(d) => format!("+{d} дн."),
        }
    }
}

/// Почему код не подошёл. Каждая причина — своя: человек должен понять,
/// что делать дальше, а не просто увидеть отказ.
#[derive(Debug, PartialEq)]
pub enum Rejected {
    Unknown,
    Expired,
    NotStarted,
    UsedUp,
    AlreadyUsed,
    FirstPurchaseOnly,
    OtherTariff,
}

impl Rejected {
    pub fn message(&self) -> &'static str {
        match self {
            Rejected::Unknown => "Такого кода нет — проверьте написание.",
            Rejected::Expired => "Срок действия кода истёк.",
            Rejected::NotStarted => "Код ещё не начал действовать.",
            Rejected::UsedUp => "Код исчерпан — его уже использовали.",
            Rejected::AlreadyUsed => "Вы уже пользовались этим кодом.",
            Rejected::FirstPurchaseOnly => "Код только для первой покупки.",
            Rejected::OtherTariff => "Код действует на другой тариф.",
        }
    }
}

/// Проверить код для конкретного клиента и тарифа.
pub async fn check(
    pool: &Pool,
    code: &str,
    client_id: i64,
    tariff_id: i64,
) -> Result<std::result::Result<Valid, Rejected>> {
    // Регистр не важен: люди набирают код как придётся, а отказ из-за
    // строчных букв выглядит как «код не работает».
    let row = sqlx::query(
        "SELECT id, code, kind::text AS kind, value, currency, max_uses, used_count,
                per_client_limit, valid_from, valid_until, tariff_id,
                first_purchase_only
           FROM promo_codes
          WHERE upper(code) = upper($1) AND is_active",
    )
    .bind(code.trim())
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(Err(Rejected::Unknown));
    };

    let now = Utc::now();
    if let Some(from) = r.get::<Option<chrono::DateTime<Utc>>, _>("valid_from") {
        if now < from {
            return Ok(Err(Rejected::NotStarted));
        }
    }
    if let Some(until) = r.get::<Option<chrono::DateTime<Utc>>, _>("valid_until") {
        if now > until {
            return Ok(Err(Rejected::Expired));
        }
    }

    let max_uses: Option<i32> = r.get("max_uses");
    let used: i32 = r.get("used_count");
    if max_uses.map(|m| used >= m).unwrap_or(false) {
        return Ok(Err(Rejected::UsedUp));
    }

    let only: Option<i64> = r.get("tariff_id");
    if only.map(|t| t != tariff_id).unwrap_or(false) {
        return Ok(Err(Rejected::OtherTariff));
    }

    let id: i64 = r.get("id");
    let per_client: i32 = r.get("per_client_limit");
    let mine: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM promo_redemptions WHERE promo_code_id = $1 AND client_id = $2",
    )
    .bind(id)
    .bind(client_id)
    .fetch_one(pool)
    .await?;
    if mine >= per_client as i64 {
        return Ok(Err(Rejected::AlreadyUsed));
    }

    if r.get::<bool, _>("first_purchase_only") {
        let paid: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM payments WHERE client_id = $1 AND status = 'success'",
        )
        .bind(client_id)
        .fetch_one(pool)
        .await?;
        if paid > 0 {
            return Ok(Err(Rejected::FirstPurchaseOnly));
        }
    }

    let value: i32 = r.get("value");
    let discount = match r.get::<String, _>("kind").as_str() {
        "percent" => Discount::Percent(value),
        "days" => Discount::Days(value),
        _ => Discount::Fixed(value as i64),
    };

    Ok(Ok(Valid { id, code: r.get("code"), currency:r.get("currency"), discount }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn процент_считается_от_цены() {
        assert_eq!(Discount::Percent(20).apply(1000), 800);
        assert_eq!(Discount::Percent(100).apply(1000), 0);
    }

    #[test]
    fn скидка_не_уводит_цену_в_минус() {
        // Отрицательная сумма — это не «бесплатно», а отказ платёжки
        // без объяснений для клиента.
        assert_eq!(Discount::Fixed(5000).apply(1000), 0);
        assert_eq!(Discount::Percent(150).apply(1000), 0);
    }

    #[test]
    fn бонусные_дни_цену_не_меняют() {
        assert_eq!(Discount::Days(7).apply(1000), 1000);
        assert_eq!(Discount::Days(7).bonus_days(), 7);
        assert_eq!(Discount::Percent(20).bonus_days(), 0);
    }

    #[test]
    fn у_каждого_отказа_своя_причина() {
        // «Код не подошёл» заставляет гадать — каждая причина объясняет,
        // что делать дальше.
        let all = [
            Rejected::Unknown, Rejected::Expired, Rejected::NotStarted,
            Rejected::UsedUp, Rejected::AlreadyUsed,
            Rejected::FirstPurchaseOnly, Rejected::OtherTariff,
        ];
        let msgs: Vec<&str> = all.iter().map(|r| r.message()).collect();
        let uniq: std::collections::HashSet<_> = msgs.iter().collect();
        assert_eq!(uniq.len(), msgs.len(), "сообщения не должны повторяться");
        assert!(msgs.iter().all(|m| !m.is_empty()));
    }

    #[test]
    fn выгода_объясняется_словами() {
        assert_eq!(Discount::Percent(20).describe(), "−20%");
        assert_eq!(Discount::Days(7).describe(), "+7 дн.");
    }
}

/// Запомнить выбранный код до оплаты.
///
/// Между вводом и нажатием «оплатить» несколько экранов, и код надо где-то
/// держать. Одна запись на клиента: применять два кода разом незачем,
/// новый просто заменяет прежний.
pub async fn hold(pool: &Pool, client_id: i64, v: &Valid) -> Result<()> {
    sqlx::query(
        "INSERT INTO bot_promo_hold (client_id, promo_id, code, expires_at)
         VALUES ($1, $2, $3, now() + interval '1 hour')
         ON CONFLICT (client_id) DO UPDATE
            SET promo_id = EXCLUDED.promo_id, code = EXCLUDED.code,
                expires_at = EXCLUDED.expires_at",
    )
    .bind(client_id)
    .bind(v.id)
    .bind(&v.code)
    .execute(pool)
    .await?;
    Ok(())
}

/// Действующий код клиента, если он есть и не протух.
///
/// Проверяем заново, а не доверяем записи: между вводом и оплатой код
/// могли исчерпать или выключить, и списать скидку по нему было бы
/// неверно уже перед самой платёжкой.
pub async fn held(
    pool: &Pool,
    client_id: i64,
    tariff_id: i64,
) -> Result<Option<Valid>> {
    let code: Option<String> = sqlx::query_scalar(
        "SELECT code FROM bot_promo_hold WHERE client_id = $1 AND expires_at > now()",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;

    let Some(code) = code else { return Ok(None) };
    match check(pool, &code, client_id, tariff_id).await? {
        Ok(v) => Ok(Some(v)),
        Err(_) => {
            // Перестал годиться — убираем, чтобы он не показывался
            // скидкой на следующем экране.
            let _ = release(pool, client_id).await;
            Ok(None)
        }
    }
}

/// Забыть код: после оплаты или когда он больше не подходит.
pub async fn release(pool: &Pool, client_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM bot_promo_hold WHERE client_id = $1")
        .bind(client_id)
        .execute(pool)
        .await?;
    Ok(())
}

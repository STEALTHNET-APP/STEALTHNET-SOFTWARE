/// Валюта сервиса. Она одна на всю установку: цены хранятся строкой на
/// каждую валюту, и показывать клиенту сразу несколько — значит показать
/// один и тот же срок дважды по разной цене.
///
/// Telegram Stars (`XTR`) — оговорённое исключение: это не вторая валюта
/// витрины, а способ оплаты, и всплывает он только на экране оплаты.
///
/// Спрашиваем базу, а не кэшируем: смену валюты в панели ждут сразу, а
/// запрос этот делается на переходах между экранами, не в цикле.
pub async fn service_currency(pool: &crate::Pool) -> String {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT value #>> '{}' FROM settings WHERE key = 'billing.currency'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    // Приводим к верхнему регистру: коды валют в ценах хранятся так, и
    // «rub» из настроек не совпал бы ни с одной строкой цен.
    .map(|c| c.trim().to_uppercase())
    .filter(|c| !c.is_empty())
    .unwrap_or_else(|| "USD".into())
}

/// Старые цены могут оставаться в БД после смены валюты владельцем.
/// Их нельзя использовать для новых покупок, включая старые кнопки бота.
pub async fn check_purchase_currency(pool: &crate::Pool, currency: &str, provider: &str) -> crate::Result<()> {
    let service = service_currency(pool).await;
    if currency == service || (currency == "XTR" && provider == "stars") {
        Ok(())
    } else {
        Err(crate::Error::bad("Валюта проекта изменилась. Откройте тарифы заново."))
    }
}

/// Деньги живут в минорных единицах (центах) целыми числами.
/// Форматирование — только на границе с человеком.
pub fn format_minor(amount_minor: i64, currency: &str) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let a = amount_minor.unsigned_abs();
    if matches!(currency, "XTR" | "JPY" | "KRW" | "VND" | "CLP" | "ISK") {
        return format!("{sign}{a} {currency}");
    }
    let symbol = match currency {
        "USD" => "$",
        "EUR" => "€",
        "RUB" => "₽",
        _ => "",
    };
    if symbol.is_empty() {
        format!("{sign}{}.{:02} {currency}", a / 100, a % 100)
    } else {
        format!("{sign}{symbol}{}.{:02}", a / 100, a % 100)
    }
}

/// Байты → человекочитаемое. Внутри системы трафик всегда в байтах.
///
/// Ступени те же и округление то же, что в кабинете: один и тот же
/// остаток трафика человек видит и в боте, и в мини-приложении, и
/// расхождение в цифре читается как ошибка счёта.
///
/// Ступени килобайт раньше не было вовсе, и всё мельче мегабайта
/// выпадало сырым числом — «63887 B» вместо «62 КБ».
pub fn format_bytes(bytes: i64) -> String {
    const ЕДИНИЦЫ: [&str; 5] = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
    let знак = if bytes < 0 { "-" } else { "" };
    let mut v = bytes.unsigned_abs() as f64;
    let mut i = 0;
    while v >= 1024.0 && i < ЕДИНИЦЫ.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    // Байты и килобайты — целыми: доля килобайта человеку не нужна.
    // От сотни тоже целыми: «184 ГБ» вместо «184.2 ГБ».
    let число = if i <= 1 || v >= 100.0 {
        format!("{v:.0}")
    } else {
        // Хвостовой ноль убираем: «10 ГБ» читается лучше «10.0 ГБ».
        let s = format!("{v:.1}");
        s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
    };
    format!("{знак}{число} {}", ЕДИНИЦЫ[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn деньги_не_теряют_копейки() {
        assert_eq!(format_minor(125, "XTR"), "125 XTR");
        assert_eq!(format_minor(i64::MIN, "JPY"), "-9223372036854775808 JPY");
        assert_eq!(format_minor(599, "USD"), "$5.99");
        assert_eq!(format_minor(0, "USD"), "$0.00");
        assert_eq!(format_minor(100000, "USD"), "$1000.00");
        assert_eq!(format_minor(-1250, "USD"), "-$12.50");
        assert_eq!(format_minor(50, "RUB"), "₽0.50");
    }

    #[test]
    fn байты_читаемо() {
        assert_eq!(format_bytes(0), "0 Б");
        assert_eq!(format_bytes(1024 * 1024), "1 МБ");
        assert_eq!(format_bytes(197_779_132_416), "184 ГБ");
        assert_eq!(format_bytes(1024i64.pow(4)), "1 ТБ");
    }

    #[test]
    fn мелкий_трафик_не_показывают_сырыми_байтами() {
        // «Трафик: 63887 B из 10.0 GiB» — именно это и выглядело пугающе:
        // ступени килобайт не было, и число вываливалось как есть.
        assert_eq!(format_bytes(63_887), "62 КБ");
        assert_eq!(format_bytes(1023), "1023 Б");
        assert_eq!(format_bytes(1024), "1 КБ");
    }

    #[test]
    fn единицы_совпадают_с_кабинетом() {
        // Кабинет считает по тем же ступеням; разные цифры на одном и
        // том же остатке читаются как ошибка счёта.
        assert_eq!(format_bytes(10 * 1024i64.pow(3)), "10 ГБ");
        assert_eq!(format_bytes(500 * 1024i64.pow(3)), "500 ГБ");
        assert_eq!(format_bytes(1024i64.pow(3) * 3 / 2), "1.5 ГБ");
        assert_eq!(format_bytes(-1024), "-1 КБ");
    }
}

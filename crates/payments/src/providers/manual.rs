//! Ручной приём оплаты: администратор отмечает платёж сам.
//!
//! Включён по умолчанию и не требует ключей. Нужен для переводов на карту,
//! наличных и разбора спорных ситуаций, а ещё как пример минимального модуля.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::{Invoice, InvoiceRequest, PaymentProvider, PaymentStatus, WebhookOutcome};
use sn_core::{Error, Result};

// Все валюты из выбора валюты проекта в панели. Stars оплачиваются своим модулем.
const DEFAULT_CURRENCIES: &str = "USD,EUR,RUB,UAH,KZT,TRY,GBP";

fn configured_currencies(value: Option<&str>) -> Vec<String> {
    let mut currencies: Vec<String> = value.unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    if currencies.is_empty() {
        currencies = DEFAULT_CURRENCIES.split(',').map(str::to_owned).collect();
    }
    currencies
}

pub struct Manual {
    /// Валюты берём из настроек: у каждого свой набор.
    currencies: Vec<String>,
    /// Инструкция, которую увидит клиент: реквизиты, куда писать.
    instructions: String,
}

impl Manual {
    pub fn new() -> Self {
        let currencies = configured_currencies(std::env::var("PAY_MANUAL_CURRENCIES").ok().as_deref());
        let instructions = std::env::var("PAY_MANUAL_INSTRUCTIONS")
            .unwrap_or_else(|_| "Свяжитесь с поддержкой для оплаты вручную.".into());
        Self { currencies, instructions }
    }
}

#[async_trait]
impl PaymentProvider for Manual {
    fn id(&self) -> &'static str {
        "manual"
    }

    fn title(&self) -> &str {
        "Вручную"
    }

    fn currencies(&self) -> Vec<String> {
        self.currencies.clone()
    }

    /// Ручной приём не требует ключей — он настроен всегда.
    fn is_configured(&self) -> bool {
        true
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn sort_order(&self) -> i32 {
        900
    }

    async fn create_invoice(&self, req: &InvoiceRequest) -> Result<Invoice> {
        Ok(Invoice {
            // Ссылки нет: клиент видит инструкцию, платёж подтверждает админ.
            pay_url: String::new(),
            external_id: None,
            expires_in_minutes: 60 * 24,
            payload: serde_json::json!({
                "instructions": self.instructions,
                "description": req.description,
            }),
        })
    }

    async fn handle_webhook(
        &self,
        _headers: &HashMap<String, String>,
        _body: &[u8],
    ) -> Result<WebhookOutcome> {
        // У ручного приёма нет вебхуков: подтверждение приходит из панели.
        Err(Error::bad("ручной приём не принимает вебхуки"))
    }

    async fn poll_status(&self, _external_id: &str) -> Result<Option<PaymentStatus>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_or_empty_currency_setting_accepts_every_panel_currency() {
        for value in [None, Some(""), Some(" , \t, ")] {
            assert_eq!(configured_currencies(value), ["USD", "EUR", "RUB", "UAH", "KZT", "TRY", "GBP"]);
        }
    }

    #[test]
    fn explicit_currency_setting_is_preserved() {
        assert_eq!(configured_currencies(Some(" rub, usd, , eur ")), ["RUB", "USD", "EUR"]);
    }
}

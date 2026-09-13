//! Ручной приём оплаты: администратор отмечает платёж сам.
//!
//! Всегда доступен и ничего не требует. Нужен для переводов на карту,
//! наличных и разбора спорных ситуаций, а ещё как пример минимального модуля.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::{Invoice, InvoiceRequest, PaymentProvider, PaymentStatus, WebhookOutcome};
use sn_core::{Error, Result};

pub struct Manual {
    /// Валюты берём из настроек: у каждого свой набор.
    currencies: Vec<String>,
    /// Инструкция, которую увидит клиент: реквизиты, куда писать.
    instructions: String,
}

impl Manual {
    pub fn new() -> Self {
        let currencies = std::env::var("PAY_MANUAL_CURRENCIES")
            .unwrap_or_else(|_| "USD,RUB,EUR".into())
            .split(',')
            .map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty())
            .collect();
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

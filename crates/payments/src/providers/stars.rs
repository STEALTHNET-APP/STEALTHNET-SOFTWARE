//! Telegram Stars (валюта `XTR`).
//!
//! Оплата прямо внутри бота, без внешних платёжных систем и без ключей —
//! достаточно токена бота. Поэтому это самый простой способ начать продавать.
//!
//! Особенности, из-за которых модуль выглядит иначе остальных:
//!  * У Stars нет копеек: 150 XTR — это ровно 150, делить на 100 нельзя.
//!  * Уведомление приходит не HTTP-вебхуком от платёжной системы, а обычным
//!    апдейтом бота (`successful_payment`). Бот передаёт его сюда как «вебхук».
//!  * Наш `payment_id` летает в `invoice_payload` и возвращается обратно —
//!    это и есть связь платежа с подпиской.

use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use crate::{Invoice, InvoiceRequest, PaymentProvider, PaymentStatus, WebhookOutcome};
use sn_core::{Error, Result};

pub struct TelegramStars {
    cfg: crate::Settings,
    http: reqwest::Client,
}

impl TelegramStars {
    pub fn new() -> Self {
        Self { cfg: crate::Settings::default(), http: reqwest::Client::new() }
    }

    /// Токен того же бота, что общается с клиентами: счёт в звёздах
    /// выставляется от его имени.
    fn token(&self) -> Result<String> {
        self.cfg
            .get_or_env("bot_token", "BOT_TOKEN")
            .ok_or_else(|| Error::Internal("токен бота не задан".into()))
    }

    fn api(&self, method: &str) -> Result<String> {
        Ok(format!("https://api.telegram.org/bot{}/{method}", self.token()?))
    }
}

#[async_trait]
impl PaymentProvider for TelegramStars {
    fn id(&self) -> &'static str {
        "stars"
    }

    fn title(&self) -> &str {
        "Telegram Stars"
    }

    fn currencies(&self) -> Vec<String> {
        vec!["XTR".into()]
    }

    fn is_configured(&self) -> bool {
        self.token().is_ok()
    }

    fn settings_schema(&self) -> Vec<crate::SettingField> {
        vec![crate::SettingField::secret(
            "bot_token",
            "Токен бота",
            "Тот же бот, что общается с клиентами: счёт в звёздах выставляется от его имени. Пусто — берётся BOT_TOKEN сервиса",
        )
        .required(false)]
    }

    fn settings(&self) -> Option<crate::Settings> {
        Some(self.cfg.clone())
    }

    fn sort_order(&self) -> i32 {
        10
    }

    async fn create_invoice(&self, req: &InvoiceRequest) -> Result<Invoice> {
        if req.currency != "XTR" {
            return Err(Error::bad("Telegram Stars принимает только XTR"));
        }

        // createInvoiceLink даёт ссылку, которую можно положить в кнопку.
        // provider_token для Stars должен быть пустым — это требование API.
        let body = json!({
            "title": "Подписка",
            "description": req.description,
            "payload": req.payment_id.to_string(),
            "provider_token": "",
            "currency": "XTR",
            "prices": [{ "label": req.description, "amount": req.amount_minor }],
        });

        let res: serde_json::Value = self
            .http
            .post(self.api("createInvoiceLink")?)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Telegram API недоступен: {e}")))?
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Telegram вернул не JSON: {e}")))?;

        if res["ok"] != true {
            let desc = res["description"].as_str().unwrap_or("неизвестная ошибка");
            return Err(Error::Internal(format!("Telegram отказал: {desc}")));
        }

        let link = res["result"]
            .as_str()
            .ok_or_else(|| Error::Internal("Telegram не вернул ссылку".into()))?;

        Ok(Invoice {
            pay_url: link.to_string(),
            external_id: None,
            expires_in_minutes: 60,
            payload: json!({ "kind": "telegram_stars" }),
        })
    }

    /// Сюда бот передаёт `successful_payment` из апдейта.
    /// Подпись проверять не нужно: данные пришли по доверенному каналу
    /// от самого Telegram, а не снаружи по HTTP.
    async fn handle_webhook(
        &self,
        _headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<WebhookOutcome> {
        let v: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| Error::bad(format!("не разобрал апдейт: {e}")))?;

        let sp = &v["successful_payment"];
        if sp.is_null() {
            return Err(Error::bad("в апдейте нет successful_payment"));
        }

        // payload — это наш payment_id, положенный при создании счёта.
        let payment_id = sp["invoice_payload"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok());

        Ok(WebhookOutcome {
            external_event_id: sp["telegram_payment_charge_id"].as_str().map(String::from),
            payment_id,
            provider_txid: sp["telegram_payment_charge_id"].as_str().map(String::from),
            status: PaymentStatus::Success,
            amount_minor: sp["total_amount"].as_i64(),
            currency: sp["currency"].as_str().map(String::from),
            error: None,
            raw: v,
        })
    }
}

/// Verify the exact invoice, owner, amount and currency before Telegram charges Stars.
pub async fn validate_checkout(pool: &sn_core::Pool, q: &serde_json::Value) -> Result<bool> {
    let Some(id) = q["invoice_payload"].as_str().and_then(|s| s.parse::<i64>().ok()) else { return Ok(false) };
    let Some(amount) = q["total_amount"].as_i64().filter(|a| *a > 0) else { return Ok(false) };
    let Some(user) = q["from"]["id"].as_i64().filter(|id| *id > 0) else { return Ok(false) };
    if q["currency"].as_str() != Some("XTR") { return Ok(false) }
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payments p JOIN clients c ON c.id=p.client_id
        JOIN client_identities i ON i.client_id=c.id AND i.kind='telegram'
        WHERE p.id=$1 AND p.provider='stars' AND p.status='pending' AND p.currency='XTR'
          AND p.amount_minor=$2 AND i.value=$3 AND c.deleted_at IS NULL
          AND (p.expires_at IS NULL OR p.expires_at>now()))")
        .bind(id).bind(amount).bind(user.to_string()).fetch_one(pool).await?)
}

#[cfg(test)]
mod trust_boundary_tests {
    use super::*;
    #[test]
    fn stars_notifications_are_internal_only() {
        assert!(!TelegramStars::new().accepts_http_webhooks());
        assert!(!crate::providers::manual::Manual::new().accepts_http_webhooks());
        assert!(crate::providers::cryptobot::CryptoBot::new().accepts_http_webhooks());
        assert!(crate::providers::generic_http::GenericHttp::new().accepts_http_webhooks());
    }
}

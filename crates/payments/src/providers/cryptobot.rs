//! Crypto Pay API (@CryptoBot) — приём USDT, TON, BTC и прочей крипты.
//!
//! Пример модуля с внешним HTTP-API и проверкой подписи вебхука.
//! Подпись у Crypto Pay считается так: ключ = SHA-256 от API-токена,
//! затем HMAC-SHA256 этим ключом по сырому телу запроса.
//! Сравнивать нужно именно сырое тело — пересобранный из JSON текст
//! даст другую подпись из-за порядка полей.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::{minor_to_units, Invoice, InvoiceRequest, PaymentProvider, PaymentStatus, WebhookOutcome};
use sn_core::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

pub struct CryptoBot {
    cfg: crate::Settings,
    http: reqwest::Client,
}

impl CryptoBot {
    pub fn new() -> Self {
        Self { cfg: crate::Settings::default(), http: reqwest::Client::new() }
    }

    fn token(&self) -> Result<String> {
        self.cfg
            .get_or_env("token", "PAY_CRYPTOBOT_TOKEN")
            .ok_or_else(|| Error::Internal("токен CryptoBot не задан".into()))
    }

    /// Валюта счёта. Крипто-провайдер сам пересчитает по курсу,
    /// если попросить фиатную сумму.
    fn asset(&self) -> String {
        self.cfg
            .get_or_env("asset", "PAY_CRYPTOBOT_ASSET")
            .unwrap_or_else(|| "USDT".into())
    }

    /// Проверка подписи. Возвращает false при любом сомнении —
    /// пропустить чужой вебхук страшнее, чем отклонить свой.
    fn signature_valid(&self, headers: &HashMap<String, String>, body: &[u8]) -> bool {
        let Ok(token) = self.token() else { return false };
        let Some(sig) = headers
            .get("crypto-pay-api-signature")
            .or_else(|| headers.get("Crypto-Pay-Api-Signature"))
        else {
            return false;
        };

        let secret = Sha256::digest(token.as_bytes());
        let Ok(mut mac) = HmacSha256::new_from_slice(&secret) else {
            return false;
        };
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());

        // Сравнение постоянного времени: обычное == утекает по таймингу.
        constant_time_eq(expected.as_bytes(), sig.as_bytes())
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn parse_minor_units(v: &serde_json::Value) -> Option<i64> {
    if let Ok(minor) = super::hosted::amount_minor(v) {
        return Some(minor);
    }
    let raw = match v {
        serde_json::Value::String(s) => s.trim(),
        _ => return None,
    };
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let fraction_trimmed = fraction.trim_end_matches('0');
    if fraction_trimmed.len() <= 2
        && !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.bytes().all(|b| b.is_ascii_digit())
    {
        let w = whole.parse::<i64>().ok()?;
        let f = if fraction_trimmed.is_empty() {
            0
        } else {
            fraction_trimmed.parse::<i64>().ok()? * if fraction_trimmed.len() == 1 { 10 } else { 1 }
        };
        w.checked_mul(100)?.checked_add(f)
    } else {
        None
    }
}

#[async_trait]
impl PaymentProvider for CryptoBot {
    fn accepts_http_webhooks(&self) -> bool { true }

    fn id(&self) -> &'static str {
        "cryptobot"
    }

    fn title(&self) -> &str {
        "Криптовалюта"
    }

    fn currencies(&self) -> Vec<String> {
        // Счёт выставляем в фиате, а платит человек криптой по курсу.
        vec!["USD".into(), "EUR".into(), "RUB".into()]
    }

    fn is_configured(&self) -> bool {
        self.token().is_ok()
    }

    fn settings_schema(&self) -> Vec<crate::SettingField> {
        vec![
            crate::SettingField::secret(
                "token",
                "API-токен",
                "Выдаёт @CryptoBot: Crypto Pay → My Apps → Create App",
            ),
            crate::SettingField::text(
                "asset",
                "Актив счёта",
                "В чём выставлять счёт: USDT, TON, BTC. Курс к валюте тарифа пересчитает провайдер",
            )
            .with_default("USDT"),
        ]
    }

    fn settings(&self) -> Option<crate::Settings> {
        Some(self.cfg.clone())
    }

    fn sort_order(&self) -> i32 {
        20
    }

    async fn create_invoice(&self, req: &InvoiceRequest) -> Result<Invoice> {
        let body = json!({
            "currency_type": "fiat",
            "fiat": req.currency,
            "amount": minor_to_units(req.amount_minor, &req.currency),
            "accepted_assets": self.asset(),
            "description": req.description,
            "payload": req.payment_id.to_string(),
            "expires_in": 3600,
            "allow_comments": false,
        });

        let res: serde_json::Value = self
            .http
            .post("https://pay.crypt.bot/api/createInvoice")
            .header("Crypto-Pay-API-Token", self.token()?)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Crypto Pay недоступен: {e}")))?
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Crypto Pay вернул не JSON: {e}")))?;

        if res["ok"] != true {
            let err = res["error"].to_string();
            return Err(Error::Internal(format!("Crypto Pay отказал: {err}")));
        }

        let result = &res["result"];
        let url = result["bot_invoice_url"]
            .as_str()
            .or_else(|| result["pay_url"].as_str())
            .ok_or_else(|| Error::Internal("Crypto Pay не вернул ссылку".into()))?;

        Ok(Invoice {
            pay_url: url.to_string(),
            external_id: result["invoice_id"].as_i64().map(|v| v.to_string()),
            expires_in_minutes: 60,
            payload: json!({ "asset": self.asset(), "invoice_id": result["invoice_id"] }),
        })
    }

    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<WebhookOutcome> {
        if !self.signature_valid(headers, body) {
            return Err(Error::Forbidden);
        }

        let v: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| Error::bad(format!("не разобрал вебхук: {e}")))?;

        let payload = &v["payload"];
        let status = match payload["status"].as_str() {
            Some("paid") => PaymentStatus::Success,
            Some("expired") => PaymentStatus::Canceled,
            _ => PaymentStatus::Pending,
        };

        // `payload` внутри счёта — это наш payment_id.
        let payment_id = payload["payload"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok());

        let fiat = payload["fiat"].as_str().filter(|s| !s.trim().is_empty());
        let is_fiat = payload["currency_type"].as_str() == Some("fiat") || fiat.is_some();

        let raw_amount = if is_fiat {
            &payload["amount"]
        } else {
            payload
                .get("paid_amount")
                .filter(|v| !v.is_null() && v.as_str() != Some(""))
                .unwrap_or(&payload["amount"])
        };

        let amount_minor = parse_minor_units(raw_amount);

        let currency = fiat
            .or_else(|| payload["asset"].as_str().filter(|s| !s.trim().is_empty()))
            .map(String::from);

        Ok(WebhookOutcome {
            external_event_id: v["update_id"].as_i64().map(|v| v.to_string()),
            payment_id,
            provider_txid: payload["invoice_id"].as_i64().map(|v| v.to_string()),
            status,
            amount_minor,
            currency,
            error: None,
            raw: v,
        })
    }

    async fn poll_status(&self, external_id: &str) -> Result<Option<PaymentStatus>> {
        let res: serde_json::Value = self
            .http
            .get("https://pay.crypt.bot/api/getInvoices")
            .header("Crypto-Pay-API-Token", self.token()?)
            .query(&[("invoice_ids", external_id)])
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Crypto Pay недоступен: {e}")))?
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Crypto Pay вернул не JSON: {e}")))?;

        let status = res["result"]["items"][0]["status"].as_str();
        Ok(match status {
            Some("paid") => Some(PaymentStatus::Success),
            Some("expired") => Some(PaymentStatus::Canceled),
            Some(_) => Some(PaymentStatus::Pending),
            None => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn сравнение_постоянного_времени() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    /// Модуль с заданным токеном — как если бы его вписали в панели.
    fn with_token(token: &str) -> CryptoBot {
        let cb = CryptoBot::new();
        let mut m = serde_json::Map::new();
        m.insert("token".into(), serde_json::json!(token));
        cb.cfg.replace(m);
        cb
    }

    #[test]
    fn без_токена_подпись_не_проходит() {
        // Пустые настройки и пустое окружение: подпись проверить нечем.
        let cb = CryptoBot::new();
        let mut h = HashMap::new();
        h.insert("crypto-pay-api-signature".to_string(), "deadbeef".to_string());
        assert!(!cb.signature_valid(&h, b"{}"));
    }

    #[test]
    fn неверная_подпись_отклоняется() {
        let cb = with_token("test-token");
        let mut h = HashMap::new();
        h.insert("crypto-pay-api-signature".to_string(), "00".repeat(32));
        assert!(!cb.signature_valid(&h, b"{\"update_id\":1}"));
    }

    #[test]
    fn верная_подпись_принимается() {
        let token = "test-token";
        let body = br#"{"update_id":1,"payload":{"status":"paid"}}"#;
        let secret = Sha256::digest(token.as_bytes());
        let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        let cb = with_token(token);
        let mut h = HashMap::new();
        h.insert("crypto-pay-api-signature".to_string(), sig);
        assert!(cb.signature_valid(&h, body));
    }

    #[tokio::test]
    async fn fiat_webhook_extracts_amount_and_fiat_currency() {
        let token = "test-token";
        let body = json!({
            "update_id": 101,
            "payload": {
                "invoice_id": 999,
                "status": "paid",
                "currency_type": "fiat",
                "fiat": "USD",
                "amount": "19.99",
                "paid_amount": "0.005123",
                "paid_asset": "TON",
                "payload": "42"
            }
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let secret = Sha256::digest(token.as_bytes());
        let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
        mac.update(&bytes);
        let sig = hex::encode(mac.finalize().into_bytes());

        let cb = with_token(token);
        let mut h = HashMap::new();
        h.insert("crypto-pay-api-signature".to_string(), sig);

        let outcome = cb.handle_webhook(&h, &bytes).await.unwrap();
        assert_eq!(outcome.external_event_id.as_deref(), Some("101"));
        assert_eq!(outcome.payment_id, Some(42));
        assert_eq!(outcome.provider_txid.as_deref(), Some("999"));
        assert_eq!(outcome.status, PaymentStatus::Success);
        assert_eq!(outcome.amount_minor, Some(1999));
        assert_eq!(outcome.currency.as_deref(), Some("USD"));
    }

    #[tokio::test]
    async fn crypto_webhook_falls_back_to_asset_when_fiat_missing() {
        let token = "test-token";
        let body = json!({
            "update_id": 102,
            "payload": {
                "invoice_id": 1000,
                "status": "paid",
                "currency_type": "crypto",
                "asset": "USDT",
                "fiat": null,
                "amount": "15.00",
                "paid_amount": "15.00",
                "payload": "43"
            }
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let secret = Sha256::digest(token.as_bytes());
        let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
        mac.update(&bytes);
        let sig = hex::encode(mac.finalize().into_bytes());

        let cb = with_token(token);
        let mut h = HashMap::new();
        h.insert("crypto-pay-api-signature".to_string(), sig);

        let outcome = cb.handle_webhook(&h, &bytes).await.unwrap();
        assert_eq!(outcome.external_event_id.as_deref(), Some("102"));
        assert_eq!(outcome.payment_id, Some(43));
        assert_eq!(outcome.provider_txid.as_deref(), Some("1000"));
        assert_eq!(outcome.status, PaymentStatus::Success);
        assert_eq!(outcome.amount_minor, Some(1500));
        assert_eq!(outcome.currency.as_deref(), Some("USDT"));
    }

    #[tokio::test]
    async fn crypto_webhook_handles_trailing_zeros_and_empty_fiat() {
        let token = "test-token";
        let body = json!({
            "update_id": 103,
            "payload": {
                "invoice_id": 1001,
                "status": "paid",
                "currency_type": "crypto",
                "asset": "TON",
                "fiat": "",
                "amount": "15.00",
                "paid_amount": "15.000000",
                "payload": "44"
            }
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let secret = Sha256::digest(token.as_bytes());
        let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
        mac.update(&bytes);
        let sig = hex::encode(mac.finalize().into_bytes());

        let cb = with_token(token);
        let mut h = HashMap::new();
        h.insert("crypto-pay-api-signature".to_string(), sig);

        let outcome = cb.handle_webhook(&h, &bytes).await.unwrap();
        assert_eq!(outcome.external_event_id.as_deref(), Some("103"));
        assert_eq!(outcome.payment_id, Some(44));
        assert_eq!(outcome.provider_txid.as_deref(), Some("1001"));
        assert_eq!(outcome.status, PaymentStatus::Success);
        assert_eq!(outcome.amount_minor, Some(1500));
        assert_eq!(outcome.currency.as_deref(), Some("TON"));
    }
}

use std::collections::HashMap;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use serde_json::json;
use sn_payments::{PaymentProvider, PaymentStatus};
use sn_payments::providers::cryptobot::CryptoBot;
use sn_payments::providers::generic_http::GenericHttp;

type HmacSha256 = Hmac<Sha256>;

fn cryptobot_with_token(token: &str) -> CryptoBot {
    let cb = CryptoBot::new();
    let mut m = serde_json::Map::new();
    m.insert("token".into(), json!(token));
    cb.settings().unwrap().replace(m);
    cb
}

fn generic_http_cfg(pairs: &[(&str, &str)]) -> GenericHttp {
    let p = GenericHttp::new();
    let mut m = serde_json::Map::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), json!(v));
    }
    p.settings().unwrap().replace(m);
    p
}

#[tokio::test]
async fn cryptobot_fiat_invoice_extracts_amount_rather_than_paid_crypto_amount() {
    let token = "test-secret-token";
    let body = json!({
        "update_id": 99991,
        "payload": {
            "invoice_id": 123456,
            "status": "paid",
            "currency_type": "fiat",
            "fiat": "USD",
            "amount": "25.50",
            "paid_amount": "0.00789123",
            "paid_asset": "TON",
            "payload": "100"
        }
    });
    let bytes = serde_json::to_vec(&body).unwrap();
    let secret = Sha256::digest(token.as_bytes());
    let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
    mac.update(&bytes);
    let sig = hex::encode(mac.finalize().into_bytes());

    let cb = cryptobot_with_token(token);
    let mut headers = HashMap::new();
    headers.insert("crypto-pay-api-signature".to_string(), sig);

    let outcome = cb.handle_webhook(&headers, &bytes).await.expect("valid signature");
    assert_eq!(outcome.external_event_id.as_deref(), Some("99991"));
    assert_eq!(outcome.payment_id, Some(100));
    assert_eq!(outcome.provider_txid.as_deref(), Some("123456"));
    assert_eq!(outcome.status, PaymentStatus::Success);
    assert_eq!(outcome.amount_minor, Some(2550));
    assert_eq!(outcome.currency.as_deref(), Some("USD"));
}

#[tokio::test]
async fn cryptobot_crypto_invoice_falls_back_to_asset_when_fiat_null() {
    let token = "test-secret-token";
    let body = json!({
        "update_id": 99992,
        "payload": {
            "invoice_id": 123457,
            "status": "paid",
            "currency_type": "crypto",
            "asset": "USDT",
            "fiat": null,
            "amount": "10.00",
            "paid_amount": "10.00",
            "payload": "101"
        }
    });
    let bytes = serde_json::to_vec(&body).unwrap();
    let secret = Sha256::digest(token.as_bytes());
    let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
    mac.update(&bytes);
    let sig = hex::encode(mac.finalize().into_bytes());

    let cb = cryptobot_with_token(token);
    let mut headers = HashMap::new();
    headers.insert("crypto-pay-api-signature".to_string(), sig);

    let outcome = cb.handle_webhook(&headers, &bytes).await.expect("valid signature");
    assert_eq!(outcome.external_event_id.as_deref(), Some("99992"));
    assert_eq!(outcome.payment_id, Some(101));
    assert_eq!(outcome.provider_txid.as_deref(), Some("123457"));
    assert_eq!(outcome.status, PaymentStatus::Success);
    assert_eq!(outcome.amount_minor, Some(1000));
    assert_eq!(outcome.currency.as_deref(), Some("USDT"));
}

#[tokio::test]
async fn generic_http_webhook_accepts_both_lowercase_and_uppercase_hmac() {
    let secret = "webhook-secret-key-123";
    let body = br#"{"order":"555","state":"PAID","amount":"49.90","currency":"EUR"}"#;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let raw_sig = hex::encode(mac.finalize().into_bytes());

    let p = generic_http_cfg(&[
        ("webhook_secret", secret),
        ("webhook_signature_header", "x-signature"),
        ("webhook_payment_id_path", "order"),
        ("webhook_status_path", "state"),
        ("webhook_success_value", "paid"),
        ("webhook_amount_path", "amount"),
        ("webhook_currency_path", "currency"),
    ]);

    // 1. Lowercase hex signature
    let mut headers_lower = HashMap::new();
    headers_lower.insert("x-signature".to_string(), raw_sig.to_lowercase());
    let outcome_lower = p.handle_webhook(&headers_lower, body).await.expect("lowercase signature accepted");
    assert_eq!(outcome_lower.payment_id, Some(555));
    assert_eq!(outcome_lower.status, PaymentStatus::Success);
    assert_eq!(outcome_lower.amount_minor, Some(4990));
    assert_eq!(outcome_lower.currency.as_deref(), Some("EUR"));

    // 2. Uppercase hex signature
    let mut headers_upper = HashMap::new();
    headers_upper.insert("x-signature".to_string(), raw_sig.to_uppercase());
    let outcome_upper = p.handle_webhook(&headers_upper, body).await.expect("uppercase signature accepted");
    assert_eq!(outcome_upper.payment_id, Some(555));
    assert_eq!(outcome_upper.status, PaymentStatus::Success);
    assert_eq!(outcome_upper.amount_minor, Some(4990));
    assert_eq!(outcome_upper.currency.as_deref(), Some("EUR"));

    // 3. Invalid signature rejected
    let mut headers_invalid = HashMap::new();
    headers_invalid.insert("x-signature".to_string(), "badf00d".to_string());
    assert!(p.handle_webhook(&headers_invalid, body).await.is_err());

    // 4. Mixed-case and whitespace-padded signature accepted
    let mut headers_mixed = HashMap::new();
    let mixed_sig = format!("  {}  ", raw_sig.chars().enumerate().map(|(i, c)| if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).collect::<String>());
    headers_mixed.insert("x-signature".to_string(), mixed_sig);
    let outcome_mixed = p.handle_webhook(&headers_mixed, body).await.expect("mixed-case whitespace signature accepted");
    assert_eq!(outcome_mixed.payment_id, Some(555));
    assert_eq!(outcome_mixed.amount_minor, Some(4990));
}

#[tokio::test]
async fn cryptobot_crypto_invoice_with_trailing_zeros_and_empty_fiat() {
    let token = "test-secret-token";
    let body = json!({
        "update_id": 99993,
        "payload": {
            "invoice_id": 123458,
            "status": "paid",
            "currency_type": "crypto",
            "asset": "TON",
            "fiat": "",
            "amount": "10.00",
            "paid_amount": "10.000000",
            "payload": "102"
        }
    });
    let bytes = serde_json::to_vec(&body).unwrap();
    let secret = Sha256::digest(token.as_bytes());
    let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
    mac.update(&bytes);
    let sig = hex::encode(mac.finalize().into_bytes());

    let cb = cryptobot_with_token(token);
    let mut headers = HashMap::new();
    headers.insert("crypto-pay-api-signature".to_string(), sig);

    let outcome = cb.handle_webhook(&headers, &bytes).await.expect("valid signature");
    assert_eq!(outcome.external_event_id.as_deref(), Some("99993"));
    assert_eq!(outcome.payment_id, Some(102));
    assert_eq!(outcome.provider_txid.as_deref(), Some("123458"));
    assert_eq!(outcome.status, PaymentStatus::Success);
    assert_eq!(outcome.amount_minor, Some(1000));
    assert_eq!(outcome.currency.as_deref(), Some("TON"));
}


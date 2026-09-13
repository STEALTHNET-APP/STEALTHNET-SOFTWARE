//! Универсальный модуль для платёжек с обычным REST API.
//!
//! Нужен затем, что настоящий модуль — это скомпилированный Rust-код: чтобы
//! добавить его, проект надо пересобрать, а на живой панели это невозможно.
//! Большинству платёжных систем при этом нужно одно и то же: POST с суммой,
//! ссылка на оплату в ответе и вебхук со статусом. Всё это описывается
//! настройками, поэтому такую платёжку можно завести прямо в панели.
//!
//! Чего этот модуль намеренно НЕ умеет: нестандартные схемы подписи (кроме
//! HMAC-SHA256 и простого секрета в заголовке), многошаговые протоколы,
//! автосписание. Для них пишется свой модуль — см. `crates/payments/README.md`.

use std::collections::HashMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::{
    Invoice, InvoiceRequest, PaymentProvider, PaymentStatus, SettingField, Settings, WebhookOutcome,
};
use sn_core::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

pub struct GenericHttp {
    cfg: Settings,
    http: reqwest::Client,
}

impl GenericHttp {
    pub fn new() -> Self {
        Self { cfg: Settings::default(), http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none()).build().expect("HTTP client") }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.cfg.get(key)
    }

    /// Достаёт значение по пути вида `data.result.url` — провайдеры кладут
    /// ссылку на оплату кто в корень, кто на третий уровень вложенности.
    fn pick<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
        let mut cur = v;
        for part in path.split('.') {
            if part.is_empty() {
                continue;
            }
            cur = match cur {
                Value::Object(m) => m.get(part)?,
                Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(cur)
    }

    fn pick_str(v: &Value, path: &str) -> Option<String> {
        match Self::pick(v, path)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /// Подставляет значения запроса в шаблон тела.
    ///
    /// Плейсхолдеры намеренно простые: провайдеру нужно передать сумму,
    /// валюту и наш идентификатор платежа, по которому мы узнаем счёт,
    /// когда придёт уведомление.
    fn fill(tpl: &str, req: &InvoiceRequest, ret: &str) -> String {
        let escaped = |s: &str| { let json = serde_json::to_string(s).expect("string JSON"); json[1..json.len()-1].to_owned() };
        tpl.replace("{amount}", &crate::minor_to_units(req.amount_minor, &req.currency))
            .replace("{amount_minor}", &req.amount_minor.to_string())
            .replace("{currency}", &escaped(&req.currency))
            .replace("{payment_id}", &req.payment_id.to_string())
            .replace("{description}", &escaped(&req.description))
            .replace("{return_url}", &escaped(ret))
    }
}

#[async_trait]
impl PaymentProvider for GenericHttp {
    fn accepts_http_webhooks(&self) -> bool { true }

    fn id(&self) -> &'static str {
        "http"
    }

    fn title(&self) -> &str {
        "Своя платёжка"
    }

    fn currencies(&self) -> Vec<String> {
        self.get("currencies")
            .map(|s| {
                s.split(',')
                    .map(|c| c.trim().to_uppercase())
                    .filter(|c| !c.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_configured(&self) -> bool {
        self.get("invoice_url").is_some()
            && !self.currencies().is_empty()
            && self.get("pay_url_path").is_some()
            && self.get("webhook_secret").is_some()
            && self.get("webhook_amount_path").is_some()
            && self.get("webhook_currency_path").is_some()
    }

    fn sort_order(&self) -> i32 {
        500
    }

    fn settings_schema(&self) -> Vec<SettingField> {
        vec![
            SettingField::text("display_title", "Название на кнопке", "Что увидит клиент в боте")
                .with_default("Оплата картой"),
            SettingField::text(
                "currencies",
                "Валюты через запятую",
                "Например: RUB, USD. Модуль предлагается только для этих валют",
            ),
            SettingField::text(
                "invoice_url",
                "URL выставления счёта",
                "POST-адрес API платёжной системы",
            ),
            SettingField::secret(
                "auth_header",
                "Заголовок авторизации",
                "Целиком, как требует провайдер: например «Authorization: Bearer КЛЮЧ»",
            )
            .required(false),
            SettingField::text(
                "body_template",
                "Шаблон тела запроса (JSON)",
                "Плейсхолдеры: {amount}, {amount_minor}, {currency}, {payment_id}, {description}, {return_url}",
            ),
            SettingField::text(
                "pay_url_path",
                "Путь к ссылке оплаты в ответе",
                "Например data.url — по нему модуль достанет ссылку из ответа",
            ),
            SettingField::text(
                "external_id_path",
                "Путь к id счёта в ответе",
                "Необязательно: пригодится для сверки платежей",
            ),
            SettingField::secret(
                "webhook_secret",
                "Секрет вебхука",
                "Проверяется как HMAC-SHA256 тела запроса. Без него уведомления НЕ принимаются: иначе подписку можно получить бесплатно",
            ),
            SettingField::text(
                "webhook_signature_header",
                "Заголовок с подписью",
                "Как называется заголовок, в котором провайдер шлёт подпись",
            )
            .with_default("x-signature"),
            SettingField::text(
                "webhook_payment_id_path",
                "Путь к нашему id платежа в уведомлении",
                "То же значение, что вы передали как {payment_id}",
            ),
            SettingField::text(
                "webhook_status_path",
                "Путь к статусу в уведомлении",
                "Например status",
            ),
            SettingField::text("webhook_amount_path", "Путь к оплаченной сумме", "Например amount или data.amount; без суммы доступ не выдаётся").required(true),
            SettingField::text("webhook_currency_path", "Путь к валюте платежа", "Например currency; проверяется совпадение с валютой счёта").required(true),
            SettingField::text("webhook_amount_units", "Единицы суммы", "units — рубли/доллары; minor — копейки/центы").with_default("units"),
            SettingField::text(
                "webhook_success_value",
                "Значение статуса «оплачено»",
                "Например paid или success — сравнивается без учёта регистра",
            ),
        ]
    }

    fn settings(&self) -> Option<Settings> {
        Some(self.cfg.clone())
    }

    fn validate_settings(&self, cfg: &serde_json::Map<String, Value>) -> Result<()> {
        if let Some(raw) = cfg.get("invoice_url").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            let u = reqwest::Url::parse(raw).map_err(|_| Error::bad("URL платёжного API должен использовать HTTPS"))?;
            if u.scheme() != "https" || !u.username().is_empty() || u.password().is_some() {
                return Err(Error::bad("URL платёжного API должен использовать HTTPS без логина и пароля"));
            }
        }
        if cfg.get("webhook_amount_units").and_then(Value::as_str).is_some_and(|s| !s.is_empty() && !matches!(s,"units"|"minor")) {
            return Err(Error::bad("Единицы суммы: units или minor"));
        }
        Ok(())
    }

    async fn create_invoice(&self, req: &InvoiceRequest) -> Result<Invoice> {
        let url = self
            .get("invoice_url")
            .ok_or_else(|| Error::Internal("не задан URL выставления счёта".into()))?;
        self.validate_settings(&self.cfg.snapshot())?;
        let tpl = self.get("body_template").unwrap_or_else(|| "{}".into());
        let ret = req.return_url.clone().unwrap_or_default();

        let filled = Self::fill(&tpl, req, &ret);
        let body: Value = serde_json::from_str(&filled)
            .map_err(|e| Error::bad(format!("шаблон тела не разобрался как JSON: {e}")))?;

        let mut rq = self.http.post(&url).json(&body);
        if let Some(h) = self.get("auth_header") {
            if let Some((name, value)) = h.split_once(':') {
                rq = rq.header(name.trim(), value.trim());
            }
        }

        let res = rq
            .send()
            .await
            .map_err(|e| Error::Internal(format!("платёжка недоступна: {e}")))?;
        let status = res.status();
        let payload: Value = res.json().await.unwrap_or(Value::Null);

        if !status.is_success() {
            return Err(Error::Internal(format!(
                "платёжка ответила {status}"
            )));
        }

        let path = self
            .get("pay_url_path")
            .ok_or_else(|| Error::Internal("не задан путь к ссылке оплаты".into()))?;
        let pay_url = Self::pick_str(&payload, &path).ok_or_else(|| {
            Error::Internal(format!("в ответе платёжки нет поля «{path}»"))
        })?;

        Ok(Invoice {
            pay_url,
            external_id: self
                .get("external_id_path")
                .and_then(|p| Self::pick_str(&payload, &p)),
            expires_in_minutes: 60,
            payload,
        })
    }

    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<WebhookOutcome> {
        // Вебхук — открытый endpoint, авторизацию даёт только подпись.
        // Без секрета принимать нечего: любой смог бы «оплатить» подписку.
        let secret = self
            .get("webhook_secret")
            .ok_or_else(|| Error::Internal("не задан секрет вебхука".into()))?;
        let header_name = self
            .get("webhook_signature_header")
            .unwrap_or_else(|| "x-signature".into())
            .to_lowercase();

        let got = headers
            .get(&header_name)
            .ok_or_else(|| Error::bad(format!("нет заголовка {header_name}")))?;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|_| Error::Internal("плохой секрет".into()))?;
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());

        // Сравнение постоянного времени: по времени ответа подпись подбирается.
        let ok = got.len() == expected.len()
            && got
                .bytes()
                .zip(expected.bytes())
                .fold(0u8, |a, (x, y)| a | (x ^ y))
                == 0;
        if !ok {
            return Err(Error::Forbidden);
        }

        let raw: Value = serde_json::from_slice(body).unwrap_or(Value::Null);

        let payment_id = self
            .get("webhook_payment_id_path")
            .and_then(|p| Self::pick_str(&raw, &p))
            .and_then(|v| v.parse::<i64>().ok());

        let status_value = self
            .get("webhook_status_path")
            .and_then(|p| Self::pick_str(&raw, &p))
            .unwrap_or_default();
        let success = self
            .get("webhook_success_value")
            .unwrap_or_else(|| "success".into());

        let status = if status_value.eq_ignore_ascii_case(&success) {
            PaymentStatus::Success
        } else {
            PaymentStatus::Pending
        };

        let (amount_minor, currency) = if status == PaymentStatus::Success {
            let amount_path = self.get("webhook_amount_path").ok_or_else(|| Error::bad("настройте путь к сумме платежа"))?;
            let currency_path = self.get("webhook_currency_path").ok_or_else(|| Error::bad("настройте путь к валюте платежа"))?;
            let currency = Self::pick_str(&raw,&currency_path).filter(|s| s.len()==3 && s.bytes().all(|b|b.is_ascii_alphabetic()))
                .ok_or_else(|| Error::bad("нет валюты платежа"))?.to_uppercase();
            let value = Self::pick(&raw,&amount_path).ok_or_else(|| Error::bad("нет суммы платежа"))?;
            let minor = self.get("webhook_amount_units").as_deref()==Some("minor")
                || matches!(currency.as_str(),"XTR"|"JPY"|"KRW"|"VND"|"CLP"|"ISK");
            let amount = if minor {
                let value = Self::pick_str(&raw,&amount_path).unwrap_or_default();
                if value.is_empty() || !value.bytes().all(|b|b.is_ascii_digit()) { return Err(Error::bad("сумма в minor должна быть целым положительным числом")); }
                value.parse::<i64>().map_err(|_| Error::bad("сумма слишком велика"))?
            } else { super::hosted::amount_minor(value)? };
            if payment_id.is_none() { return Err(Error::bad("нет номера оплаченного счёта")); }
            (Some(amount), Some(currency))
        } else { (None,None) };

        Ok(WebhookOutcome {
            external_event_id: None,
            payment_id,
            provider_txid: None,
            status,
            amount_minor,
            currency,
            error: None,
            raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(pairs: &[(&str, &str)]) -> GenericHttp {
        let p = GenericHttp::new();
        let mut m = serde_json::Map::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), json!(v));
        }
        p.cfg.replace(m);
        p
    }

    #[test]
    fn путь_достаёт_вложенное_поле() {
        let v = json!({ "data": { "result": { "url": "https://pay.example/1" } } });
        assert_eq!(
            GenericHttp::pick_str(&v, "data.result.url").as_deref(),
            Some("https://pay.example/1")
        );
        assert_eq!(GenericHttp::pick_str(&v, "data.nope"), None);
    }

    #[test]
    fn шаблон_подставляет_значения() {
        let req = InvoiceRequest {
            payment_id: 42,
            client_id: 1,
            telegram_id: None,
            amount_minor: 59900,
            currency: "RUB".into(),
            description: "PRO · 30 дней".into(),
            return_url: None,
        };
        let out = GenericHttp::fill(
            r#"{"sum":"{amount}","cur":"{currency}","order":"{payment_id}"}"#,
            &req,
            "",
        );
        assert!(out.contains(r#""sum":"599.00""#), "{out}");
        assert!(out.contains(r#""order":"42""#), "{out}");
        // Шаблон обязан оставаться валидным JSON после подстановки.
        serde_json::from_str::<Value>(&out).expect("подстановка сломала JSON");
    }

    #[test]
    fn без_обязательных_полей_модуль_не_настроен() {
        assert!(!GenericHttp::new().is_configured());
        assert!(!cfg(&[("invoice_url", "https://x")]).is_configured());
        assert!(cfg(&[
            ("invoice_url", "https://x"),
            ("currencies", "RUB"),
            ("pay_url_path", "url"),
            ("webhook_secret", "test"),
            ("webhook_amount_path", "amount"),
            ("webhook_currency_path", "currency"),
        ])
        .is_configured());
    }

    #[test]
    fn валюты_разбираются_из_строки() {
        let p = cfg(&[("currencies", " rub , usd ")]);
        assert_eq!(p.currencies(), vec!["RUB".to_string(), "USD".to_string()]);
    }

    #[tokio::test]
    async fn вебхук_без_секрета_отклоняется() {
        // Без секрета модуль не должен принимать уведомления вовсе —
        // иначе подписку можно получить одним HTTP-запросом.
        let p = cfg(&[("invoice_url", "https://x")]);
        let r = p.handle_webhook(&HashMap::new(), b"{}").await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn неверная_подпись_отклоняется() {
        let p = cfg(&[
            ("webhook_secret", "s3cret"),
            ("webhook_signature_header", "x-signature"),
        ]);
        let mut h = HashMap::new();
        h.insert("x-signature".to_string(), "deadbeef".to_string());
        assert!(p.handle_webhook(&h, b"{}").await.is_err());
    }

    #[tokio::test]
    async fn верная_подпись_принимается() {
        let secret = "s3cret";
        let body = br#"{"order":"77","state":"PAID","amount":"19.99","currency":"RUB"}"#;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        let p = cfg(&[
            ("webhook_secret", secret),
            ("webhook_signature_header", "x-signature"),
            ("webhook_payment_id_path", "order"),
            ("webhook_status_path", "state"),
            ("webhook_success_value", "paid"),
            ("webhook_amount_path", "amount"),
            ("webhook_currency_path", "currency"),
        ]);
        let mut h = HashMap::new();
        h.insert("x-signature".to_string(), sig);

        let out = p.handle_webhook(&h, body).await.expect("подпись верная");
        assert_eq!(out.payment_id, Some(77));
        assert_eq!(out.status, PaymentStatus::Success);
        assert_eq!(out.amount_minor, Some(1999));
        assert_eq!(out.currency.as_deref(), Some("RUB"));
    }
    #[tokio::test]
    async fn signed_success_requires_precise_amount_currency_and_order() {
        let p = cfg(&[("webhook_secret","test"),("webhook_payment_id_path","order"),
            ("webhook_status_path","state"),("webhook_success_value","paid"),
            ("webhook_amount_path","amount"),("webhook_currency_path","currency")]);
        let valid = json!({"order":77,"state":"paid","amount":"19.99","currency":"USD"});
        let mut invalid = Vec::new();
        for key in ["order","amount","currency"] {
            let mut body = valid.clone(); body.as_object_mut().unwrap().remove(key); invalid.push(body);
        }
        for amount in ["-1", "19.999", "NaN", "999999999999999999999"] {
            let mut body = valid.clone(); body["amount"] = json!(amount); invalid.push(body);
        }
        for body in invalid {
            let bytes = serde_json::to_vec(&body).unwrap();
            let mut mac = HmacSha256::new_from_slice(b"test").unwrap(); mac.update(&bytes);
            let headers = HashMap::from([("x-signature".into(),hex::encode(mac.finalize().into_bytes()))]);
            assert!(p.handle_webhook(&headers,&bytes).await.is_err(), "accepted {body}");
        }
    }

    #[test]
    fn template_values_cannot_inject_json_fields() {
        let description = "test\" ,\"amount\":0,\"other\":\"\\\n\r\t";
        let req = InvoiceRequest { payment_id:1,client_id:1,telegram_id:None,amount_minor:1999,
            currency:"USD".into(),description:description.into(),return_url:None };
        let text = GenericHttp::fill(r#"{"description":"{description}","sum":"{amount}"}"#, &req, "");
        let value:Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["description"], description);
        assert_eq!(value["sum"], "19.99");
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

}

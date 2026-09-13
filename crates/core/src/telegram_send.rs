//! Shared Telegram delivery semantics for previews and the broadcast worker.
use serde_json::{json, Value};
pub const API: &str = "https://api.telegram.org";
#[derive(Debug, PartialEq)]
pub enum Delivery {
    Sent,
    Permanent(String),
    Retry { seconds: u64, error: String },
    Stop(String),
}
pub fn client() -> crate::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| crate::Error::Internal("Telegram HTTP client".into()))
}
pub fn personalize(template: &str, name: &str, tariff: &str, expires: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    };
    template
        .replace("{expire_date}", &esc(expires))
        .replace("{tariff}", &esc(tariff))
        .replace("{name}", &esc(name))
}
pub async fn send(
    http: &reqwest::Client,
    endpoint: &str,
    token: &str,
    chat: i64,
    text: &str,
    keyboard: &Option<Value>,
) -> Delivery {
    send_with_photo(http, endpoint, token, chat, text, keyboard, None).await
}
pub struct Photo {
    pub data: Vec<u8>,
    pub content_type: String,
}
pub async fn load_photo(
    pool: &crate::Pool,
    id: Option<uuid::Uuid>,
) -> crate::Result<Option<Photo>> {
    use sqlx::Row;
    let Some(id) = id else { return Ok(None) };
    let row = sqlx::query("SELECT data,content_type FROM broadcast_media WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(crate::Error::NotFound)?;
    Ok(Some(Photo {
        data: row.get("data"),
        content_type: row.get("content_type"),
    }))
}
pub async fn send_with_photo(
    http: &reqwest::Client,
    endpoint: &str,
    token: &str,
    chat: i64,
    text: &str,
    keyboard: &Option<Value>,
    photo: Option<&Photo>,
) -> Delivery {
    let mut body = json!({"chat_id":chat,"text":text,"parse_mode":"HTML","link_preview_options":{"is_disabled":true}});
    if let Some(kb) = keyboard {
        body["reply_markup"] = kb.clone();
    }
    let request = if let Some(photo) = photo {
        let ext = if photo.content_type == "image/png" {
            "png"
        } else {
            "jpg"
        };
        let part = match reqwest::multipart::Part::bytes(photo.data.clone())
            .file_name(format!("photo.{ext}"))
            .mime_str(&photo.content_type)
        {
            Ok(part) => part,
            Err(_) => return Delivery::Stop("Некорректный формат фото".into()),
        };
        let mut form = reqwest::multipart::Form::new()
            .text("chat_id", chat.to_string())
            .text("caption", text.to_owned())
            .text("parse_mode", "HTML")
            .part("photo", part);
        if let Some(kb) = keyboard {
            form = form.text("reply_markup", kb.to_string());
        }
        http.post(format!("{endpoint}/bot{token}/sendPhoto"))
            .multipart(form)
    } else {
        http.post(format!("{endpoint}/bot{token}/sendMessage"))
            .json(&body)
    };
    match request.send().await {
        Ok(r) => {
            let status = r.status().as_u16();
            let value = r.json::<Value>().await.unwrap_or(Value::Null);
            classify(status, &value)
        }
        // reqwest errors include the URL containing the bot token. Never persist it.
        Err(_) => Delivery::Retry {
            seconds: 15,
            error: "Telegram недоступен: ошибка сети или время ожидания истекло".into(),
        },
    }
}
fn classify(status: u16, v: &Value) -> Delivery {
    if (200..300).contains(&status) && v["ok"] == true {
        return Delivery::Sent;
    }
    let code = v["error_code"].as_u64().unwrap_or(status as u64);
    let desc = v["description"]
        .as_str()
        .unwrap_or("Некорректный ответ Telegram")
        .chars()
        .take(400)
        .collect::<String>();
    if code == 429 {
        return Delivery::Retry {
            seconds: v["parameters"]["retry_after"]
                .as_u64()
                .unwrap_or(30)
                .clamp(1, 86400),
            error: desc,
        };
    }
    if code >= 500 || (200..300).contains(&code) {
        return Delivery::Retry {
            seconds: 15,
            error: desc,
        };
    }
    if code == 403 || desc.contains("chat not found") || desc.contains("user is deactivated") {
        return Delivery::Permanent(desc);
    }
    // Bad markup/buttons and invalid bot credentials must not retry forever or
    // prevent another campaign from running. The operator sees the exact cause.
    Delivery::Stop(desc)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn telegram_response_classification() {
        assert_eq!(classify(200, &json!({"ok":true})), Delivery::Sent);
        assert!(matches!(
            classify(
                400,
                &json!({"description":"Bad Request: can't parse entities"})
            ),
            Delivery::Stop(_)
        ));
        assert!(matches!(
            classify(401, &json!({"description":"Unauthorized"})),
            Delivery::Stop(_)
        ));
        assert!(matches!(
            classify(403, &json!({"description":"bot blocked"})),
            Delivery::Permanent(_)
        ));
        assert!(matches!(
            classify(500, &Value::Null),
            Delivery::Retry { .. }
        ));
        assert!(matches!(
            classify(200, &Value::Null),
            Delivery::Retry { .. }
        ));
        assert!(matches!(
            classify(429, &json!({"parameters":{"retry_after":120}})),
            Delivery::Retry { seconds: 120, .. }
        ));
    }
    #[test]
    fn customer_fields_are_escaped() {
        assert_eq!(
            personalize("<b>{name}</b> {tariff}", "A<&", "VIP<", "—"),
            "<b>A&lt;&amp;</b> VIP&lt;"
        );
    }
}

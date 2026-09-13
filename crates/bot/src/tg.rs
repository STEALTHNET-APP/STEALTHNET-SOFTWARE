//! Тонкая обёртка над Telegram Bot API.
//!
//! Framework сознательно не берём: боту нужно пять методов, а фреймворк
//! тянет свою модель жизненного цикла и обновляется по своему графику.

use serde_json::{json, Value};
use sn_core::{Error, Result};

#[derive(Clone)]
pub struct Tg {
    token: String,
    api_base: String,
    http: reqwest::Client,
}

/// Кнопка inline-клавиатуры: либо callback, либо ссылка, либо оплата.
#[allow(dead_code)]
pub enum Btn {
    Data(String, String),
    Url(String, String),
    Pay(String),
    /// Мини-приложение: открывается внутри Telegram, без выхода в браузер.
    /// Адрес обязан быть на https — по http Telegram кнопку не покажет.
    WebApp(String, String),
    Icon(Box<Btn>, String),
}

impl Btn {
    pub fn with_icon(self, id: Option<&str>) -> Self {
        match id.and_then(sn_core::bot_config::emoji_id) {
            Some(id) => Self::Icon(Box::new(self), id),
            None => self,
        }
    }
    fn to_json(&self) -> Value {
        match self {
            Btn::Icon(button, id) => {
                let mut value = button.to_json();
                let text = value["text"].as_str().unwrap_or("");
                // Replace a leading ordinary emoji, keeping the complete action label.
                let clean = text.trim_start_matches(|c: char| c.is_whitespace()
                    || matches!(c as u32, 0x1f000..=0x1faff | 0x2600..=0x27bf | 0xfe0f | 0x200d | 0x20e3));
                if !clean.is_empty() { value["text"] = json!(clean); }
                value["icon_custom_emoji_id"] = json!(id);
                value
            },
            Btn::Data(text, data) => json!({ "text": text, "callback_data": data }),
            Btn::Url(text, url) => json!({ "text": text, "url": url }),
            Btn::Pay(text) => json!({ "text": text, "pay": true }),
            Btn::WebApp(text, url) => json!({ "text": text, "web_app": { "url": url } }),
        }
    }
}

/// Клавиатура строками — так её удобнее собирать на месте.
pub fn keyboard(rows: Vec<Vec<Btn>>) -> Value {
    json!({
        "inline_keyboard": rows.iter()
            .map(|row| row.iter().map(Btn::to_json).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    })
}

fn remove_custom_icons(value: &mut Value) -> bool {
    match value {
        Value::Object(fields) => {
            let mut removed = fields.remove("icon_custom_emoji_id").is_some();
            for nested in fields.values_mut() { removed |= remove_custom_icons(nested); }
            removed
        },
        Value::Array(values) => values.iter_mut().fold(false, |removed, value| remove_custom_icons(value) | removed),
        _ => false,
    }
}

impl Tg {
    pub fn new(token: String) -> Self {
        Self {
            token,
            api_base: "https://api.telegram.org".into(),
            http: reqwest::Client::builder()
                // Long-polling держит соединение открытым: таймаут должен быть
                // заметно больше, чем таймаут самого polling.
                .timeout(std::time::Duration::from_secs(65))
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn call(&self, method: &str, body: Value) -> Result<Value> {
        match self.call_once(method, body.clone()).await {
            Err(error) if {
                let message = error.to_string().to_lowercase();
                message.contains("telegram отказал") && (message.contains("emoji") || message.contains("premium"))
            } => {
                let mut fallback = body;
                if !remove_custom_icons(&mut fallback) { return Err(error); }
                tracing::warn!(method, "Telegram отклонил эмодзи кнопки — повторяем без иконок");
                self.call_once(method, fallback).await
            },
            result => result,
        }
    }

    async fn call_once(&self, method: &str, body: Value) -> Result<Value> {
        let url = format!("{}/bot{}/{method}", self.api_base,self.token);
        let res: Value = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Telegram недоступен ({method}): {}", e.without_url())))?
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Telegram вернул не JSON ({method}): {}", e.without_url())))?;

        if res["ok"] != true {
            let desc = res["description"].as_str().unwrap_or("неизвестно");
            return Err(Error::Internal(format!("Telegram отказал ({method}): {desc}")));
        }
        Ok(res["result"].clone())
    }

    pub async fn get_me(&self) -> Result<Value> {
        self.call("getMe", json!({})).await
    }

    /// Забирает апдейты. `offset` = последний обработанный + 1.
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<Value>> {
        let res = self
            .call(
                "getUpdates",
                json!({
                    "offset": offset,
                    "timeout": 25,
                    "allowed_updates": ["message", "callback_query", "pre_checkout_query"],
                }),
            )
            .await?;
        Ok(res.as_array().cloned().unwrap_or_default())
    }

    pub async fn send_chat_id(&self, chat_id:i64, topic:Option<i64>)->Result<()> {
        let text=format!("ID группы: {chat_id}{}\n\nУкажите эти значения в панели: Бот → Уведомления команды.",topic.map(|id|format!("\nID темы: {id}")).unwrap_or_default());
        let mut body=json!({"chat_id":chat_id,"text":text});
        if let Some(id)=topic {body["message_thread_id"]=json!(id);}
        self.call("sendMessage",body).await?;Ok(())
    }

    pub async fn send(&self, chat_id: i64, text: &str, kb: Option<Value>) -> Result<i64> {
        let mut body = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "link_preview_options": { "is_disabled": true },
        });
        if let Some(kb) = kb.clone() {
            body["reply_markup"] = kb;
        }
        let res = match self.call("sendMessage", body.clone()).await {
            Err(e) if e.to_string().contains("parse entities") => {
                body.as_object_mut().unwrap().remove("parse_mode");
                self.call("sendMessage",body).await?
            }
            other=>other?,
        };
        Ok(res["message_id"].as_i64().unwrap_or(0))
    }

    /// Правит существующее сообщение — так меню не засоряет переписку.
    pub async fn edit(&self, chat_id: i64, message_id: i64, text: &str, kb: Option<Value>) -> Result<()> {
        let mut body = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
            "parse_mode": "HTML",
            "link_preview_options": { "is_disabled": true },
        });
        if let Some(kb) = kb.clone() {
            body["reply_markup"] = kb;
        }
        match self.call("editMessageText", body.clone()).await {
            Ok(_) => Ok(()),
            // «message is not modified» — не ошибка: пользователь нажал ту же кнопку.
            Err(e) if e.to_string().contains("not modified") => Ok(()),
            Err(e) if e.to_string().contains("no text") => {
                body["caption"]=body["text"].take();
                body.as_object_mut().unwrap().remove("text");
                body.as_object_mut().unwrap().remove("link_preview_options");
                match self.call("editMessageCaption",body).await {
                    Ok(_)=>Ok(()), Err(e) if e.to_string().contains("not modified")=>Ok(()),
                    Err(_)=>self.replace(chat_id,message_id,text,None,kb).await.map(|_| ())
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Экран: текст с картинкой или без.
    ///
    /// Telegram различает сообщения с фото и без: у первых текст живёт в
    /// `caption`, у вторых — в `text`, и править одно как другое нельзя.
    /// Поэтому переключение между видами делаем удалением и отправкой
    /// заново — иначе экран с картинкой, открытый поверх текстового,
    /// молча не обновлялся бы.
    ///
    /// Возвращает id сообщения, чтобы следующий экран правил именно его.
    pub async fn screen(
        &self,
        chat_id: i64,
        edit: Option<i64>,
        text: &str,
        photo: Option<&str>,
        kb: Option<Value>,
    ) -> Result<i64> {
        let photo=photo.filter(|_| text.chars().count()<=1024);
        match (edit, photo) {
            // Было фото, стало фото — меняем и картинку, и подпись.
            (Some(mid), Some(url)) => {
                let mut body = json!({
                    "chat_id": chat_id,
                    "message_id": mid,
                    "media": { "type": "photo", "media": url,
                               "caption": cut_caption(text), "parse_mode": "HTML" },
                });
                if let Some(kb) = kb.clone() {
                    body["reply_markup"] = kb;
                }
                match self.call("editMessageMedia", body).await {
                    Ok(_) => Ok(mid),
                    Err(e) if e.to_string().contains("not modified") => Ok(mid),
                    // Сообщение было без фото — правкой это не исправить.
                    Err(_) => self.replace(chat_id, mid, text, photo, kb).await,
                }
            }
            (Some(mid), None) => match self.edit(chat_id, mid, text, kb.clone()).await {
                Ok(()) => Ok(mid),
                // Сообщение было с фото: у него нет поля text.
                Err(_) => self.replace(chat_id, mid, text, None, kb).await,
            },
            (None, Some(url)) => self.send_photo(chat_id, url, text, kb).await,
            (None, None) => self.send(chat_id, text, kb).await,
        }
    }

    /// Удалить и отправить заново — когда вид сообщения меняется.
    async fn replace(
        &self,
        chat_id: i64,
        mid: i64,
        text: &str,
        photo: Option<&str>,
        kb: Option<Value>,
    ) -> Result<i64> {
        let sent=match photo {
            Some(url) => self.send_photo(chat_id,url,text,kb).await,
            None => self.send(chat_id,text,kb).await,
        }?;
        self.delete(chat_id,mid).await.ok();
        Ok(sent)
    }

    pub async fn send_photo(
        &self,
        chat_id: i64,
        photo: &str,
        caption: &str,
        kb: Option<Value>,
    ) -> Result<i64> {
        let mut body = json!({
            "chat_id": chat_id,
            "photo": photo,
            "caption": cut_caption(caption),
            "parse_mode": "HTML",
        });
        if let Some(kb) = kb.clone() {
            body["reply_markup"] = kb;
        }
        match self.call("sendPhoto", body).await {
            Ok(res) => Ok(res["message_id"].as_i64().unwrap_or(0)),
            // Битая ссылка на картинку не должна оставлять человека без
            // экрана вовсе: показываем то же самое текстом.
            Err(e) => {
                tracing::warn!(error = %e, photo, "картинка не отправилась — шлём текстом");
                self.send(chat_id, caption, kb).await
            }
        }
    }

    pub async fn delete(&self, chat_id: i64, message_id: i64) -> Result<()> {
        self.call("deleteMessage", json!({ "chat_id": chat_id, "message_id": message_id }))
            .await?;
        Ok(())
    }

    /// Состоит ли человек в канале.
    ///
    /// `Ok(false)` — не состоит либо канал недоступен боту: для гостя это
    /// одно и то же, а администратор увидит причину в панели.
    pub async fn is_member(&self, chat: &str, user_id: i64) -> Result<bool> {
        let res = self
            .call(
                "getChatMember",
                json!({ "chat_id": chat, "user_id": user_id }),
            )
            .await?;
        let status = res["status"].as_str().unwrap_or("left");
        Ok(matches!(status, "creator" | "administrator" | "member") || (status=="restricted" && res["is_member"]==true))
    }

    /// Гасит «часики» на кнопке. Без этого клиент Telegram висит в ожидании.
    pub async fn answer_callback(&self, id: &str, text: Option<&str>) -> Result<()> {
        let mut body = json!({ "callback_query_id": id });
        if let Some(t) = text {
            body["text"] = json!(t);
        }
        self.call("answerCallbackQuery", body).await?;
        Ok(())
    }

    /// Подтверждение перед списанием. Ответить обязательно в течение 10 секунд,
    /// иначе Telegram отменит платёж.
    pub async fn answer_pre_checkout(&self, id: &str, ok: bool, error: Option<&str>) -> Result<()> {
        let mut body = json!({ "pre_checkout_query_id": id, "ok": ok });
        if let Some(e) = error {
            body["error_message"] = json!(e);
        }
        self.call("answerPreCheckoutQuery", body).await?;
        Ok(())
    }

    pub async fn set_commands(&self, commands: &[(&str, &str)]) -> Result<()> {
        let list: Vec<Value> = commands
            .iter()
            .map(|(c, d)| json!({ "command": c, "description": d }))
            .collect();
        self.call("setMyCommands", json!({ "commands": list })).await?;
        Ok(())
    }
}

/// Подпись под фото у Telegram ограничена 1024 символами.
///
/// Текст экрана бывает длиннее — например, список тарифов с описаниями.
/// Молча получить отказ API и не показать ничего хуже, чем показать
/// урезанное: обрезаем по границе строки, чтобы не рвать разметку.
fn cut_caption(text: &str) -> String {
    const LIMIT: usize = 1024;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let mut out = String::new();
    for line in text.lines() {
        if out.chars().count() + line.chars().count() + 1 > LIMIT - 1 {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_icon_keeps_button_action_and_large_string_id() {
        for button in [Btn::Data("🚀 Купить".into(),"buy".into()), Btn::Url("💬 Канал".into(),"https://t.me/example".into()), Btn::WebApp("🔌 Подключиться".into(),"https://example.com".into())] {
            let before=button.to_json();
            let after=button.with_icon(Some("[5215361191051798408]")).to_json();
            assert_eq!(after["icon_custom_emoji_id"], "5215361191051798408");
            for key in ["callback_data","url","web_app"] { assert_eq!(after[key],before[key]); }
            assert!(!after["text"].as_str().unwrap().starts_with(['🚀','💬','🔌']));
        }
    }
    #[test]
    fn короткая_подпись_не_меняется() {
        assert_eq!(cut_caption("привет"), "привет");
    }

    #[test]
    fn длинная_подпись_режется_по_строкам() {
        let text = (0..200).map(|i| format!("строка номер {i}")).collect::<Vec<_>>().join("\n");
        let cut = cut_caption(&text);
        assert!(cut.chars().count() <= 1024, "влезает в лимит: {}", cut.chars().count());
        assert!(cut.ends_with('…'), "видно, что текст обрезан");
        // Резать нужно по строкам: обрыв посреди тега сломал бы разметку.
        assert!(cut.contains("строка номер 0"));
        assert!(!cut.contains("строка номер 199"));
    }
}

#[cfg(test)] pub(crate) mod transport_tests {
    use super::*;
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    pub(crate) async fn mock(replies:Vec<Value>)->(Tg,tokio::task::JoinHandle<Vec<(String,Value)>>) {
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {
            let mut requests=Vec::new();
            for reply in replies {
                let (mut stream,_)=listener.accept().await.unwrap();let mut buf=Vec::new();
                let (end,length)=loop {
                    let mut b=[0u8;4096];let n=stream.read(&mut b).await.unwrap();assert!(n>0);buf.extend_from_slice(&b[..n]);
                    if let Some(end)=buf.windows(4).position(|w|w==b"\r\n\r\n") {
                        let head=String::from_utf8_lossy(&buf[..end]);let length=head.lines().find_map(|l|l.to_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);
                        if buf.len()>=end+4+length {break(end,length);}
                    }
                };
                let path=String::from_utf8_lossy(&buf[..end]).split_whitespace().nth(1).unwrap().to_string();
                let body=serde_json::from_slice(&buf[end+4..end+4+length]).unwrap();requests.push((path,body));
                let body=reply.to_string();let response=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        let mut tg=Tg::new("test-only".into());tg.api_base=format!("http://{address}");(tg,server)
    }
    #[tokio::test] async fn rejected_premium_icon_retries_same_action_without_icon() {
        let (tg,server)=mock(vec![json!({"ok":false,"description":"Bad Request: CUSTOM_EMOJI_INVALID"}),json!({"ok":true,"result":{"message_id":5}})]).await;
        let kb=keyboard(vec![vec![Btn::Url("Канал".into(),"https://t.me/example".into()).with_icon(Some("5215361191051798408"))]]);
        assert_eq!(tg.send(1,"Меню",Some(kb)).await.unwrap(),5);
        let requests=server.await.unwrap();assert_eq!(requests.len(),2);
        let first=&requests[0].1["reply_markup"]["inline_keyboard"][0][0];
        let second=&requests[1].1["reply_markup"]["inline_keyboard"][0][0];
        assert!(first.get("icon_custom_emoji_id").is_some());assert!(second.get("icon_custom_emoji_id").is_none());
        assert_eq!(first["url"],second["url"]);assert_eq!(first["text"],second["text"]);
    }
    #[tokio::test] async fn photo_callback_edits_caption() {
        let (tg,server)=mock(vec![json!({"ok":false,"description":"Bad Request: there is no text in the message to edit"}),json!({"ok":true,"result":{}})]).await;
        tg.edit(1,2,"Тарифы",Some(keyboard(vec![vec![Btn::Data("Назад".into(),"menu".into())]]))).await.unwrap();
        let r=server.await.unwrap();assert!(r[1].0.ends_with("editMessageCaption"));assert_eq!(r[1].1["caption"],"Тарифы");assert!(!r[1].1["reply_markup"].is_null());
    }
    #[tokio::test] async fn replacement_is_sent_before_old_menu_is_deleted() {
        let (tg,server)=mock(vec![json!({"ok":false,"description":"message cannot be edited"}),json!({"ok":true,"result":{"message_id":3}}),json!({"ok":true,"result":true})]).await;
        assert_eq!(tg.screen(1,Some(2),"Меню",Some("photo-id"),None).await.unwrap(),3);
        let r=server.await.unwrap();assert!(r[1].0.ends_with("sendPhoto"));assert!(r[2].0.ends_with("deleteMessage"));
    }
    #[tokio::test] async fn bad_html_falls_back_to_readable_text() {
        let (tg,server)=mock(vec![json!({"ok":false,"description":"can't parse entities"}),json!({"ok":true,"result":{"message_id":4}})]).await;
        tg.send(1,"<broken>",None).await.unwrap();let r=server.await.unwrap();assert!(r[1].1.get("parse_mode").is_none());assert_eq!(r[1].1["text"],"<broken>");
    }
    #[tokio::test] async fn network_error_does_not_expose_token() {
        let (tg,server)=mock(vec![]).await;server.await.unwrap();let error=tg.get_me().await.unwrap_err().to_string();assert!(!error.contains("test-only"));
    }
    #[tokio::test] async fn restricted_member_with_membership_passes_gate() {
        let (tg,server)=mock(vec![json!({"ok":true,"result":{"status":"restricted","is_member":true}})]).await;
        assert!(tg.is_member("@channel",1).await.unwrap());server.await.unwrap();
    }
}

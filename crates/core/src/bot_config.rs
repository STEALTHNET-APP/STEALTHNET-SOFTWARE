//! Общие настройки клиентских поверхностей. Секреты сюда не входят.
use crate::{Error, Pool, Result};
use serde_json::{Map, Value};

pub async fn load(pool: &Pool) -> Result<Map<String, Value>> {
    Ok(sqlx::query_as::<_, (String,Value)>("SELECT key,value FROM settings WHERE key LIKE 'bot.%'")
        .fetch_all(pool).await?.into_iter().collect())
}
pub fn text<'a>(s: &'a Map<String,Value>, key: &str, fallback: &'a str) -> &'a str {
    s.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).unwrap_or(fallback)
}
pub fn flag(s: &Map<String,Value>, key: &str, fallback: bool) -> bool {
    s.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}
pub const MENU_BUILTINS: [&str; 9] = ["miniapp", "connect", "purchase", "addons", "subscription", "referral", "tickets", "docs", "support"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuButton {
    pub id: String,
    #[serde(default = "menu_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_custom_emoji_id: Option<String>,
}
fn menu_enabled() -> bool { true }

pub fn button_url(s: &str) -> bool {
    if s.len() > 2048 { return false; }
    web_url(s, false) || s.parse::<axum::http::Uri>().ok().is_some_and(|u| {
        u.scheme_str() == Some("tg") && u.host().is_some_and(|h| !h.is_empty())
            && !s.chars().any(|c| c.is_whitespace() || c.is_control())
            && !u.authority().is_some_and(|a| a.as_str().contains('@'))
    })
}
pub fn emoji_id(value: &str) -> Option<String> {
    let value = value.trim();
    let id = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')).unwrap_or(value);
    (!id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
        && id.bytes().any(|b| b != b'0')).then(|| id.to_owned())
}
fn parse_menu(value: &Value) -> Result<Vec<MenuButton>> {
    let mut entries: Vec<MenuButton> = serde_json::from_value(value.clone())
        .map_err(|_| Error::bad("Меню бота: нужен список кнопок с корректными полями"))?;
    if entries.len() > 29 { return Err(Error::bad("Можно добавить до 20 своих кнопок")); }
    let mut seen = std::collections::HashSet::new();
    let mut links = 0;
    for e in &mut entries {
        if let Some(raw) = &e.icon_custom_emoji_id {
            e.icon_custom_emoji_id = Some(emoji_id(raw).ok_or_else(|| Error::bad("Эмодзи кнопки: укажите числовой ID, например [5215361191051798408]"))?);
        }
        if !seen.insert(e.id.clone()) { return Err(Error::bad("Кнопки меню не должны повторяться")); }
        if MENU_BUILTINS.contains(&e.id.as_str()) {
            if e.label.is_some() || e.url.is_some() { return Err(Error::bad("У системной кнопки нельзя заменять действие ссылкой")); }
        } else {
            links += 1;
            if links > 20 || !e.id.starts_with("link_") || e.id.len() > 64 || !e.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err(Error::bad("Некорректная кнопка меню"));
            }
            let label = e.label.as_deref().unwrap_or("");
            if label.trim().is_empty() || label.chars().count() > 64 || label.chars().any(char::is_control) {
                return Err(Error::bad("У своей кнопки нужно название от 1 до 64 символов"));
            }
            if !button_url(e.url.as_deref().unwrap_or("")) {
                return Err(Error::bad("Для кнопки укажите ссылку https://, http:// или tg:// до 2048 символов"));
            }
        }
    }
    Ok(entries)
}
/// Missing/new builtin actions retain their default position at the end.
/// Existing installations with no saved layout keep the original keyboard.
pub fn menu_buttons(value: Option<&Value>) -> Vec<MenuButton> {
    let mut entries = value.and_then(|v| parse_menu(v).ok()).unwrap_or_default();
    for id in MENU_BUILTINS {
        if !entries.iter().any(|e| e.id == id) {
            entries.push(MenuButton { id: id.into(), enabled: true, label: None, url: None, icon_custom_emoji_id: None });
        }
    }
    entries
}

pub fn validate(s: &Map<String,Value>) -> Result<()> {
    for (k,v) in s.iter().filter(|(k,_)| k.starts_with("bot.")) {
        if k == "bot.menu_layout" { parse_menu(v)?; continue; }
        if matches!(k.as_str(),"bot.alert_chat_id"|"bot.alert_thread_id") {
            let text=v.as_str().ok_or_else(||Error::bad("ID группы и темы должны быть текстом"))?;
            if !text.is_empty() && (text.parse::<i64>().ok().is_none_or(|n|n.to_string()!=text || if k=="bot.alert_chat_id" {n>=0}else{n<=0})) {return Err(Error::bad("Для группы укажите отрицательный числовой ID, для темы — положительный"));}
            continue;
        }
        if k == "bot.username" { return Err(Error::bad("Имя бота определяется автоматически")); }
        if k.ends_with("_enabled") || matches!(k.as_str(),"bot.welcome_always"|"bot.connect_inline"|"bot.miniapp_shop"|"bot.miniapp_devices") {
            if !v.is_boolean() { return Err(Error::bad(format!("{k}: требуется переключатель"))); }
        } else if k == "bot.referral_percent" {
            if !v.as_f64().is_some_and(|n| n.is_finite() && (0.0..=100.0).contains(&n)) { return Err(Error::bad("Процент — число от 0 до 100")); }
        } else if k == "bot.notify_days" {
            let days=v.as_array().ok_or_else(|| Error::bad("Дни напоминаний: нужен список чисел"))?;
            if days.is_empty() || days.len()>5 || days.iter().any(|d| !d.as_i64().is_some_and(|n| (1..=30).contains(&n))) {
                return Err(Error::bad("Укажите от 1 до 5 порогов напоминаний, от 1 до 30 дней"));
            }
        } else if k == "bot.docs_links" {
            let links=v.as_array().ok_or_else(|| Error::bad("Документы: нужен список ссылок"))?;
            if links.len()>20 { return Err(Error::bad("Можно добавить до 20 документов")); }
            for link in links {
                let label=link["label"].as_str().unwrap_or("");
                if label.trim().is_empty() || label.chars().count()>64 || !web_url(link["url"].as_str().unwrap_or(""),false) {
                    return Err(Error::bad("У документа нужны подпись до 64 символов и адрес http(s)"));
                }
            }
        } else {
            let value=v.as_str().ok_or_else(|| Error::bad(format!("{k}: требуется текст")))?;
            let max=if k.contains("label") || k.starts_with("bot.button.") || k=="bot.brand_name" {64}
                    else if k=="bot.menu_note" {1000} else {3000};
            if k=="bot.require_channel" && !value.trim().is_empty() {
                let v=value.trim();
                let valid=if let Some(name)=v.strip_prefix('@') {name.len()>=4 && name.chars().all(|c| c.is_ascii_alphanumeric() || c=='_')}
                    else {v.parse::<i64>().is_ok_and(|id| id<0)};
                if !valid {return Err(Error::bad("Канал задаётся как @имя или отрицательный числовой ID"));}
            }
            if value.chars().count()>max { return Err(Error::bad(format!("{k}: максимум {max} символов"))); }
            if !value.trim().is_empty() && (k.ends_with("_url") || k=="bot.logo_url") && !web_url(value,k=="bot.miniapp_url") {
                return Err(Error::bad(format!("{k}: некорректная ссылка{}",if k=="bot.miniapp_url" {" — нужен HTTPS"} else {""})));
            }
        }
    }
    Ok(())
}
pub fn web_url(s: &str, https: bool) -> bool {
    s.parse::<axum::http::Uri>().ok().is_some_and(|u| {
        let scheme=u.scheme_str().unwrap_or("");
        (scheme=="https" || (!https && scheme=="http")) && u.host().is_some_and(|h| !h.is_empty())
            && !s.chars().any(|c| c.is_whitespace() || c.is_control()) && !u.authority().is_some_and(|a| a.as_str().contains('@'))
    })
}
pub fn notification_days(s: &Map<String,Value>) -> Vec<i64> {
    let mut days=s.get("bot.notify_days").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64)
        .filter(|d| (1..=30).contains(d)).collect::<Vec<_>>()).unwrap_or_else(|| vec![3,1]);
    days.sort_unstable(); days.dedup(); days
}
#[cfg(test)] mod tests {
 use super::*; use serde_json::json;
 #[test] fn settings_reject_invalid_types_links_and_limits() {
  for v in [json!({"bot.tickets_enabled":"false"}),json!({"bot.miniapp_url":"http://a.test"}),json!({"bot.support_url":"https://"}),json!({"bot.notify_days":[0,31]}),json!({"bot.referral_percent":101}),json!({"bot.docs_links":[{"label":"test","url":"javascript:alert(1)"}]}),json!({"bot.username":"fake"})] {assert!(validate(v.as_object().unwrap()).is_err(),"{v}");}
 }
 #[test] fn custom_emoji_ids_preserve_precision_and_validate() {
  for raw in ["5215361191051798408", "[5215361191051798408]"] {
   assert_eq!(emoji_id(raw).as_deref(), Some("5215361191051798408"));
   let value=json!([{"id":"docs","icon_custom_emoji_id":raw}]);
   assert_eq!(menu_buttons(Some(&value))[0].icon_custom_emoji_id.as_deref(),Some("5215361191051798408"));
  }
  for raw in ["", "0", "abc", "[123", "123]", "12 34", "123456789012345678901"] {
   assert!(parse_menu(&json!([{"id":"docs","icon_custom_emoji_id":raw}])).is_err());
  }
  assert!(parse_menu(&json!([{"id":"docs","icon_custom_emoji_id":5215361191051798408u64}])).is_err());
 }
 #[test] fn menu_layout_rejects_unsafe_or_ambiguous_buttons() {
  for value in [
   json!([{"id":"docs"},{"id":"docs"}]),
   json!([{"id":"unknown"}]),
   json!([{"id":"docs","url":"https://example.com"}]),
   json!([{"id":"link_a","label":"Канал","url":"javascript:alert(1)"}]),
   json!([{"id":"link_a","label":"Канал","url":"https://"}]),
   json!([{"id":"link_a","label":"Канал","url":"https://user:pass@example.com"}]),
   json!([{"id":"link_a","label":" ","url":"https://example.com"}]),
   json!([{"id":"link_a","label":"Канал","url":"https://example.com","enabled":"false"}]),
   json!([{"id":"link_a","label":"Канал","url":"https://example.com","callback_data":"buy"}]),
  ] { assert!(validate(json!({"bot.menu_layout":value}).as_object().unwrap()).is_err()); }
  let many=(0..21).map(|n|json!({"id":format!("link_{n}"),"label":"Канал","url":"https://example.com"})).collect::<Vec<_>>();
  assert!(validate(json!({"bot.menu_layout":many}).as_object().unwrap()).is_err());
 }
 #[test] fn menu_preserves_order_visibility_and_original_defaults() {
  let value=json!([{"id":"docs","enabled":false},{"id":"link_channel","label":"Наш канал","url":"tg://resolve?domain=example"},{"id":"purchase"}]);
  assert!(validate(json!({"bot.menu_layout":value}).as_object().unwrap()).is_ok());
  let entries=menu_buttons(Some(&value));assert_eq!(entries.len(),10);assert_eq!(entries[0].id,"docs");assert!(!entries[0].enabled);
  assert_eq!(entries[1].url.as_deref(),Some("tg://resolve?domain=example"));assert_eq!(entries[2].id,"purchase");
  assert_eq!(menu_buttons(None).iter().map(|e|e.id.as_str()).collect::<Vec<_>>(),MENU_BUILTINS);
  assert!(button_url("https://example.com/path?q=1#anchor"));assert!(button_url("http://example.com"));
 }
 #[test] fn legitimate_settings_and_defaults() {
  let v=json!({"bot.notify_days":[3,1,3],"bot.support_url":"https://t.me/support","bot.tickets_enabled":false});
  assert!(validate(v.as_object().unwrap()).is_ok()); assert_eq!(notification_days(v.as_object().unwrap()),vec![1,3]);
  assert_eq!(notification_days(&Map::new()),vec![1,3]);
 }
}

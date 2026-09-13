use axum::http::{header, HeaderMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language { Ru, En }

#[derive(Default, serde::Deserialize)]
pub struct LanguageQuery { pub lang: Option<String> }

impl Language {
    pub fn code(self) -> &'static str { self.text("ru", "en") }
    pub fn text<'a>(self, ru: &'a str, en: &'a str) -> &'a str {
        match self { Self::Ru => ru, Self::En => en }
    }
    /// Translate shipped defaults, including those seeded in older databases.
    /// Custom owner messages are preserved; substitutions happen afterwards.
    pub fn default_text(self, value: &str) -> &str {
        if self == Self::Ru { return value; }
        match value {
            "Подключение" => "Connection",
            "Написать в поддержку" => "Contact support",
            "⛔ Подписка истекла {date}" => "⛔ Subscription expired on {date}",
            "📉 Трафик закончился" => "📉 Data allowance used up",
            "🚫 Доступ приостановлен" => "🚫 Access suspended",
            "Нет доступных локаций" => "No locations available",
            "Занято устройств: {used} из {limit}. Отключите лишнее и обновите подписку" => "Devices used: {used} of {limit}. Remove an unused device and refresh your subscription",
            "Приложение не передало HWID. Используйте приложение с поддержкой идентификатора устройства." => "The app did not provide a device ID. Use an app that supports device identification.",
            _ => value,
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() { "ru" => Some(Self::Ru), "en" => Some(Self::En), _ => None }
    }
    /// Explicit link, then a manual choice on this domain, then browser preferences.
    pub fn for_request(query: &LanguageQuery, headers: &HeaderMap) -> Self {
        if let Some(lang) = query.lang.as_deref().and_then(Self::parse) { return lang; }
        for cookie in headers.get_all(header::COOKIE) {
            if let Ok(cookie) = cookie.to_str() {
                for item in cookie.split(';') {
                    if let Some(("sn.sub.lang", value)) = item.trim().split_once('=') {
                        if let Some(lang) = Self::parse(value) { return lang; }
                    }
                }
            }
        }
        let preferences = headers.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let mut best = None;
        for item in preferences.split(',') {
            let mut parts = item.trim().split(';');
            let tag = parts.next().unwrap_or("").split('-').next().unwrap_or("");
            let quality = parts.find_map(|p| p.trim().strip_prefix("q=")).map(|q| q.parse::<f32>().unwrap_or(0.0)).unwrap_or(1.0);
            if quality > 0.0 && quality <= 1.0 && best.is_none_or(|(_, q)| quality > q) {
                if let Some(lang) = Self::parse(tag) { best = Some((lang, quality)); }
            }
        }
        best.map(|(lang, _)| lang).unwrap_or(if preferences.is_empty() { Self::Ru } else { Self::En })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_cookie_and_browser_precedence() {
        let mut h = HeaderMap::new();
        h.insert(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9,ru;q=0.7".parse().unwrap());
        assert_eq!(Language::for_request(&LanguageQuery::default(), &h), Language::En);
        h.insert(header::COOKIE, "other=1; sn.sub.lang=ru".parse().unwrap());
        assert_eq!(Language::for_request(&LanguageQuery::default(), &h), Language::Ru);
        assert_eq!(Language::for_request(&LanguageQuery { lang: Some("en".into()) }, &h), Language::En);
        assert_eq!(Language::for_request(&LanguageQuery { lang: Some("invalid".into()) }, &h), Language::Ru);
        h.remove(header::COOKIE);
        for (value, expected) in [("ru-RU,en;q=0.8", Language::Ru), ("en;q=0.4,ru;q=0.9", Language::Ru), ("ru;q=0,en", Language::En), ("de-DE", Language::En), ("en;q=NaN,ru", Language::Ru)] {
            h.insert(header::ACCEPT_LANGUAGE, value.parse().unwrap());
            assert_eq!(Language::for_request(&LanguageQuery::default(), &h), expected);
        }
        assert_eq!(Language::for_request(&LanguageQuery::default(), &HeaderMap::new()), Language::Ru);
    }
}

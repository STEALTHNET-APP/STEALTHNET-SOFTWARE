//! Declarative, panel-owned website settings. Never passed to Xray itself.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const KEY: &str = "_selfsteal";
pub const TARGET: &str = "127.0.0.1:9443";
pub const TEMPLATES: [&str; 9] = ["studio", "journal", "travel", "recipes", "tools", "library", "gallery", "garden", "clock"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub domain: String,
    pub inbound_tag: String,
    pub template: String,
    pub language: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
}

pub fn valid_domain(s: &str) -> bool {
    s.len() <= 253 && s.contains('.') && s == s.to_ascii_lowercase()
        && s.parse::<std::net::IpAddr>().is_err()
        && s.split('.').all(|p| !p.is_empty() && p.len() <= 63
            && !p.starts_with('-') && !p.ends_with('-')
            && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
        && s.rsplit('.').next().is_some_and(|t| t.len() >= 2 && t.bytes().any(|b| b.is_ascii_lowercase()))
        && ![".local", ".localhost", ".internal", ".test", ".invalid", ".example", ".home.arpa"].iter().any(|suffix| s.ends_with(suffix))
}

impl Site {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_domain(&self.domain) {
            return Err("Selfsteal: enter a public domain such as node.example.com, without https, a port or a path / Selfsteal: укажите публичный домен, например node.example.com, без https, порта и пути".into());
        }
        if !TEMPLATES.contains(&self.template.as_str()) || !["ru", "en"].contains(&self.language.as_str()) {
            return Err("Selfsteal: select a website and its language / Selfsteal: выберите оформление и язык сайта".into());
        }
        if self.inbound_tag.is_empty() || self.inbound_tag.len() > 160 || self.title.chars().count() > 80 || self.description.chars().count() > 600
            || [&self.title, &self.description].iter().any(|s| s.chars().any(|c| c.is_control() && c != '\n')) {
            return Err("Selfsteal: title up to 80 characters, description up to 600 / Selfsteal: название до 80 символов, описание до 600".into());
        }
        Ok(())
    }
}

pub fn settings(config: &Value) -> Result<Option<Site>, String> {
    let Some(value) = config.get(KEY) else { return Ok(None); };
    let site: Site = serde_json::from_value(value.clone()).map_err(|_| "Invalid Selfsteal settings / Некорректные настройки Selfsteal".to_string())?;
    site.validate()?;
    Ok(Some(site))
}

/// Validate both sides of the contract, including hand-edited/imported JSON.
pub fn validate(config: &Value) -> Result<Option<Site>, String> {
    let Some(site) = settings(config)? else { return Ok(None); };
    let list = config["inbounds"].as_array().ok_or("Selfsteal: missing inbounds / Selfsteal: нет подключений")?;
    let matching: Vec<_> = list.iter().filter(|i| i["tag"] == site.inbound_tag).collect();
    if matching.len() != 1 { return Err("Selfsteal: select one REALITY inbound / Selfsteal: выберите одно подключение REALITY".into()); }
    let ib = matching[0];
    let r = &ib["streamSettings"]["realitySettings"];
    if ib["protocol"] != "vless" || ib["streamSettings"]["security"] != "reality" || ib["port"] != 443
        || !["raw", "tcp"].contains(&ib["streamSettings"]["network"].as_str().unwrap_or("raw"))
        || r["target"] != TARGET || r.get("dest").is_some() || r["serverNames"] != json!([site.domain])
        || r.get("xver").is_some_and(|v| v != 0)
        || !["0.0.0.0", "::", "[::]"].contains(&ib["listen"].as_str().unwrap_or("0.0.0.0")) {
        return Err("Selfsteal: use VLESS REALITY on port 443 with the website domain and internal target; reopen Parameters / Selfsteal: нужен VLESS REALITY на порту 443 с доменом сайта и внутренним адресом; откройте «Параметры»".into());
    }
    if list.iter().any(|i| matches!(i["port"].as_u64(), Some(80 | 9443))) {
        return Err("Selfsteal reserves port 80 for certificates and loopback port 9443 for the website / Selfsteal использует порт 80 для сертификатов и внутренний порт 9443 для сайта".into());
    }
    Ok(Some(site))
}

pub fn engine_config(config: &Value) -> Value {
    let mut config = config.clone();
    if let Some(o) = config.as_object_mut() { o.remove(KEY); }
    config
}

/// Changing a rehearsal domain must update the REALITY settings as one unit.
pub fn set_domain(config: &mut Value, domain: &str) -> Result<(), String> {
    if !valid_domain(domain) { return Err("Selfsteal: invalid domain / Selfsteal: неверный домен".into()); }
    let mut site = settings(config)?.ok_or("Selfsteal is not enabled / Selfsteal не включён")?;
    site.domain = domain.into();
    for ib in config["inbounds"].as_array_mut().into_iter().flatten() {
        if ib["tag"] == site.inbound_tag {
            ib["streamSettings"]["realitySettings"]["serverNames"] = json!([domain]);
        }
    }
    config[KEY] = json!(site);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn sample() -> Value { json!({KEY:{"domain":"node.example.com","inbound_tag":"vpn","template":"studio","language":"en"},"inbounds":[{"tag":"vpn","port":443,"protocol":"vless","streamSettings":{"network":"raw","security":"reality","realitySettings":{"target":TARGET,"serverNames":["node.example.com"],"xver":0}}}]}) }
    #[test] fn domains_reject_injection_and_local_names() {
        for s in ["localhost", "a.local", "a.test", "1.2.3.4", "a.com:80", "a.com/path", "https://a.com", "a.com\n}", "a.com.", "*.a.com", "a..com", "-a.com", "A.com", "a.123"] { assert!(!valid_domain(s), "{s}"); }
        for s in ["node.example.com", "xn--e1afmkfd.xn--p1ai", "edge-2.example.org"] { assert!(valid_domain(s)); }
    }
    #[test] fn contract_and_engine_separation() {
        let cfg = sample(); assert!(validate(&cfg).unwrap().is_some());
        let clean = engine_config(&cfg); assert!(clean.get(KEY).is_none()); assert_eq!(clean["inbounds"], cfg["inbounds"]);
        for (path, value) in [("/inbounds/0/port", json!(8443)), ("/inbounds/0/streamSettings/realitySettings/target", json!("other:443")), ("/inbounds/0/streamSettings/realitySettings/serverNames", json!(["other.com"])), ("/inbounds/0/streamSettings/realitySettings/xver", json!(1)), ("/_selfsteal/template", json!("../../file"))] { let mut c = cfg.clone(); *c.pointer_mut(path).unwrap() = value; assert!(validate(&c).is_err(), "{path}"); }
    }
}

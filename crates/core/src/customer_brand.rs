//! Public visual identity shared by customer surfaces. Never exports cabinet settings or keys.
use serde::{Deserialize,Serialize};
use serde_json::Value;
#[derive(Clone,Debug,Default,Deserialize,Serialize)]
pub struct CustomerBrand {
    pub brand:Option<String>,pub logo:Option<String>,pub logo_dark:Option<String>,pub favicon:Option<String>,
    pub accent:Option<String>,pub accent_end:Option<String>,
}
impl CustomerBrand {
    pub fn from_config(value:&Value)->Self {
        let text=|key:&str|value[key].as_str().map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned);
        let url=|key:&str|text(key).filter(|s|s.parse::<axum::http::Uri>().ok().is_some_and(|u|u.scheme_str()==Some("https")&&u.host().is_some()&&!u.authority().is_some_and(|a|a.as_str().contains('@'))));
        let color=|key:&str|text(key).filter(|s|s.len()==7&&s.starts_with('#')&&s[1..].bytes().all(|b|b.is_ascii_hexdigit()));
        Self{brand:text("brand"),logo:url("logo"),logo_dark:url("logo_dark"),favicon:url("favicon"),accent:color("accent"),accent_end:color("accent_end")}
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn public_identity_excludes_private_configuration_and_rejects_injection(){
        let brand=CustomerBrand::from_config(&serde_json::json!({"brand":"Owner","logo":"https://example.test/logo.svg","accent":"#523BFF","accent_end":"red;}</style><script>alert(1)</script>","logo_dark":"javascript:alert(1)","favicon":"https://user:pass@example.test/icon","miniapp_installation_id":"private","secret":"private"}));
        assert_eq!(brand.accent.as_deref(),Some("#523BFF"));assert!(brand.accent_end.is_none()&&brand.logo_dark.is_none()&&brand.favicon.is_none());
        let out=serde_json::to_string(&brand).unwrap();assert!(!out.contains("private")&&!out.contains("script>"));assert!(out.contains("https://example.test/logo.svg"));
    }
}

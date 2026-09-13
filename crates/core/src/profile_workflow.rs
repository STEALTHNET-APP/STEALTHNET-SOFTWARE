//! Profile parameters and reusable templates. No executable template language.
use serde_json::{json, Value};

pub const MAX_CONFIG_BYTES: usize = 512 * 1024;
pub fn validate_shape(config: &Value) -> std::result::Result<(), String> {
    if !config.is_object() || config.to_string().len() > MAX_CONFIG_BYTES {
        return Err("JSON object required, maximum 512 KiB / Нужен JSON-объект до 512 КиБ".into());
    }
    for key in ["log", "api", "stats", "policy", "routing", "dns"] {
        if config.get(key).is_some_and(|v| !v.is_object()) {
            return Err(format!("Expected object / Ожидается объект: /{key}"));
        }
    }
    for key in ["inbounds", "outbounds"] {
        if let Some(v) = config.get(key) {
            let list = v
                .as_array()
                .ok_or_else(|| format!("Expected array / Ожидается массив: /{key}"))?;
            for (i, ib) in list.iter().enumerate() {
                if !ib.is_object() {
                    return Err(format!("Expected object / Ожидается объект: /{key}/{i}"));
                }
                for field in ["settings", "streamSettings", "sniffing"] {
                    if ib.get(field).is_some_and(|v| !v.is_object()) {
                        return Err(format!(
                            "Expected object / Ожидается объект: /{key}/{i}/{field}"
                        ));
                    }
                }
                if let Some(stream) = ib.get("streamSettings") {
                    for field in [
                        "realitySettings",
                        "tlsSettings",
                        "wsSettings",
                        "grpcSettings",
                        "xhttpSettings",
                        "httpupgradeSettings",
                        "sockopt",
                    ] {
                        if stream.get(field).is_some_and(|v| !v.is_object()) {
                            return Err(format!("Expected object / Ожидается объект: /{key}/{i}/streamSettings/{field}"));
                        }
                    }
                }
            }
        }
    }
    for pointer in ["/policy/levels", "/policy/system"] {
        if config.pointer(pointer).is_some_and(|v| !v.is_object()) {
            return Err(format!("Expected object / Ожидается объект: {pointer}"));
        }
    }
    if let Some(levels) = config.pointer("/policy/levels").and_then(Value::as_object) {
        for (key, level) in levels {
            if !level.is_object() {
                return Err(format!(
                    "Expected object / Ожидается объект: /policy/levels/{key}"
                ));
            }
        }
    }
    for pointer in ["/api/services", "/routing/rules", "/routing/balancers"] {
        if config.pointer(pointer).is_some_and(|v| !v.is_array()) {
            return Err(format!("Expected array / Ожидается массив: {pointer}"));
        }
    }
    Ok(())
}
pub fn is_placeholder(s: &str) -> bool {
    let upper = s.to_uppercase();
    [
        "#REPLACE",
        "YOUR_",
        "ВАШ_",
        "СЕРВЕРНЫЙ_КЛЮЧ",
        "/ПУТЬ/К/",
        "ВТОРАЯ_НОДА",
        "UUID_СЕРВИСНОГО",
        "ПУБЛИЧНЫЙ_КЛЮЧ",
        "{{",
        "<REQUIRED",
        "CHANGE_ME",
    ]
    .iter()
    .any(|p| upper.contains(p))
}
pub fn missing_parameters(config: &Value) -> Vec<String> {
    fn walk(v: &Value, path: String, out: &mut Vec<String>) {
        match v {
            Value::String(s) if is_placeholder(s) => out.push(path),
            Value::Object(obj) => {
                for (k, v) in obj {
                    walk(
                        v,
                        format!("{}/{}", path, k.replace('~', "~0").replace('/', "~1")),
                        out,
                    )
                }
            }
            Value::Array(list) => {
                for (i, v) in list.iter().enumerate() {
                    walk(v, format!("{path}/{i}"), out)
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    walk(config, String::new(), &mut out);
    out
}
/// Template export deliberately parameterizes operator values, including unknown
/// string fields. Working configuration export is a separate authenticated action.
pub fn sanitize_template(config: &Value) -> Value {
    fn clean(v: &Value, key: &str, inbound: bool) -> Value {
        let lower = key.to_ascii_lowercase();
        if inbound
            && ["clients", "users", "accounts", "peers"].contains(&lower.as_str())
            && v.is_array()
        {
            return json!([]);
        }
        if [
            "privatekey",
            "password",
            "psk",
            "secretkey",
            "token",
            "authorization",
            "cookie",
            "email",
            "username",
            "id",
            "uuid",
            "auth",
        ]
        .contains(&lower.as_str())
            && !v.is_null()
        {
            return json!(format!("{{{{{key}}}}}"));
        }
        match v {
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), clean(v, k, inbound || k == "inbounds")))
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(|v| clean(v, key, inbound)).collect()),
            Value::String(s) => {
                // Structural names are necessary for routing and preserve references.
                let structural = [
                    "tag",
                    "inboundTag",
                    "outboundTag",
                    "balancerTag",
                    "selector",
                ];
                let enum_values = [
                    "protocol",
                    "network",
                    "security",
                    "method",
                    "loglevel",
                    "decryption",
                    "encryption",
                    "flow",
                    "type",
                    "domainStrategy",
                    "queryStrategy",
                    "mode",
                    "destOverride",
                    "services",
                    "listen",
                ];
                if structural.contains(&key)
                    || (enum_values.contains(&key)
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "-_:,. ".contains(c)))
                {
                    v.clone()
                } else {
                    json!(format!("{{{{{key}}}}}"))
                }
            }
            _ => v.clone(),
        }
    }
    clean(config, "", false)
}
/// Generate only missing secrets, per inbound. Existing values are never rotated.
pub fn generate_missing(config: &Value) -> Value {
    use base64::Engine;
    use rand::RngCore;
    let mut config = config.clone();
    if let Some(list) = config["inbounds"].as_array_mut() {
        for ib in list {
            if let Some(r) = ib
                .pointer_mut("/streamSettings/realitySettings")
                .and_then(Value::as_object_mut)
            {
                let keys = crate::keygen::generate_reality_keys();
                if r.get("privateKey")
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.is_empty() || is_placeholder(s))
                {
                    r.insert("privateKey".into(), json!(keys.private_key));
                }
                if r.get("shortIds").is_none_or(|v| {
                    v.as_array().is_some_and(|a| {
                        a.is_empty() || a.iter().any(|s| s.as_str().is_some_and(is_placeholder))
                    })
                }) {
                    r.insert("shortIds".into(), json!([keys.short_id]));
                }
            }
            if ib["protocol"] == "shadowsocks" {
                let method = ib["settings"]["method"].as_str().unwrap_or("");
                let size = if method == "2022-blake3-aes-128-gcm" {
                    16
                } else {
                    32
                };
                if ib["settings"]["password"]
                    .as_str()
                    .is_none_or(|s| s.is_empty() || is_placeholder(s))
                {
                    let mut raw = vec![0; size];
                    rand::thread_rng().fill_bytes(&mut raw);
                    ib["settings"]["password"] =
                        json!(base64::engine::general_purpose::STANDARD.encode(raw));
                }
            }
        }
    }
    config
}
pub fn changed_paths(old: &Value, new: &Value) -> Vec<String> {
    fn diff(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        if a == b {
            return;
        }
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
                for key in keys {
                    diff(
                        a.get(key).unwrap_or(&Value::Null),
                        b.get(key).unwrap_or(&Value::Null),
                        format!("{path}/{key}"),
                        out,
                    )
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                for i in 0..a.len().max(b.len()) {
                    diff(
                        a.get(i).unwrap_or(&Value::Null),
                        b.get(i).unwrap_or(&Value::Null),
                        format!("{path}/{i}"),
                        out,
                    )
                }
            }
            _ => out.push(path),
        }
    }
    let mut out = vec![];
    diff(old, new, String::new(), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_reality_parameters_are_detected() {
        let c = json!({"inbounds":[{"streamSettings":{"realitySettings":{"target":"#REPLACE_WITH_YOUR_DEST, EXAMPLE: example.com:443","serverNames":["#REPLACE_WITH_YOUR_SERVER_NAMES"],"privateKey":"#REPLACE_WITH_YOUR_PRIVATE_KEY"}}}]});
        assert_eq!(missing_parameters(&c).len(), 3);
        let filled = generate_missing(&c);
        assert_eq!(missing_parameters(&filled).len(), 2);
        assert!(crate::keygen::reality_public_from_private(
            filled["inbounds"][0]["streamSettings"]["realitySettings"]["privateKey"]
                .as_str()
                .unwrap()
        )
        .is_some());
    }
    #[test]
    fn custom_values_survive_generation() {
        let c = json!({"extra":{"unknown":[1,2,"keep"]},"inbounds":[{"streamSettings":{"realitySettings":{"privateKey":"manual","shortIds":[""]}}}]});
        assert_eq!(generate_missing(&c), c);
    }
    #[test]
    fn export_removes_credentials_even_in_unknown_fields() {
        let c = json!({"unknown":"SECRET","headers":{"Cookie":"TOKEN"},"inbounds":[{"tag":"vless","settings":{"clients":[{"id":"UUID"}],"password":"PASSWORD"},"streamSettings":{"realitySettings":{"privateKey":"PRIVATE","serverNames":["private.example"]}}}]});
        let s = sanitize_template(&c).to_string();
        for secret in [
            "SECRET",
            "TOKEN",
            "UUID",
            "PASSWORD",
            "PRIVATE",
            "private.example",
        ] {
            assert!(!s.contains(secret));
        }
        assert!(s.contains("vless"));
        assert!(s.contains("{{privateKey}}"));
    }
    #[test]
    fn ss_keys_match_method_and_differ() {
        use base64::Engine;
        let c = json!({"inbounds":[{"protocol":"shadowsocks","settings":{"method":"2022-blake3-aes-128-gcm"}},{"protocol":"shadowsocks","settings":{"method":"2022-blake3-aes-256-gcm"}}]});
        let v = generate_missing(&c);
        for (i, size) in [16, 32].iter().enumerate() {
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(v["inbounds"][i]["settings"]["password"].as_str().unwrap())
                    .unwrap()
                    .len(),
                *size
            );
        }
    }
}

#[cfg(test)]
mod shape_tests {
    use super::*;
    #[test]
    fn malformed_objects_are_rejected() {
        for c in [
            json!({"log":[]}),
            json!({"inbounds":[true]}),
            json!({"inbounds":[{"streamSettings":[]}]}),
            json!({"routing":null}),
        ] {
            assert!(validate_shape(&c).is_err());
        }
    }
    #[test]
    fn outbound_users_are_parameterized() {
        let c = json!({"outbounds":[{"settings":{"vnext":[{"users":[{"id":"secret-uuid","encryption":"none"}]}]}}]});
        let v = sanitize_template(&c);
        assert_eq!(
            v["outbounds"][0]["settings"]["vnext"][0]["users"][0]["id"],
            "{{id}}"
        );
    }
}

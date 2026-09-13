//! Shared contract for host overrides and per-customer subscription settings.
use serde_json::{json, Value};
use sqlx::Row;
use sn_core::{Error, Result};

pub const FORMATS: &[&str] = &["xray_json", "base64", "plain", "mihomo", "stash", "singbox", "clash"];

pub fn validate_host(v: &Value, group: bool) -> std::result::Result<(), String> {
    let map = v.as_object().ok_or("Параметры хоста должны быть объектом JSON")?;
    if v.to_string().len() > 65536 { return Err("Параметры хоста превышают 64 КБ".into()); }
    for (key, value) in map {
        match key.as_str() {
            "tag" | "sni" | "host_header" | "path" | "fingerprint" | "alpn" | "server_description" => {
                let text = value.as_str().ok_or_else(||format!("{key}: требуется текст"))?;
                let max = if key == "server_description" {30} else {2048};
                if text.chars().count() > max || text.chars().any(char::is_control) { return Err(format!("{key}: слишком длинное значение или управляющие символы")); }
            }
            "security" => if !value.as_str().is_some_and(|s|matches!(s,"inherit"|"none"|"tls"|"reality")){return Err("Неизвестный режим защиты".into())},
            "sni_mode" => if !value.as_str().is_some_and(|s|matches!(s,"inherit"|"custom"|"address"|"empty")){return Err("Неизвестный режим SNI".into())},
            "allow_insecure" | "hide" | "shuffle" => if !value.is_boolean(){return Err(format!("{key}: требуется переключатель"))},
            "vless_route_id" => if !value.as_u64().is_some_and(|n|n<=65535){return Err("VLESS Route ID: целое число от 0 до 65535".into())},
            "mux" | "sockopt" | "xhttp" | "finalmask" => {
                if !value.is_object(){return Err(format!("{key}: требуется объект JSON"))}
                if key=="mux" {
                    for field in ["enabled"] {if value.get(field).is_some_and(|v|!v.is_boolean()){return Err(format!("MUX {field}: требуется true/false"))}}
                    for field in ["concurrency","xudpConcurrency"] {if value.get(field).is_some_and(|v|!v.as_i64().is_some_and(|n|(-1..=1024).contains(&n))){return Err(format!("MUX {field}: целое число от -1 до 1024"))}}
                    if value.get("xudpProxyUDP443").is_some_and(|v|!v.as_str().is_some_and(|s|matches!(s,"reject"|"allow"|"skip"))){return Err("MUX xudpProxyUDP443: reject, allow или skip".into())}
                }
            }
            "excluded_formats" => if !value.as_array().is_some_and(|a|a.len()<=FORMATS.len()&&a.iter().all(|v|v.as_str().is_some_and(|s|FORMATS.contains(&s)))){return Err("Неизвестный формат исключения".into())},
            "node_ids" | "exclude_squad_ids" if !group => if !value.as_array().is_some_and(|a|a.len()<=1000&&a.iter().all(|v|v.as_i64().is_some_and(|n|n>0))){return Err(format!("{key}: требуется список идентификаторов"))},
            "xray_template_id" if !group => if !value.as_i64().is_some_and(|n|n>0){return Err("Выберите шаблон Xray JSON".into())},
            _ => return Err(format!("Неизвестный параметр хоста: {key}")),
        }
    }
    Ok(())
}

pub async fn validate_templates(pool: &sn_core::Pool, value: &Value) -> Result<()> {
    let map=value.as_object().ok_or_else(||Error::bad("Шаблоны должны быть объектом"))?;
    for (code,id) in map {
        if !FORMATS.contains(&code.as_str()) {return Err(Error::bad("Неизвестный формат шаблона"))}
        let id=id.as_i64().filter(|n|*n>0).ok_or_else(||Error::bad("Неверный ID шаблона"))?;
        let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM subscription_templates WHERE id=$1 AND code=$2)").bind(id).bind(code).fetch_one(pool).await?;
        if !found {return Err(Error::bad(format!("Шаблон {id} не найден для формата {code}")))}
    }
    Ok(())
}

pub fn validate_settings(value:&Value)->std::result::Result<(),String>{
    let map=value.as_object().ok_or("Настройки подписки должны быть объектом")?;
    for (key,value) in map {
        match key.as_str(){
            "subscription.profile_title"|"subscription.support_url"|"subscription.announce"|"subscription.happ_routing"|"subscription.title"|"subscription.support_text"|"subscription.bot_url"|"subscription.footer_note"=>{
                if !value.as_str().is_some_and(|s|s.len()<=8192){return Err(format!("{key}: требуется текст до 8 КБ"))}
            }
            "subscription.update_interval_hours"|"subscription.response_headers"=>{},
            "subscription.require_hwid"|"subscription.json_on_unknown_ua"|"subscription.username_header"|"subscription.shuffle_hosts"=>if !value.is_boolean(){return Err(format!("{key}: требуется переключатель"))},
            x if ["expired","limited","disabled","devices","unsupported","no_hosts"].iter().any(|s|x==format!("subscription.remark_{s}"))=>{},
            _=>return Err(format!("Этот параметр нельзя переопределить в скваде: {key}")),
        }
    }
    crate::policy::validate_settings(map)
}

pub async fn load_external(pool:&sn_core::Pool, client_id:i64)->Result<Value>{
    let row=sqlx::query("SELECT e.subscription_settings,e.template_overrides,e.host_overrides FROM clients c JOIN external_squads e ON e.id=c.external_squad_id AND e.is_active WHERE c.id=$1").bind(client_id).fetch_optional(pool).await?;
    let Some(row)=row else{return Ok(json!({}))};
    let ids:Value=row.get("template_overrides");
    let mut templates=serde_json::Map::new();
    for (code,id) in ids.as_object().into_iter().flatten(){
        if let Some(id)=id.as_i64(){
            let body:Option<String>=sqlx::query_scalar("SELECT body FROM subscription_templates WHERE id=$1 AND code=$2").bind(id).bind(code).fetch_optional(pool).await?;
            if let Some(body)=body{templates.insert(code.clone(),json!(body));}
        }
    }
    Ok(json!({"settings":row.get::<Value,_>("subscription_settings"),"templates":templates,"hosts":row.get::<Value,_>("host_overrides")}))
}

pub fn apply_host(host:&mut crate::formats::HostEntry, group:&Value){
    let mut options=host.options.as_object().cloned().unwrap_or_default();
    if let Some(group)=group.as_object(){options.extend(group.clone());}
    if let Some(security)=options.get("security").and_then(Value::as_str).filter(|s|*s!="inherit"){host.security=security.into();}
    for (key,field) in [("sni",&mut host.sni),("host_header",&mut host.host_header),("path",&mut host.path),("fingerprint",&mut host.fingerprint),("alpn",&mut host.alpn)]{
        if let Some(value)=options.get(key).and_then(Value::as_str){*field=Some(value.into());}
    }
    match options.get("sni_mode").and_then(Value::as_str){Some("address")=>host.sni=Some(host.address.clone()),Some("empty")=>host.sni=Some(String::new()),_=>{}}
    if host.protocol=="vless" {
        if let Some(route)=options.get("vless_route_id").and_then(Value::as_u64).filter(|r|*r>0){
            if let Ok(id)=uuid::Uuid::parse_str(&host.uuid){let mut bytes=*id.as_bytes();bytes[6]=(route>>8) as u8;bytes[7]=route as u8;host.uuid=uuid::Uuid::from_bytes(bytes).to_string();}
        }
    }
    host.options=Value::Object(options);
}

pub fn visible(host:&crate::formats::HostEntry, format:&str)->bool{
    host.options["hide"]!=true&&!host.options["excluded_formats"].as_array().is_some_and(|a|a.iter().any(|x|x==format))
}

#[cfg(test)] mod tests{
    use super::*;
    #[test] fn sparse_override_preserves_empty_and_false(){let mut h=crate::formats::stub_host("test");h.sni=Some("global.test".into());h.options=json!({"allow_insecure":true,"path":"/old"});apply_host(&mut h,&json!({"sni":"","allow_insecure":false}));assert_eq!(h.sni.as_deref(),Some(""));assert_eq!(h.path.as_deref(),Some("/old"));assert_eq!(h.options["allow_insecure"],false);}
    #[test] fn rejects_bad_types_and_unknown_fields(){for v in [json!({"vless_route_id":65536}),json!({"mux":[]}),json!({"unknown":1}),json!({"hide":"true"})]{assert!(validate_host(&v,false).is_err());}assert!(validate_host(&json!({"vless_route_id":0,"mux":{"enabled":true,"concurrency":-1}}),false).is_ok());}
    #[test] fn filters_are_format_specific(){let mut h=crate::formats::stub_host("test");h.options=json!({"excluded_formats":["mihomo"]});assert!(!visible(&h,"mihomo"));assert!(visible(&h,"xray_json"));}
}

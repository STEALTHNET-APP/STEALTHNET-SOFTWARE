//! Shared validation and rendering for the panel and both subscription modes.
use axum::http::{HeaderMap,HeaderName,HeaderValue};
use base64::{Engine as _,engine::general_purpose::STANDARD};
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};
use crate::formats::{self,HostEntry};

pub const FORMATS:&[&str]=&["xray_json","mihomo","clash","stash","singbox","base64","plain"];
pub const ACTIONS:&[&str]=&["xray_json","mihomo","clash","stash","singbox","base64","plain","web_page","template","block","not_found","unavailable"];
#[derive(Clone,Debug,Default,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseHeader {pub key:String,pub value:String}
#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Condition {pub header_name:String,pub operator:String,pub value:String,#[serde(default)]pub case_sensitive:bool}

pub fn validate_headers(list:&[ResponseHeader])->Result<(),String>{
    if list.len()>32{return Err("Можно задать не более 32 заголовков".into())}
    let mut seen=std::collections::HashSet::new();
    for h in list{
        let name=HeaderName::from_bytes(h.key.as_bytes()).map_err(|_|format!("Некорректное имя заголовка: {}",h.key))?;
        if matches!(name.as_str(),"content-type"|"content-length"|"content-encoding"|"transfer-encoding"|"connection"|"set-cookie"|"location"|"cache-control"|"pragma"|"subscription-userinfo"|"authorization"|"www-authenticate")||name.as_str().starts_with("access-control-") {return Err(format!("Заголовок {} управляется сервисом",h.key))}
        if !seen.insert(name){return Err(format!("Заголовок {} указан дважды",h.key))}
        if h.value.len()>8192||h.value.bytes().any(|b|b<32||b==127){return Err(format!("В значении {} недопустим перенос строки, управляющий символ или размер более 8 КБ",h.key))}
    }Ok(())
}
pub fn apply_headers(out:&mut HeaderMap,headers:&[ResponseHeader],username:&str,title:&str){
    for h in headers{
        let value=h.value.replace("{username}",username).replace("{title}",title);
        let value=if let Some(raw)=value.strip_prefix("rwEncodeBase64:"){format!("base64:{}",STANDARD.encode(raw))}else if value.is_ascii(){value}else{format!("base64:{}",STANDARD.encode(value))};
        if let (Ok(name),Ok(value))=(HeaderName::from_bytes(h.key.as_bytes()),HeaderValue::from_str(&value)){out.insert(name,value);}
    }
}
pub fn validate_conditions(op:&str,conditions:&[Condition])->Result<(),String>{
    if !matches!(op,"AND"|"OR"){return Err("Операция условий должна быть AND или OR".into())}
    if conditions.len()>24{return Err("В правиле не более 24 условий".into())}
    for c in conditions{
        HeaderName::from_bytes(c.header_name.as_bytes()).map_err(|_|"Некорректное имя заголовка условия")?;
        if c.value.is_empty()||c.value.len()>2048{return Err("Значение условия должно содержать 1–2048 байт".into())}
        if !["EQUALS","NOT_EQUALS","CONTAINS","NOT_CONTAINS","STARTS_WITH","NOT_STARTS_WITH","ENDS_WITH","NOT_ENDS_WITH","REGEX","NOT_REGEX"].contains(&c.operator.as_str()){return Err("Неизвестная операция сравнения".into())}
        if c.operator.ends_with("REGEX"){regex::RegexBuilder::new(&c.value).case_insensitive(!c.case_sensitive).size_limit(1024*1024).build().map_err(|e|format!("Ошибка регулярного выражения: {e}"))?;}
    }Ok(())
}
pub fn conditions_match(op:&str,conditions:&[Condition],headers:&HeaderMap)->bool{
    if conditions.is_empty(){return true}
    let test=|c:&Condition|{
        let values:Vec<_>=headers.get_all(c.header_name.as_str()).iter().filter_map(|v|v.to_str().ok()).collect();
        if values.is_empty(){return false}
        let original=values.join(",");
        let (actual,wanted)=if c.case_sensitive{(original.clone(),c.value.clone())}else{(original.to_lowercase(),c.value.to_lowercase())};
        let positive=match c.operator.trim_start_matches("NOT_"){
            "EQUALS"=>actual==wanted,"CONTAINS"=>actual.contains(&wanted),"STARTS_WITH"=>actual.starts_with(&wanted),"ENDS_WITH"=>actual.ends_with(&wanted),
            "REGEX"=>match regex::RegexBuilder::new(&c.value).case_insensitive(!c.case_sensitive).size_limit(1024*1024).build(){Ok(r)=>r.is_match(&original),Err(_)=>return false},_=>return false};
        if c.operator.starts_with("NOT_"){!positive}else{positive}
    };
    if op=="OR"{conditions.iter().any(test)}else{conditions.iter().all(test)}
}
pub fn validate_routing(link:&str)->Result<(),String>{
    if link.is_empty()||link=="happ://routing/off"{return Ok(())}
    if link.len()>8192{return Err("Ссылка роутинга превышает размер HTTP-заголовка: 8 КБ".into())}
    let encoded=link.strip_prefix("happ://routing/add/").or_else(||link.strip_prefix("happ://routing/onadd/")).ok_or("Ожидается happ://routing/add/… или happ://routing/onadd/…")?;
    let raw=STANDARD.decode(encoded).map_err(|_|"Повреждён Base64 в ссылке Happ")?;
    let v:Value=serde_json::from_slice(&raw).map_err(|_|"В ссылке Happ должен быть JSON-профиль")?;
    if !v.is_object()||v["Name"].as_str().is_none_or(|s|s.trim().is_empty()){return Err("В профиле Happ требуется Name".into())}
    for key in ["DirectSites","DirectIp","ProxySites","ProxyIp","BlockSites","BlockIp"]{if let Some(a)=v.get(key){if !a.as_array().is_some_and(|a|a.iter().all(Value::is_string)){return Err(format!("{key} должен быть списком строк"))}}}
    for key in ["GlobalProxy","FakeDNS","UseChunkFiles"]{if let Some(x)=v.get(key){if !x.as_str().is_some_and(|s|matches!(s,"true"|"false")){return Err(format!("{key}: ожидается строка true или false"))}}}
    if let Some(x)=v.get("DnsHosts"){if !x.is_object(){return Err("DnsHosts должен быть объектом".into())}}
    for key in ["RemoteDNSType","DomesticDNSType","RemoteDNSDomain","DomesticDNSDomain","RemoteDNSIP","DomesticDNSIP","Geoipurl","Geositeurl","DomainStrategy","RouteOrder"]{if v.get(key).is_some_and(|x|!x.is_string()){return Err(format!("{key} должен быть строкой"))}}
    Ok(())
}
pub fn validate_settings(map:&serde_json::Map<String,Value>)->Result<(),String>{
    for (k,v) in map{
        match k.as_str(){
            "subscription.response_headers"=>validate_headers(&serde_json::from_value::<Vec<ResponseHeader>>(v.clone()).map_err(|_|"Заголовки должны быть списком key/value")?)?,
            "subscription.happ_routing"=>validate_routing(v.as_str().ok_or("Ссылка Happ должна быть строкой")?)?,
            "subscription.require_hwid"=>{if !v.is_boolean(){return Err("Проверка HWID должна быть переключателем".into())}},
            "subscription.update_interval_hours"=>{if !v.as_i64().is_some_and(|n|(1..=168).contains(&n)){return Err("Интервал обновления — целое число от 1 до 168 часов".into())}},
            x if x.starts_with("subscription.remark_")=>{let a=match v{Value::String(s)=>vec![s.as_str()],Value::Array(a)=>a.iter().map(|v|v.as_str().ok_or("Примечания должны быть строками")).collect::<Result<Vec<_>,_>>()?,_=>return Err("Примечания должны быть текстом или списком строк".into())};if a.len()>20||a.iter().any(|s|s.len()>2048){return Err("Не более 20 примечаний по 2048 байт для одного состояния".into())}},
            "subscription.announce"=>{if !v.as_str().is_some_and(|s|s.len()<=4096){return Err("Объявление должно быть текстом не более 4 КБ".into())}},
            _=>{}
        }
    }Ok(())
}

fn parse_template(code:&str,body:&str)->Result<Value,String>{
    // Bare placeholders become JSON strings before parsing. Replacement happens
    // in the value tree, so usernames, quotes and newlines cannot break JSON/YAML.
    let mut probe=body.to_string();
    for t in ["SERVERS","OUTBOUNDS","PROXY_NAMES"]{
        probe=probe.replace(&format!("\"{{{{{t}}}}}\""),&format!("\"__SN_{t}__\""));
        probe=probe.replace(&format!("{{{{{t}}}}}"),&format!("\"__SN_{t}__\""));
    }
    if matches!(code,"mihomo"|"clash"|"stash"){serde_yaml_ng::from_str(&probe).map_err(|e|format!("YAML: {e}"))}else{serde_json::from_str(&probe).map_err(|e|format!("JSON: {e}"))}
}
pub fn validate_template(code:&str,body:&str)->Result<(),String>{
    if !FORMATS.contains(&code){return Err(format!("Неизвестный формат {code}"))}
    if body.trim().is_empty()||body.len()>256*1024{return Err("Шаблон должен содержать от 1 байта до 256 КБ".into())}
    if matches!(code,"plain"|"base64"){if !body.contains("{{SERVERS}}") {return Err("Текстовый шаблон должен содержать {{SERVERS}}".into())}return Ok(())}
    let v=parse_template(code,body)?;
    if !v.is_object()&&!v.is_array()&&v!="__SN_SERVERS__"{return Err("Шаблон должен быть объектом конфигурации или {{SERVERS}}".into())}
    if v.is_array()&&!body.contains("{{SERVERS}}") {return Err("Базовый шаблон должен быть объектом конфигурации".into())}
    if v.get("remnawave").is_some(){return Err("Директивы remnawave требуют адаптации. Используйте обычный конфиг или пресет STEALTHNET".into())}
    Ok(())
}
fn deep_merge(target:&mut Value,source:&Value){
    if let (Some(dst),Some(src))=(target.as_object_mut(),source.as_object()){for (k,v) in src{deep_merge(dst.entry(k.clone()).or_insert(Value::Null),v)}}else{*target=source.clone()}
}
fn expand(v:&mut Value,servers:&Value,outbounds:&Value,names:&Value,title:&str){
    match v{
        Value::String(s)=>{*v=match s.as_str(){"__SN_SERVERS__"=>servers.clone(),"__SN_OUTBOUNDS__"=>outbounds.clone(),"__SN_PROXY_NAMES__"=>names.clone(),_=>Value::String(s.replace("{{TITLE}}",title))}},
        Value::Array(a)=>{for x in a{expand(x,servers,outbounds,names,title)}},
        Value::Object(o)=>{for x in o.values_mut(){expand(x,servers,outbounds,names,title)}},_=>{}
    }
}
pub fn render_template(code:&str,body:&str,hosts:&[HostEntry],title:&str)->Result<String,String>{
    validate_template(code,body)?;
    if matches!(code,"plain"|"base64"){
        let plain=body.replace("{{SERVERS}}",&formats::render_plain(hosts)).replace("{{TITLE}}",title);
        return Ok(if code=="base64"{STANDARD.encode(plain)}else{plain});
    }
    let mut v=parse_template(code,body)?;
    let names=json!(hosts.iter().map(|h|&h.remark).collect::<Vec<_>>());
    if code=="xray_json"{
        let generated=formats::render_xray_json(hosts,title);
        if body.contains("{{SERVERS}}")||body.contains("{{OUTBOUNDS}}"){
            let outs=json!(hosts.iter().enumerate().map(|(i,h)|{let mut o=h.to_xray_outbound();o["tag"]=json!(format!("proxy-{i}"));o}).collect::<Vec<_>>());
            expand(&mut v,&generated,&outs,&names,title);
        }else{
            let balanced=v["stealthnet"]["mode"]=="balanced";
            if let Some(o)=v.as_object_mut(){o.remove("stealthnet");}
            let template=v;
            let mut configs=Vec::new();
            for mut cfg in generated.as_array().cloned().unwrap_or_default(){
                let proxy=cfg["outbounds"][0].clone();let remark=cfg["remarks"].clone();
                deep_merge(&mut cfg,&template);
                if template.get("observatory").is_some(){cfg.as_object_mut().unwrap().remove("burstObservatory");}
                let mut outs=cfg["outbounds"].as_array().cloned().unwrap_or_default();outs.retain(|o|o["tag"]!="proxy"&&!o["tag"].as_str().is_some_and(|s|s.starts_with("proxy-")));
                if balanced{
                    for (i,h) in hosts.iter().enumerate().rev(){let mut o=h.to_xray_outbound();o["tag"]=json!(format!("proxy-{i}"));outs.insert(0,o);}
                    cfg["remarks"]=json!(title);
                }else{outs.insert(0,proxy);cfg["remarks"]=remark;}
                cfg["outbounds"]=json!(outs);expand(&mut cfg,&Value::Null,&Value::Null,&names,title);configs.push(cfg);if balanced{break}
            }v=json!(configs);
        }
        return serde_json::to_string_pretty(&v).map_err(|e|e.to_string())
    }
    if code=="singbox"{
        let generated=formats::render_singbox(hosts,title);let outs=generated["outbounds"].clone();
        let dynamic=json!(outs.as_array().unwrap().iter().filter(|o|o.get("server").is_some()).collect::<Vec<_>>());
        if body.contains("{{SERVERS}}")||body.contains("{{OUTBOUNDS}}"){expand(&mut v,&dynamic,&outs,&names,title)}else{
            let template=v;v=generated;deep_merge(&mut v,&template);
            let mut merged=outs.as_array().cloned().unwrap_or_default();
            for o in template["outbounds"].as_array().into_iter().flatten(){merged.retain(|x|x["tag"]!=o["tag"]);merged.push(o.clone());}v["outbounds"]=json!(merged);expand(&mut v,&dynamic,&outs,&names,title);
        }
        return serde_json::to_string_pretty(&v).map_err(|e|e.to_string())
    }
    let generated:Value=serde_yaml_ng::from_str(&formats::render_clash(hosts,title)).map_err(|e|format!("Генератор YAML: {e}"))?;
    if body.contains("{{SERVERS}}") {expand(&mut v,&generated["proxies"],&Value::Null,&names,title)}else{let template=v;v=generated.clone();deep_merge(&mut v,&template);v["proxies"]=generated["proxies"].clone();expand(&mut v,&generated["proxies"],&Value::Null,&names,title);}
    serde_yaml_ng::to_string(&v).map_err(|e|e.to_string())
}
pub fn format_template(code:&str,body:&str)->Result<String,String>{
    validate_template(code,body)?;
    // Preserve placeholder syntax. Formatting only valid standalone documents
    // avoids converting a bare insertion marker into a quoted literal.
    if body.contains("{{SERVERS}}")||body.contains("{{OUTBOUNDS}}")||body.contains("{{PROXY_NAMES}}") {return Ok(body.to_string())}
    if matches!(code,"plain"|"base64"){return Ok(body.to_string())}
    let v=parse_template(code,body)?;
    if matches!(code,"mihomo"|"clash"|"stash"){serde_yaml_ng::to_string(&v).map_err(|e|e.to_string())}else{serde_json::to_string_pretty(&v).map_err(|e|e.to_string())}
}

#[cfg(test)]
mod tests{
    use super::*;
    fn hosts()->Vec<HostEntry>{let mut a=formats::stub_host("Германия \"A\"\n2");a.address="de.example.com".into();let mut b=a.clone();b.remark="Нидерланды".into();b.address="nl.example.com".into();vec![a,b]}
    #[test]fn custom_observer_replaces_builtin_observer(){let out=render_template("xray_json",r#"{"stealthnet":{"mode":"balanced"},"observatory":{"subjectSelector":["proxy-"]}}"#,&hosts(),"title").unwrap();let v:Value=serde_json::from_str(&out).unwrap();assert!(v[0].get("observatory").is_some());assert!(v[0].get("burstObservatory").is_none());}
    #[test]fn routing_rejects_wrong_field_types(){for payload in [json!({"Name":"test","DirectIp":"bad"}),json!({"Name":"test","GlobalProxy":false}),json!({"Name":"test","DnsHosts":[]})]{let link=format!("happ://routing/add/{}",STANDARD.encode(payload.to_string()));assert!(validate_routing(&link).is_err());}}
    #[test]fn headers_validate_boundaries(){
        for key in ["set-cookie","Content-Type","content-length","cache-control","subscription-userinfo","connection","location","access-control-allow-origin"]{assert!(validate_headers(&[ResponseHeader{key:key.into(),value:"x".into()}]).is_err());}
        assert!(validate_headers(&[ResponseHeader{key:"x-test".into(),value:"ok\r\nSet-Cookie: x".into()}]).is_err());
        assert!(validate_headers(&[ResponseHeader{key:"x-test".into(),value:"1".into()},ResponseHeader{key:"X-Test".into(),value:"2".into()}]).is_err());
    }
    #[test]fn unicode_and_base64_header_values(){let mut h=HeaderMap::new();apply_headers(&mut h,&[ResponseHeader{key:"announce".into(),value:"Привет, {username}".into()},ResponseHeader{key:"x-encoded".into(),value:"rwEncodeBase64:hello".into()}],"a","title");assert_eq!(h["x-encoded"],"base64:aGVsbG8=");assert_eq!(STANDARD.decode(h["announce"].to_str().unwrap().trim_start_matches("base64:")).unwrap(),"Привет, a".as_bytes());}
    #[test]fn all_conditions_and_missing_negative(){
        let cases=[("EQUALS","Happ/3",true),("NOT_EQUALS","other",true),("CONTAINS","APP",true),("NOT_CONTAINS","ios",true),("STARTS_WITH","happ",true),("NOT_STARTS_WITH","n",true),("ENDS_WITH","/3",true),("NOT_ENDS_WITH","/2",true),("REGEX","^happ/\\d+$",true),("NOT_REGEX","^n",true)];let mut h=HeaderMap::new();h.insert("user-agent","Happ/3".parse().unwrap());
        for (op,value,expected) in cases{let c=Condition{header_name:"user-agent".into(),operator:op.into(),value:value.into(),case_sensitive:false};assert!(validate_conditions("AND",&[c.clone()]).is_ok());assert_eq!(conditions_match("AND",&[c.clone()],&h),expected, "{op}");assert!(!conditions_match("AND",&[c],&HeaderMap::new()));}
        let a=Condition{header_name:"user-agent".into(),operator:"CONTAINS".into(),value:"happ".into(),case_sensitive:false};let b=Condition{header_name:"x-device-os".into(),operator:"EQUALS".into(),value:"android".into(),case_sensitive:false};assert!(!conditions_match("AND",&[a.clone(),b.clone()],&h));assert!(conditions_match("OR",&[a,b],&h));
    }
    #[test]fn invalid_regex_is_rejected(){let c=Condition{header_name:"user-agent".into(),operator:"REGEX".into(),value:"[".into(),case_sensitive:false};assert!(validate_conditions("AND",&[c]).is_err());}
    #[test]fn xray_base_preserves_real_proxy_per_location(){let out=render_template("xray_json",r#"{"dns":{"servers":["9.9.9.9"]},"inbounds":[{"port":1080,"protocol":"socks"}],"outbounds":[{"protocol":"freedom","tag":"direct"}]}"#,&hosts(),"title").unwrap();let v:Value=serde_json::from_str(&out).unwrap();assert_eq!(v.as_array().unwrap().len(),2);assert_eq!(v[0]["dns"]["servers"][0],"9.9.9.9");assert_eq!(v[0]["outbounds"][0]["tag"],"proxy");assert_eq!(v[0]["outbounds"][0]["settings"]["vnext"][0]["address"],"de.example.com");assert_eq!(v[1]["remarks"],"Нидерланды");}
    #[test]fn title_is_structurally_escaped(){let out=render_template("xray_json",r#"{"title":"{{TITLE}}","list":{{SERVERS}}}"#,&hosts(),"A\"\nB").unwrap();let v:Value=serde_json::from_str(&out).unwrap();assert_eq!(v["title"],"A\"\nB");assert_eq!(v["list"].as_array().unwrap().len(),2);}
    #[test]fn yaml_templates_apply_for_every_family(){for code in ["clash","mihomo","stash"]{let out=render_template(code,"mixed-port: 8888\nproxies: {{SERVERS}}\nproxy-groups:\n  - name: Select\n    type: select\n    proxies: {{PROXY_NAMES}}\nrules:\n  - MATCH,Select\n",&hosts(),"title").unwrap();let v:Value=serde_yaml_ng::from_str(&out).unwrap();assert_eq!(v["mixed-port"],8888);assert_eq!(v["proxies"].as_array().unwrap().len(),2);assert_eq!(v["proxy-groups"][0]["proxies"].as_array().unwrap().len(),2);}}
    #[test]fn singbox_applies_dns_and_retains_servers(){let out=render_template("singbox",r#"{"log":{"level":"debug"},"dns":{"servers":[{"type":"udp","server":"9.9.9.9"}]}}"#,&hosts(),"title").unwrap();let v:Value=serde_json::from_str(&out).unwrap();assert_eq!(v["log"]["level"],"debug");assert_eq!(v["outbounds"].as_array().unwrap().iter().filter(|o|o["server"].is_string()).count(),2);}
    #[test]fn balanced_xray_has_unique_real_outbounds(){let out=render_template("xray_json",r#"{"stealthnet":{"mode":"balanced"},"routing":{"balancers":[{"tag":"auto","selector":["proxy-"]}]}}"#,&hosts(),"Auto").unwrap();let v:Value=serde_json::from_str(&out).unwrap();assert_eq!(v.as_array().unwrap().len(),1);assert_eq!(v[0]["remarks"],"Auto");assert_eq!(v[0]["outbounds"][0]["tag"],"proxy-0");assert_eq!(v[0]["outbounds"][1]["tag"],"proxy-1");assert!(v[0].get("stealthnet").is_none());}
    #[test]fn text_templates_are_encoded_once(){let b="{{TITLE}}\n{{SERVERS}}";let out=render_template("base64",b,&hosts(),"T").unwrap();let plain=String::from_utf8(STANDARD.decode(out).unwrap()).unwrap();assert!(plain.starts_with("T\nvless://"));assert_eq!(plain.lines().count(),3);}
    #[test]fn routing_and_settings_reject_invalid_data(){assert!(validate_routing("happ://routing/onadd/!!!!").is_err());let l=format!("happ://routing/onadd/{}",STANDARD.encode(r#"{"Name":"Мой VPN","DirectSites":["example.com"]}"#));assert!(validate_routing(&l).is_ok());assert!(validate_routing("happ://routing/off").is_ok());assert!(validate_settings(json!({"subscription.remark_expired":["Окончено","Продлите доступ"]}).as_object().unwrap()).is_ok());assert!(validate_settings(json!({"subscription.remark_expired":[false]}).as_object().unwrap()).is_err());}
}

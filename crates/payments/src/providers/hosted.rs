//! Platega, RollyPay and ParityPay v2 hosted checkout protocols.
//! Contract sources and limitations: docs/payment-gateways.md.
use crate::{Invoice,InvoiceRequest,PaymentProvider,PaymentStatus,SettingField,Settings,WebhookOutcome};
use sn_core::{Error,Result};
use serde_json::{json,Value};
use std::collections::HashMap;
use async_trait::async_trait;
use hmac::{Hmac,Mac};
use sha2::Sha256;

#[derive(Clone,Copy,Debug)]
pub enum Gateway { Platega, RollyPay, ParityPay }
pub struct Hosted { kind:Gateway,cfg:Settings,http:reqwest::Client,base:String }
impl Hosted {
    pub fn new(kind:Gateway)->Self {
        let base=match kind {Gateway::Platega=>"https://app.platega.io",Gateway::RollyPay=>"https://rollypay.io",Gateway::ParityPay=>"https://api.paritypay.net"}.into();
        Self{kind,cfg:Settings::default(),http:reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build().expect("HTTP client"),base}
    }
    fn required(&self,key:&str)->Result<String>{self.cfg.get(key).ok_or_else(||Error::bad(format!("{}: заполните {key} в настройках платёжного модуля",self.title())))}
    fn request(&self,method:reqwest::Method,path:&str)->Result<reqwest::RequestBuilder>{
        let mut req=self.http.request(method,format!("{}{path}",self.base));
        req=match self.kind {
            Gateway::Platega=>req.header("X-MerchantId",self.required("merchant_id")?).header("X-Secret",self.required("secret")?),
            Gateway::RollyPay=>req.header("X-API-Key",self.required("api_key")?).header("X-Nonce",uuid::Uuid::new_v4().to_string()),
            Gateway::ParityPay=>req.header("X-ShopId",self.required("shop_id")?).header("X-SecretKey",self.required("secret_key")?),
        };Ok(req)
    }
    async fn response(&self,req:reqwest::RequestBuilder)->Result<Value>{
        let response=req.send().await.map_err(|_|Error::Internal(format!("{}: не удалось связаться с API, повторите позже",self.title())))?;
        let status=response.status();
        // Do not echo raw gateway responses: they may contain account data or credentials.
        if !status.is_success(){return Err(Error::bad(format!("{}: API вернул HTTP {}. Проверьте ключи, валюту и ограничения кассы.",self.title(),status.as_u16())));}
        response.json().await.map_err(|_|Error::bad(format!("{}: некорректный ответ API",self.title())))
    }
    fn body(&self,req:&InvoiceRequest)->Result<Value>{
        if req.amount_minor<=0 || !self.currencies().contains(&req.currency){return Err(Error::bad("валюта или сумма не поддерживается платёжным модулем"));}
        let amount=crate::minor_to_units(req.amount_minor,&req.currency);
        let order=format!("sn-{}",req.payment_id);
        let return_url=req.return_url.clone().or_else(||self.cfg.get("return_url"));
        let mut body=match self.kind {
            Gateway::Platega=>json!({"paymentDetails":{"amount":serde_json::from_str::<Value>(&amount).map_err(|_|Error::bad("сумма"))?,"currency":req.currency},"description":req.description,"payload":order,"metadata":{"userId":req.telegram_id.unwrap_or(req.client_id).to_string()}}),
            Gateway::RollyPay=>{
                if req.currency=="EUR" && req.amount_minor<100{return Err(Error::bad("RollyPay: минимальная сумма — 1 EUR"));}
                let mut body=json!({"amount":amount,"payment_currency":req.currency,"order_id":order,"description":req.description,"customer_id":req.client_id.to_string()});
                if req.currency=="EUR" {body["payment_method"]=json!("intl_card");}
                else if let Some(method)=self.cfg.get("payment_method") {body["payment_method"]=json!(method);}
                body
            },
            Gateway::ParityPay=>{
                let mut body=json!({"order_id":order,"amount":serde_json::from_str::<Value>(&amount).map_err(|_|Error::bad("сумма"))?,"comment":req.description.chars().take(255).collect::<String>(),"expire":60});
                if let Some(service)=self.cfg.get("service"){body["service"]=json!(service);}
                if let Some(url)=self.cfg.get("callback_url"){body["callback_url"]=json!(url);}
                body
            }
        };
        if let Some(url)=return_url {
            match self.kind {Gateway::Platega=>{body["return"]=json!(url);body["failedUrl"]=json!(url);},Gateway::RollyPay=>{body["success_redirect_url"]=json!(url);body["fail_redirect_url"]=json!(url);},Gateway::ParityPay=>{body["success_url"]=json!(url);body["fail_url"]=json!(url);}}
        }
        Ok(body)
    }
    fn parsed_outcome(&self,v:Value,webhook:bool)->Result<WebhookOutcome>{
        let (txid,order,currency,amount,status)=match self.kind {
            Gateway::Platega=>(text(&v,"id")?,v.get("payload").and_then(Value::as_str),if webhook{text(&v,"currency")?}else{text(&v["paymentDetails"],"currency")?},if webhook{&v["amount"]}else{&v["paymentDetails"]["amount"]},text(&v,"status")?),
            Gateway::RollyPay=>{
                // This module uses live checkout only. Sandbox notifications cannot unlock real access.
                if v["test"].as_bool()==Some(true){return Err(Error::bad("RollyPay: тестовый платёж не выдаёт доступ"));}
                (text(&v,"payment_id")?,v.get("order_id").and_then(Value::as_str),if webhook{text(&v,"currency")?}else{text(&v,"payment_currency")?},&v["amount"],text(&v,"status")?)
            },
            Gateway::ParityPay=>{
                if v["shop_id"].as_str()!=Some(self.required("shop_id")?.as_str()){return Err(Error::Unauthorized);}
                (text(&v,"id")?,v.get("order_id").and_then(Value::as_str),"RUB",&v["amount"],text(&v,"status")?)
            }
        };
        if !self.currencies().iter().any(|c|c==currency){return Err(Error::bad("неподдерживаемая валюта уведомления"));}
        let parsed_status=match self.kind {
            Gateway::Platega=>match status {"CONFIRMED"=>PaymentStatus::Success,"PENDING"=>PaymentStatus::Pending,"CANCELED"=>PaymentStatus::Canceled,"CHARGEBACKED"=>PaymentStatus::Refunded,_=>return Err(Error::bad("Platega: неизвестный статус"))},
            Gateway::RollyPay=>match status {"paid"=>PaymentStatus::Success,"created"|"processing"=>PaymentStatus::Pending,"expired"|"canceled"=>PaymentStatus::Canceled,"chargeback"|"refunded"=>PaymentStatus::Refunded,_=>return Err(Error::bad("RollyPay: неизвестный статус"))},
            Gateway::ParityPay=>match status {"PAID"=>PaymentStatus::Success,"NEW"=>PaymentStatus::Pending,"EXPIRED"=>PaymentStatus::Canceled,"ERROR"=>PaymentStatus::Failed,"REFUNDED"=>PaymentStatus::Refunded,_=>return Err(Error::bad("ParityPay: неизвестный статус"))},
        };
        let payment_id=order.and_then(|s|s.strip_prefix("sn-")).and_then(|s|s.parse::<i64>().ok()).filter(|n|*n>0);
        let amount_minor=amount_minor(amount)?;
        Ok(WebhookOutcome{external_event_id:Some(format!("{txid}:{status}")),payment_id,provider_txid:Some(txid.into()),status:parsed_status,amount_minor:Some(amount_minor),currency:Some(currency.into()),error:None,raw:v})
    }
}
fn text<'a>(v:&'a Value,key:&str)->Result<&'a str>{v.get(key).and_then(Value::as_str).filter(|s|!s.is_empty()).ok_or_else(||Error::bad(format!("в ответе платёжного модуля отсутствует {key}")))}
/// Strict decimal conversion; never round an underpayment into a successful purchase.
pub(super) fn amount_minor(v:&Value)->Result<i64>{
    let raw=match v {Value::String(s)=>s.clone(),Value::Number(n)=>n.to_string(),_=>return Err(Error::bad("нет суммы платежа"))};
    let (whole,fraction)=raw.split_once('.').unwrap_or((&raw,""));
    let fraction=fraction.trim_end_matches('0');
    if whole.is_empty() || !whole.bytes().all(|b|b.is_ascii_digit()) || fraction.len()>2 || !fraction.bytes().all(|b|b.is_ascii_digit()){return Err(Error::bad("некорректная сумма платежа"));}
    let w=whole.parse::<i64>().map_err(|_|Error::bad("сумма слишком велика"))?;
    let f=if fraction.is_empty(){0}else{fraction.parse::<i64>().map_err(|_|Error::bad("сумма"))? * if fraction.len()==1{10}else{1}};
    w.checked_mul(100).and_then(|n|n.checked_add(f)).ok_or_else(||Error::bad("сумма слишком велика"))
}
fn verify_hmac(secret:&str,payload:&[u8],signature:&str)->Result<()> {
    let signature=hex::decode(signature).map_err(|_|Error::Unauthorized)?;
    let mut mac=Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_|Error::Unauthorized)?;
    mac.update(payload);mac.verify_slice(&signature).map_err(|_|Error::Unauthorized)
}
fn secret_equal(a:&str,b:&str)->bool {
    let mut mac=Hmac::<Sha256>::new_from_slice(a.as_bytes()).expect("HMAC accepts any key");mac.update(b"verify");let digest=mac.finalize().into_bytes();
    let mut other=Hmac::<Sha256>::new_from_slice(b.as_bytes()).expect("HMAC accepts any key");other.update(b"verify");other.verify_slice(&digest).is_ok()
}
fn parity_signed_data(v:&Value)->Result<String>{
    let map=v.as_object().ok_or_else(||Error::bad("ожидается объект уведомления"))?;
    let mut keys:Vec<_>=map.keys().collect();keys.sort();let mut out=String::new();
    for key in keys {match &map[key] {Value::Null=>{},Value::String(s)=>out.push_str(s),Value::Number(n)=>{let raw=n.to_string();out.push_str(raw.strip_suffix(".0").unwrap_or(&raw));},_=>return Err(Error::bad("ParityPay: неверный тип поля подписи"))}}
    Ok(out)
}
#[async_trait]
impl PaymentProvider for Hosted {
    fn id(&self)->&'static str {match self.kind{Gateway::Platega=>"platega",Gateway::RollyPay=>"rollypay",Gateway::ParityPay=>"paritypay"}}
    fn title(&self)->&str {match self.kind{Gateway::Platega=>"Platega",Gateway::RollyPay=>"RollyPay",Gateway::ParityPay=>"ParityPay v2"}}
    fn accepts_http_webhooks(&self)->bool{true}
    fn currencies(&self)->Vec<String>{match self.kind{Gateway::RollyPay=>vec!["RUB".into(),"EUR".into()],_=>vec!["RUB".into()]}}
    fn is_configured(&self)->bool{self.settings_schema().iter().filter(|f|f.required).all(|f|self.cfg.get(f.key).is_some()) && self.validate_settings(&self.cfg.snapshot()).is_ok()}
    fn settings(&self)->Option<Settings>{Some(self.cfg.clone())}
    fn sort_order(&self)->i32{match self.kind{Gateway::Platega=>200,Gateway::RollyPay=>210,Gateway::ParityPay=>220}}
    fn settings_schema(&self)->Vec<SettingField>{
        let mut fields=match self.kind {
            Gateway::Platega=>vec![SettingField::text("merchant_id","Merchant ID","UUID магазина в настройках Platega").required(true),SettingField::secret("secret","API-ключ / X-Secret","Проверяется также в уведомлениях Platega. Callback URL задаётся в кабинете Platega")],
            Gateway::RollyPay=>vec![SettingField::secret("api_key","API key кассы","Ключ rpk_live_… в кабинете RollyPay"),SettingField::secret("signing_secret","Секрет подписи","signing_secret кассы для HMAC-SHA256 уведомлений"),SettingField::text("payment_method","Способ оплаты","Пусто — выбор на форме. sbp, card, intl_card или crypto. Для EUR автоматически intl_card")],
            Gateway::ParityPay=>vec![SettingField::text("shop_id","ID кассы","UUID кассы ParityPay").required(true),SettingField::secret("secret_key","Секретный ключ №1","Доступ к API v2: X-SecretKey"),SettingField::secret("webhook_secret","Секретный ключ №2","Проверка подписи HTTP-уведомлений: X-SIGNATURE"),SettingField::text("service","Способ оплаты","Пусто — выбор на форме; sbp или card"),SettingField::text("callback_url","URL уведомлений","HTTPS-адрес /api/pay/webhook/paritypay вашей панели. Пусто — используется адрес из настроек кассы")],
        };
        fields.push(SettingField::text("return_url","Адрес возврата после оплаты","Необязательно: HTTPS-адрес вашего бота или клиентского сайта"));fields
    }
    fn validate_settings(&self,cfg:&serde_json::Map<String,Value>)->Result<()> {
        for key in ["return_url","callback_url"] {if let Some(value)=cfg.get(key).and_then(Value::as_str).filter(|s|!s.is_empty()) {if !sn_core::bot_config::web_url(value,true) || value.len()>500{return Err(Error::bad(format!("{key}: нужен HTTPS-адрес до 500 символов")));}}}
        for key in ["merchant_id","shop_id"] {if let Some(value)=cfg.get(key).and_then(Value::as_str).filter(|s|!s.is_empty()) {if uuid::Uuid::parse_str(value).is_err(){return Err(Error::bad(format!("{key}: нужен UUID магазина")));}}}
        if let Some(value)=cfg.get("payment_method").and_then(Value::as_str).filter(|s|!s.is_empty()){if !["sbp","card","intl_card","crypto"].contains(&value){return Err(Error::bad("RollyPay: допустимы sbp, card, intl_card, crypto"));}}
        if let Some(value)=cfg.get("service").and_then(Value::as_str).filter(|s|!s.is_empty()){if !["sbp","card"].contains(&value){return Err(Error::bad("ParityPay: допустимы sbp и card"));}}
        Ok(())
    }
    async fn create_invoice(&self,req:&InvoiceRequest)->Result<Invoice>{
        self.validate_settings(&self.cfg.snapshot())?;
        let body=self.body(req)?;
        let path=match self.kind{Gateway::Platega=>"/v2/transaction/process",Gateway::RollyPay=>"/api/v1/payments",Gateway::ParityPay=>"/v2/invoice/create"};
        let value=self.response(self.request(reqwest::Method::POST,path)?.json(&body)).await?;
        let (id,url)=match self.kind {Gateway::Platega=>(text(&value,"transactionId")?,text(&value,"url")?),Gateway::RollyPay=>(text(&value,"payment_id")?,text(&value,"pay_url")?),Gateway::ParityPay=>(text(&value,"id")?,text(&value,"link")?)};
        if !sn_core::bot_config::web_url(url,true){return Err(Error::bad("платёжный модуль вернул некорректную ссылку"));}
        let expires=match self.kind {
            Gateway::Platega=>value["expiresIn"].as_str().and_then(|s|{let p:Vec<i64>=s.split(':').filter_map(|s|s.parse().ok()).collect();if p.len()==3{Some((p[0]*3600+p[1]*60+p[2]+59)/60)}else{None}}).unwrap_or(15),
            Gateway::RollyPay=>value["expires_at"].as_str().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d|((d.timestamp()-chrono::Utc::now().timestamp())+59)/60).unwrap_or(30),
            Gateway::ParityPay=>60,
        }.clamp(1,43200);
        Ok(Invoice{pay_url:url.into(),external_id:Some(id.into()),expires_in_minutes:expires,payload:json!({"gateway":self.id(),"order_id":format!("sn-{}",req.payment_id)})})
    }
    async fn handle_webhook(&self,headers:&HashMap<String,String>,body:&[u8])->Result<WebhookOutcome>{
        let v:Value=serde_json::from_slice(body).map_err(|_|Error::bad("некорректное уведомление"))?;
        match self.kind {
            Gateway::Platega=>{if !secret_equal(headers.get("x-secret").ok_or(Error::Unauthorized)?,&self.required("secret")?) || !secret_equal(headers.get("x-merchantid").ok_or(Error::Unauthorized)?,&self.required("merchant_id")?){return Err(Error::Unauthorized);}},
            Gateway::RollyPay=>{
                let timestamp=headers.get("x-timestamp").ok_or(Error::Unauthorized)?;
                let ts=timestamp.parse::<i64>().map_err(|_|Error::Unauthorized)?;
                if chrono::Utc::now().timestamp().abs_diff(ts)>300{return Err(Error::Unauthorized);}
                let mut message=timestamp.as_bytes().to_vec();message.push(b'.');message.extend_from_slice(body);
                verify_hmac(&self.required("signing_secret")?,&message,headers.get("x-signature").ok_or(Error::Unauthorized)?)?;
            },
            Gateway::ParityPay=>verify_hmac(&self.required("webhook_secret")?,parity_signed_data(&v)?.as_bytes(),headers.get("x-signature").ok_or(Error::Unauthorized)?)?,
        }
        self.parsed_outcome(v,true)
    }
    async fn poll_outcome(&self,external:&str)->Result<Option<WebhookOutcome>>{
        if external.is_empty() || external.len()>100 || !external.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'){return Err(Error::bad("некорректный идентификатор счёта"));}
        let path=match self.kind{Gateway::Platega=>format!("/transaction/{external}"),Gateway::RollyPay=>format!("/api/v1/payments/{external}"),Gateway::ParityPay=>format!("/v2/invoice/status?id={external}")};
        let v=self.response(self.request(reqwest::Method::GET,&path)?).await?;
        let outcome=self.parsed_outcome(v,false)?;
        if outcome.provider_txid.as_deref()!=Some(external){return Err(Error::bad("платёжный модуль вернул другой счёт"));}
        Ok(Some(outcome))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configured(kind:Gateway)->Hosted {
        let p=Hosted::new(kind);
        p.cfg.replace(json!({"merchant_id":"11111111-1111-4111-8111-111111111111","secret":"qa-secret","api_key":"qa-key","signing_secret":"qa-signing","shop_id":"11111111-1111-4111-8111-111111111111","secret_key":"qa-key1","webhook_secret":"qa-key2"}).as_object().unwrap().clone());p
    }
    fn signature(key:&str,payload:&[u8])->String {let mut m=Hmac::<Sha256>::new_from_slice(key.as_bytes()).unwrap();m.update(payload);hex::encode(m.finalize().into_bytes())}
    fn notification(kind:Gateway)->Value {match kind {
        Gateway::Platega=>json!({"id":"test-transaction","amount":19.99,"currency":"RUB","status":"CONFIRMED","paymentMethod":2}),
        Gateway::RollyPay=>json!({"payment_id":"pay-test","order_id":"sn-77","amount":"19.99","currency":"RUB","status":"paid","event_type":"payment.paid","test":false}),
        Gateway::ParityPay=>json!({"id":"test-transaction","order_id":"sn-77","shop_id":"11111111-1111-4111-8111-111111111111","amount":"19.99","credited":19.0,"comment":null,"custom_fields":null,"status":"PAID"}),
    }}
    fn signed_headers(kind:Gateway,v:&Value,body:&[u8])->HashMap<String,String>{
        let mut h=HashMap::new();match kind{
            Gateway::Platega=>{h.insert("x-merchantid".into(),"11111111-1111-4111-8111-111111111111".into());h.insert("x-secret".into(),"qa-secret".into());},
            Gateway::RollyPay=>{let ts=chrono::Utc::now().timestamp().to_string();let mut data=format!("{ts}.").into_bytes();data.extend_from_slice(body);h.insert("x-timestamp".into(),ts);h.insert("x-signature".into(),signature("qa-signing",&data));},
            Gateway::ParityPay=>{h.insert("x-signature".into(),signature("qa-key2",parity_signed_data(v).unwrap().as_bytes()));},
        }h
    }
    #[tokio::test] async fn callbacks_require_authentic_amount_and_account(){
        for kind in [Gateway::Platega,Gateway::RollyPay,Gateway::ParityPay] {
            let p=configured(kind);assert!(p.is_configured());let value=notification(kind);let body=serde_json::to_vec(&value).unwrap();let headers=signed_headers(kind,&value,&body);
            let out=p.handle_webhook(&headers,&body).await.unwrap();assert_eq!(out.status,PaymentStatus::Success);assert_eq!(out.amount_minor,Some(1999));assert_eq!(out.currency.as_deref(),Some("RUB"));
            assert!(p.handle_webhook(&HashMap::new(),&body).await.is_err());
            let mut wrong=headers.clone();for value in wrong.values_mut(){*value="bad".into();}assert!(p.handle_webhook(&wrong,&body).await.is_err());
            let mut broken=value.clone();broken["status"]=json!("not-paid");let data=serde_json::to_vec(&broken).unwrap();assert!(p.handle_webhook(&signed_headers(kind,&broken,&data),&data).await.is_err());
            if !matches!(kind,Gateway::Platega) {let mut tampered=value.clone();tampered["amount"]=json!("999.00");assert!(p.handle_webhook(&headers,&serde_json::to_vec(&tampered).unwrap()).await.is_err());}
        }
        let p=configured(Gateway::RollyPay);let mut v=notification(Gateway::RollyPay);v["test"]=json!(true);let b=serde_json::to_vec(&v).unwrap();assert!(p.handle_webhook(&signed_headers(Gateway::RollyPay,&v,&b),&b).await.is_err());
        let mut v=notification(Gateway::ParityPay);v["shop_id"]=json!("different");let b=serde_json::to_vec(&v).unwrap();assert!(configured(Gateway::ParityPay).handle_webhook(&signed_headers(Gateway::ParityPay,&v,&b),&b).await.is_err());
    }
    fn return_url_cases()->[(Gateway,&'static str,[&'static str;2]);4] {
        [
            (Gateway::Platega,"RUB",["return","failedUrl"]),
            (Gateway::RollyPay,"RUB",["success_redirect_url","fail_redirect_url"]),
            (Gateway::RollyPay,"EUR",["success_redirect_url","fail_redirect_url"]),
            (Gateway::ParityPay,"RUB",["success_url","fail_url"]),
        ]
    }
    #[test] fn invoice_return_url_prefers_request_over_provider_default(){
        for (kind,currency,fields) in return_url_cases() {
            for default in [Some("https://service.example/app/"),None] {
                let provider=configured(kind);
                if let Some(url)=default {let mut cfg=provider.cfg.snapshot();cfg.insert("return_url".into(),json!(url));provider.cfg.replace(cfg);}
                let req=InvoiceRequest{payment_id:77,client_id:5,telegram_id:None,amount_minor:17500,currency:currency.into(),description:"Test plan".into(),return_url:Some("https://cabinet.example/#payments".into())};
                let body=provider.body(&req).unwrap();
                for field in fields {assert_eq!(body[field],json!("https://cabinet.example/#payments"),"{kind:?} {currency} {field}, configured default: {default:?}");}
            }
        }
    }
    #[test] fn invoice_return_url_falls_back_to_provider_default(){
        for (kind,currency,fields) in return_url_cases() {
            let provider=configured(kind);
            let mut cfg=provider.cfg.snapshot();cfg.insert("return_url".into(),json!("https://service.example/app/"));provider.cfg.replace(cfg);
            let req=InvoiceRequest{payment_id:77,client_id:5,telegram_id:Some(11),amount_minor:17500,currency:currency.into(),description:"Test plan".into(),return_url:None};
            let body=provider.body(&req).unwrap();
            for field in fields {assert_eq!(body[field],json!("https://service.example/app/"),"{kind:?} {currency} {field}");}
        }
    }
    #[test] fn invoice_return_url_is_omitted_when_unset(){
        for (kind,currency,fields) in return_url_cases() {
            let req=InvoiceRequest{payment_id:77,client_id:5,telegram_id:Some(11),amount_minor:17500,currency:currency.into(),description:"Test plan".into(),return_url:None};
            let body=configured(kind).body(&req).unwrap();
            for field in fields {assert!(body.get(field).is_none(),"{kind:?} {currency}: unexpected {field}");}
        }
    }
    #[test] fn decimal_and_invoice_contracts(){
        for (v,n) in [(json!("19.99"),1999),(json!(19.9),1990),(json!("0.01"),1),(json!(1200),120000)]{assert_eq!(amount_minor(&v).unwrap(),n);}
        for v in [json!("19.999"),json!("-1"),json!("NaN"),json!("1e9"),Value::Null,json!("999999999999999999999")]{assert!(amount_minor(&v).is_err());}
        let req=InvoiceRequest{payment_id:77,client_id:5,telegram_id:Some(11),amount_minor:1999,currency:"RUB".into(),description:"Тариф \"Плюс\"".into(),return_url:None};
        let a=configured(Gateway::Platega).body(&req).unwrap();assert_eq!(a["paymentDetails"]["amount"],json!(19.99));assert_eq!(a["payload"],"sn-77");assert_eq!(a["metadata"]["userId"],"11");assert!(a.get("id").is_none());
        let a=configured(Gateway::ParityPay).body(&req).unwrap();assert_eq!(a["order_id"],"sn-77");assert_eq!(a["amount"],json!(19.99));assert!(a.get("subscription").is_none());
        let req=InvoiceRequest{currency:"EUR".into(),..req};let a=configured(Gateway::RollyPay).body(&req).unwrap();assert_eq!(a["payment_method"],"intl_card");assert_eq!(a["amount"],"19.99");assert!(configured(Gateway::ParityPay).body(&req).is_err());
        assert_eq!(parity_signed_data(&json!({"z":null,"b":19.0,"a":"A","x":19.99})).unwrap(),"A1919.99");
    }
    #[tokio::test] async fn hosted_http_create_and_poll_use_documented_protocols(){
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        for kind in [Gateway::Platega,Gateway::RollyPay,Gateway::ParityPay] {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
            let post=match kind {Gateway::Platega=>json!({"transactionId":"tx-123","url":"https://pay.platega.io/?id=tx-123","expiresIn":"00:15:00"}),Gateway::RollyPay=>json!({"payment_id":"tx-123","pay_url":"https://pay.rollypay.io/pay/tx-123","expires_at":(chrono::Utc::now()+chrono::Duration::minutes(30)).to_rfc3339()}),Gateway::ParityPay=>json!({"id":"tx-123","link":"https://pay.paritypay.net/tx-123"})};
            let get=match kind {Gateway::Platega=>json!({"id":"tx-123","status":"CONFIRMED","paymentDetails":{"amount":19.99,"currency":"RUB"},"payload":"sn-77"}),Gateway::RollyPay=>json!({"payment_id":"tx-123","order_id":"sn-77","amount":"19.99","payment_currency":"RUB","status":"paid"}),Gateway::ParityPay=>json!({"id":"tx-123","order_id":"sn-77","shop_id":"11111111-1111-4111-8111-111111111111","amount":19.99,"status":"PAID"})};
            let server=tokio::spawn(async move {
                let mut requests=Vec::new();for value in [post,get] {
                    let (mut stream,_)=listener.accept().await.unwrap();let mut raw=Vec::new();let mut buf=[0u8;4096];
                    loop {let n=stream.read(&mut buf).await.unwrap();if n==0{break;}raw.extend_from_slice(&buf[..n]);if let Some(at)=raw.windows(4).position(|v|v==b"\r\n\r\n"){let h=String::from_utf8_lossy(&raw[..at]);let size=h.lines().find_map(|l|l.to_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);if raw.len()>=at+4+size{break;}}}
                    requests.push(String::from_utf8(raw).unwrap());let body=value.to_string();stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }requests
            });
            let mut p=configured(kind);p.base=format!("http://{address}");let req=InvoiceRequest{payment_id:77,client_id:5,telegram_id:None,amount_minor:1999,currency:"RUB".into(),description:"Докупка".into(),return_url:None};
            let invoice=p.create_invoice(&req).await.unwrap();assert_eq!(invoice.external_id.as_deref(),Some("tx-123"));let out=p.poll_outcome("tx-123").await.unwrap().unwrap();assert_eq!(out.amount_minor,Some(1999));assert_eq!(out.payment_id,Some(77));
            let requests=server.await.unwrap();let (post_path,get_path,header)=match kind {Gateway::Platega=>("/v2/transaction/process","/transaction/tx-123","x-merchantid:"),Gateway::RollyPay=>("/api/v1/payments","/api/v1/payments/tx-123","x-nonce:"),Gateway::ParityPay=>("/v2/invoice/create","/v2/invoice/status?id=tx-123","x-shopid:")};
            assert!(requests[0].starts_with(&format!("POST {post_path} ")));assert!(requests[1].starts_with(&format!("GET {get_path} ")));for r in &requests{assert!(r.to_lowercase().contains(header));}
        }
    }

    #[test]
    fn sn_b05_exact_decimal_matrix() {
        for (raw,expected) in [("0",0),("0.00000000",0),("19.9",1990),("19.990",1999),("19.99000000",1999),("0.01000000",1),("1.00000000",100),("92233720368547758.07000000",i64::MAX)] {
            assert_eq!(amount_minor(&json!(raw)).unwrap(),expected,"{raw}");
        }
        for (raw,expected) in [("0.00000000",0),("19.99000000",1999),("19.90000000",1990)] {
            assert_eq!(amount_minor(&serde_json::from_str::<Value>(raw).unwrap()).unwrap(),expected);
        }
    }
    #[test]
    fn sn_b05_reject_fractional_cents_and_overflow() {
        for raw in ["19.99900000","0.00000001","-1","-0.00","+1"," 1","1e2","1..00",".01","NaN","92233720368547758.08","9223372036854775808.00","1.00x00"] {
            assert!(amount_minor(&json!(raw)).is_err(),"{raw}");
        }
        for v in [Value::Null,json!(-1),json!(19.999),json!(true)] {assert!(amount_minor(&v).is_err());}
    }
    #[tokio::test]
    async fn sn_b05_signed_statuses_keep_authentication() {
        let p=configured(Gateway::RollyPay);
        for (status,expected) in [("created",PaymentStatus::Pending),("processing",PaymentStatus::Pending),("paid",PaymentStatus::Success),("expired",PaymentStatus::Canceled),("canceled",PaymentStatus::Canceled)] {
            let mut v=notification(Gateway::RollyPay);v["amount"]=json!("19.99000000");v["status"]=json!(status);
            let b=serde_json::to_vec(&v).unwrap();let h=signed_headers(Gateway::RollyPay,&v,&b);
            let out=p.handle_webhook(&h,&b).await.unwrap();assert_eq!(out.status,expected);assert_eq!(out.amount_minor,Some(1999));
            assert!(p.handle_webhook(&HashMap::new(),&b).await.is_err());
            let mut bad=v.clone();bad["amount"]=json!("29.99000000");assert!(p.handle_webhook(&h,&serde_json::to_vec(&bad).unwrap()).await.is_err());
            bad=v.clone();bad["currency"]=json!("USD");let bytes=serde_json::to_vec(&bad).unwrap();assert!(p.handle_webhook(&signed_headers(Gateway::RollyPay,&bad,&bytes),&bytes).await.is_err());
        }
    }
    #[tokio::test]
    async fn sn_b05_poll_exact_amount_over_loopback() {
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {
            let (mut s,_)=listener.accept().await.unwrap();let mut raw=vec![];let mut buf=[0;4096];
            while !raw.windows(4).any(|x|x==b"\r\n\r\n") {let n=s.read(&mut buf).await.unwrap();assert!(n>0);raw.extend_from_slice(&buf[..n]);}
            let body=json!({"payment_id":"pay-test","order_id":"sn-77","amount":"19.99000000","payment_currency":"RUB","status":"paid"}).to_string();
            s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            assert!(String::from_utf8(raw).unwrap().starts_with("GET /api/v1/payments/pay-test "));
        });
        let mut p=configured(Gateway::RollyPay);p.base=format!("http://{addr}");
        let result=p.poll_outcome("pay-test").await;server.await.unwrap();let out=result.unwrap().unwrap();assert_eq!(out.amount_minor,Some(1999));assert_eq!(out.payment_id,Some(77));
    }
    #[tokio::test]
    #[ignore = "Requires isolated SN_TEST_DATABASE_URL; local synthetic notifications only"]
    async fn sn_b05_repeated_notification_preserves_invoice_and_term() {
        let url=std::env::var("SN_TEST_DATABASE_URL").expect("isolated database required");let pool=sn_core::db::connect(&url).await.unwrap();
        let name:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();assert!(name.starts_with("sn_audit_"));
        let tag=uuid::Uuid::new_v4().to_string();let tariff:i64=sqlx::query_scalar("SELECT id FROM tariffs WHERE code='CLOUD_AUDIT_PAID'").fetch_one(&pool).await.unwrap();
        let client:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES('sn_b05_fixture',$1) RETURNING id").bind(&tag).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO subscriptions(client_id,tariff_id,expires_at) VALUES($1,$2,now())").bind(client).bind(tariff).execute(&pool).await.unwrap();
        let id:i64=sqlx::query_scalar("INSERT INTO payments(client_id,tariff_id,kind,status,amount_minor,currency,period_days,provider,provider_txid) VALUES($1,$2,'renewal','pending',1999,'RUB',30,'rollypay',$3) RETURNING id").bind(client).bind(tariff).bind(&tag).fetch_one(&pool).await.unwrap();
        let mut v=notification(Gateway::RollyPay);v["order_id"]=json!(format!("sn-{id}"));v["payment_id"]=json!(&tag);v["amount"]=json!("19.99000000");
        let p=configured(Gateway::RollyPay);let bytes=serde_json::to_vec(&v).unwrap();let mut out=p.handle_webhook(&signed_headers(Gateway::RollyPay,&v,&bytes),&bytes).await.unwrap();
        let reg=crate::Registry::from_env();reg.sync_to_db(&pool).await.unwrap();
        out.amount_minor=Some(1998);assert!(reg.apply_outcome(&pool,"rollypay",&out).await.is_err());
        out.amount_minor=Some(1999);out.currency=Some("EUR".into());assert!(reg.apply_outcome(&pool,"rollypay",&out).await.is_err());
        out.currency=Some("RUB".into());out.provider_txid=Some("wrong-invoice".into());assert!(reg.apply_outcome(&pool,"rollypay",&out).await.is_err());
        out.provider_txid=Some(tag.clone());reg.apply_outcome(&pool,"rollypay",&out).await.unwrap();
        let expiry:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await.unwrap();
        reg.apply_outcome(&pool,"rollypay",&out).await.unwrap();
        let after:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await.unwrap();assert_eq!(expiry,after);
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM payments WHERE client_id=$1 AND status='success'").bind(client).fetch_one(&pool).await.unwrap();assert_eq!(n,1);
        println!("SN-B05 real DB: signed decimal notification settled once; amount/currency/invoice guards and duplicate term preserved");
    }

    #[tokio::test]
    #[ignore = "Requires SN_COMBINED_FIXTURE pointing to isolated real API, database and synthetic web account"]
    async fn combined_amount_http_notification_and_return_contract() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let fixture: Value = serde_json::from_slice(&std::fs::read(
            std::env::var("SN_COMBINED_FIXTURE").expect("combined fixture must be configured")
        ).unwrap()).unwrap();
        let db_url=fixture["database_url"].as_str().unwrap();
        let pool=sn_core::db::connect(db_url).await.unwrap();
        let db_name:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
        assert!(db_name.starts_with("sn_audit_r3_web_"));
        let api=fixture["api_url"].as_str().unwrap();assert!(api.starts_with("http://127.0.0.1:"));
        let web_return=format!("{}/#payments",fixture["cabinet_url"].as_str().unwrap());
        let tariff=fixture["tariff_id"].as_i64().unwrap();
        let http=reqwest::Client::new();let mut proofs=vec![];
        for channel in ["web","bot","miniapp"] {
            let tag=uuid::Uuid::new_v4().simple().to_string();
            let client=if channel=="web" {fixture["web_client_id"].as_i64().unwrap()} else {
                let id:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES($1,$2) RETURNING id").bind(format!("combined_{channel}")).bind(&tag).fetch_one(&pool).await.unwrap();
                sqlx::query("INSERT INTO subscriptions(client_id,tariff_id) VALUES($1,$2)").bind(id).bind(tariff).execute(&pool).await.unwrap();id
            };
            sqlx::query("UPDATE subscriptions SET expires_at=now()+interval '1 day',tariff_id=$2 WHERE client_id=$1 AND is_current").bind(client).bind(tariff).execute(&pool).await.unwrap();
            let before:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await.unwrap();
            let payment:i64=sqlx::query_scalar("INSERT INTO payments(client_id,tariff_id,kind,status,amount_minor,currency,period_days,provider) VALUES($1,$2,'renewal','pending',1999,'RUB',30,'rollypay') RETURNING id").bind(client).bind(tariff).fetch_one(&pool).await.unwrap();
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
            let response=json!({"payment_id":tag,"pay_url":"https://checkout.example.test/synthetic"}).to_string();
            let server=tokio::spawn(async move {
                let (mut stream,_)=listener.accept().await.unwrap();let mut raw=vec![];let mut buf=[0;4096];
                loop {let n=stream.read(&mut buf).await.unwrap();assert!(n>0);raw.extend_from_slice(&buf[..n]);
                    if let Some(at)=raw.windows(4).position(|x|x==b"\r\n\r\n") {
                        let h=String::from_utf8_lossy(&raw[..at]);let size=h.lines().find_map(|x|x.to_lowercase().strip_prefix("content-length:").and_then(|n|n.trim().parse::<usize>().ok())).unwrap();
                        if raw.len()>=at+4+size {break;}
                    }
                }
                assert!(raw.starts_with(b"POST /api/v1/payments HTTP/1.1\r\n"));
                let at=raw.windows(4).position(|x|x==b"\r\n\r\n").unwrap();let body:Value=serde_json::from_slice(&raw[at+4..]).unwrap();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();body
            });
            let mut p=configured(Gateway::RollyPay);p.base=format!("http://{addr}");
            let fallback=if channel=="bot" {"https://t.me/synthetic_test_bot"} else {"https://miniapp.example.test/app/"};
            let mut cfg=p.cfg.snapshot();cfg.insert("return_url".into(),json!(fallback));p.cfg.replace(cfg);
            let req=InvoiceRequest{payment_id:payment,client_id:client,telegram_id:if channel=="web"{None}else{Some(424242)},amount_minor:1999,currency:"RUB".into(),description:"Combined synthetic renewal".into(),return_url:if channel=="web"{Some(web_return.clone())}else{None}};
            let invoice=p.create_invoice(&req).await.unwrap();assert_eq!(invoice.external_id.as_deref(),Some(tag.as_str()));
            let captured=server.await.unwrap();let expected_return=if channel=="web"{web_return.as_str()}else{fallback};
            assert_eq!(captured["amount"],"19.99");assert_eq!(captured["order_id"],format!("sn-{payment}"));
            for field in ["success_redirect_url","fail_redirect_url"]{assert_eq!(captured[field],expected_return);}
            sqlx::query("UPDATE payments SET provider_txid=$2,pay_url=$3 WHERE id=$1").bind(payment).bind(&tag).bind(&invoice.pay_url).execute(&pool).await.unwrap();
            let mut notification=notification(Gateway::RollyPay);notification["payment_id"]=json!(tag);notification["order_id"]=json!(format!("sn-{payment}"));notification["amount"]=json!("19.99000000");
            let bytes=serde_json::to_vec(&notification).unwrap();let headers=signed_headers(Gateway::RollyPay,&notification,&bytes);
            let url=format!("{api}/api/pay/webhook/rollypay");
            let send=|data:Vec<u8>,signed:HashMap<String,String>| {let mut r=http.post(&url).header("Content-Type","application/json").body(data);for (k,v) in signed {r=r.header(k,v);}r.send()};
            let bad=send(bytes.clone(),HashMap::new()).await.unwrap();assert!(!bad.status().is_success());
            let mut wrong=notification.clone();wrong["amount"]=json!("19.98000000");let wrong_bytes=serde_json::to_vec(&wrong).unwrap();
            assert!(!send(wrong_bytes.clone(),signed_headers(Gateway::RollyPay,&wrong,&wrong_bytes)).await.unwrap().status().is_success());
            let unchanged:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await.unwrap();assert_eq!(before,unchanged);
            let first=send(bytes.clone(),headers.clone()).await.unwrap();assert!(first.status().is_success(),"valid exact amount rejected: {}",first.status());assert_eq!(first.json::<Value>().await.unwrap()["ok"],true);
            let once:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await.unwrap();assert_eq!(once,before+chrono::Duration::days(30));
            let (a,b,c)=tokio::join!(send(bytes.clone(),headers.clone()),send(bytes.clone(),headers.clone()),send(bytes.clone(),headers.clone()));
            for response in [a,b,c]{assert!(response.unwrap().status().is_success());}
            let after:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("SELECT expires_at FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await.unwrap();assert_eq!(after,once);
            let rows:Vec<(i64,String,i64)>=sqlx::query_as("SELECT id,status::text,amount_minor FROM payments WHERE client_id=$1 ORDER BY id").bind(client).fetch_all(&pool).await.unwrap();assert_eq!(rows,vec![(payment,"success".to_string(),1999)]);
            proofs.push(json!({"channel":channel,"client_id":client,"payment_id":payment,"request_amount":captured["amount"],"notification_amount":"19.99000000","return_url":expected_return,"before":before,"after_first":once,"after_three_concurrent_repeats":after,"payment_rows":1,"signature_and_wrong_amount_rejected":true}));
        }
        std::fs::write(fixture["proof_file"].as_str().unwrap(),serde_json::to_vec_pretty(&json!({"cases":proofs,"provider_transport":"loopback Hosted create_invoice mock","notification_transport":"real HTTP API webhook","invoice_database_setup":"synthetic pending row; checkout API creation itself not exercised"})).unwrap()).unwrap();
        println!("COMBINED FLOW COMPLETE: web override, bot/Mini App fallback, local invoice create, signed real HTTP notification, 30 days once, 3 concurrent repeats, one invoice per account");
    }
}

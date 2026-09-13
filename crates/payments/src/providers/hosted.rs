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
        let return_url=self.cfg.get("return_url");
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
}

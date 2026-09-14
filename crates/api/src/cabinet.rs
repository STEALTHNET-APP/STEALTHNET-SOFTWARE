//! Standalone customer surface. An installation key never selects a customer.
use axum::{extract::{State, Path, FromRequestParts}, http::{HeaderMap, request::Parts}, routing::{get,post}, Extension, Json, Router};
use serde::Deserialize;
use serde_json::{json,Value};
use sqlx::Row;
use sn_core::{Error,Result,auth::{generate_token,token_hash},cabinet::{generate_code,code_hash}};
use crate::state::{AppState,CurrentAdmin};

#[derive(Clone)] pub struct CabinetChannel;
pub struct Customer { pub id:i64, pub cabinet:bool, pub saved:bool, pub installation:Option<uuid::Uuid> }
impl FromRequestParts<AppState> for Customer {
    type Rejection=Error;
    async fn from_request_parts(parts:&mut Parts,st:&AppState)->Result<Self>{
        if parts.extensions.get::<CabinetChannel>().is_none(){
            return Ok(Self{id:crate::miniapp::who(st,&parts.headers).await?,cabinet:false,saved:true,installation:None});
        }
        let (installation,_)=installation(st,&parts.headers,parts.method.as_str()).await?;
        let token=bearer(&parts.headers)?;
        let row=sqlx::query("SELECT s.client_id,k.saved_at IS NOT NULL AS saved,s.csrf_hash FROM cabinet_sessions s JOIN clients c ON c.id=s.client_id JOIN cabinet_credentials k ON k.client_id=c.id WHERE s.token_hash=$1 AND s.installation_id=$2 AND s.expires_at>now() AND s.last_seen_at>now()-interval '7 days' AND c.deleted_at IS NULL")
            .bind(token_hash(token)).bind(installation).fetch_optional(&st.pool).await?.ok_or(Error::Unauthorized)?;
        if !matches!(parts.method.as_str(),"GET"|"HEAD") && row.get::<Vec<u8>,_>("csrf_hash")!=token_hash(header(&parts.headers,"x-csrf-token")?){return Err(Error::Forbidden);}
        sqlx::query("UPDATE cabinet_sessions SET last_seen_at=now() WHERE token_hash=$1 AND last_seen_at<now()-interval '1 minute'").bind(token_hash(token)).execute(&st.pool).await?;
        Ok(Self{id:row.get("client_id"),saved:row.get("saved"),cabinet:true,installation:Some(installation)})
    }
}
impl Customer {pub fn can_buy(&self)->Result<()>{if self.cabinet&&!self.saved {Err(Error::bad("Сначала сохраните код доступа"))}else{Ok(())}}}
fn header<'a>(h:&'a HeaderMap,name:&str)->Result<&'a str>{h.get(name).and_then(|s|s.to_str().ok()).ok_or(Error::Unauthorized)}
fn bearer(h:&HeaderMap)->Result<&str>{header(h,"authorization")?.strip_prefix("Bearer ").filter(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or(Error::Unauthorized)}
pub async fn settings(st:&AppState)->Result<Value>{Ok(sqlx::query_scalar("SELECT value FROM settings WHERE key='cabinet.config'").fetch_one(&st.pool).await?)}
/// Prefill the admin form only. Public settings change when the owner saves it.
fn starter_config(mut config:Value,panel:&str,brand:&str)->Value{
    fn fill(value:&mut Value,defaults:Value){
        for (key,text) in defaults.as_object().unwrap(){
            if value.get(key).is_none_or(|v|v.is_null()||v.as_str().is_some_and(|s|s.trim().is_empty())){
                value[key]=text.clone();
            }
        }
    }
    let name=config["brand"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or(brand).to_owned();
    let name=if name.trim().is_empty(){"VPN".to_owned()}else{name};
    let logo=format!("{}/customer-brand/starter.svg",panel.trim_end_matches('/'));
    fill(&mut config,json!({
        "brand":name,"logo":logo,"logo_dark":logo,"favicon":logo,
        "accent":"#513bfa","accent_end":"#0bc4db",
        "headline":"Ваш VPN — в одном кабинете",
        "description":"Выберите тариф, сохраните код доступа и подключите свои устройства. Здесь можно проверить срок подписки и управлять подключением.",
        "tariffs_heading":"Выберите подходящий тариф","faq_heading":"Вопросы и ответы",
        "seo_title":format!("{name} — VPN и личный кабинет"),
        "seo_description":"Тарифы VPN, подключение устройств и управление подпиской в личном кабинете."
    }));
    if config.get("locales").is_none_or(Value::is_null){config["locales"]=json!({});}
    if config["locales"].is_object(){
        if config["locales"].get("en").is_none_or(Value::is_null){config["locales"]["en"]=json!({});}
        if config["locales"]["en"].is_object(){
            fill(&mut config["locales"]["en"],json!({
                "headline":"Your VPN, in one account",
                "description":"Choose a plan, save your access code, and connect your devices. Check your subscription expiry and manage your connection here.",
                "tariffs_heading":"Choose your plan","faq_heading":"Questions and answers",
                "seo_title":format!("{name} — VPN and customer portal"),
                "seo_description":"VPN plans, device setup, and subscription management in your customer account."
            }));
        }
    }
    if config["support_url"].as_str().is_some_and(|s|!s.is_empty()){
        fill(&mut config,json!({"support_label":"Связаться с поддержкой"}));
        if config["locales"]["en"].is_object(){fill(&mut config["locales"]["en"],json!({"support_label":"Contact support"}));}
    }
    config
}
fn flag(v:&Value,k:&str)->bool{v[k].as_bool()==Some(true)}
pub fn validate_config(v:&Value)->Result<()>{
    if !v.is_object(){return Err(Error::bad("Настройки кабинета должны быть объектом"));}
    let text_fields=["brand","headline","description","logo","logo_dark","favicon","seo_title","seo_description","og_image","support_url","support_label","support_text","bot","accent","accent_end","miniapp_installation_id","tariffs_heading","faq_heading","docs_text"];
    let flags=["enabled","registration_enabled","shop_enabled","devices_enabled","referral_enabled","tickets_enabled","indexable"];
    for k in v.as_object().unwrap().keys(){if !text_fields.contains(&k.as_str())&&!flags.contains(&k.as_str())&&! ["faq","docs_links","steps","locales"].contains(&k.as_str()){return Err(Error::bad(format!("Неизвестная настройка: {k}")));}}
    for k in flags {if v.get(k).is_some_and(|x|!x.is_boolean()){return Err(Error::bad(format!("{k}: нужен переключатель")));}}
    if let Some(id)=v["miniapp_installation_id"].as_str().filter(|s|!s.is_empty()){uuid::Uuid::parse_str(id).map_err(|_|Error::bad("Выберите установленный кабинет для Mini App"))?;}
    if let Some(rows)=v.get("steps"){let rows=rows.as_array().ok_or_else(||Error::bad("Шаги подключения должны быть списком"))?;if rows.len()>6||rows.iter().any(|r|r["title"].as_str().is_none_or(|s|s.trim().is_empty()||s.len()>180)||r["text"].as_str().is_none_or(|s|s.len()>800)){return Err(Error::bad("Проверьте шаги подключения"));}}
    if v["support_url"].as_str().is_some_and(|s|!s.is_empty())&&v["support_label"].as_str().is_none_or(|s|s.trim().is_empty()){return Err(Error::bad("Задайте подпись ссылки поддержки"));}
    if v["bot"].as_str().is_some_and(|s|!s.is_empty()&&(s.len()>64||!s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'))){return Err(Error::bad("Имя бота задаётся без @ и ссылки"));}
    for k in text_fields {if v.get(k).is_some_and(|s|!s.is_string()||s.as_str().unwrap().len()>8000){return Err(Error::bad(format!("Некорректное поле: {k}")));}}
    for k in ["logo","logo_dark","favicon","og_image","support_url"] {if let Some(s)=v[k].as_str().filter(|s|!s.is_empty()){let u=reqwest::Url::parse(s).map_err(|_|Error::bad(format!("{k}: нужен HTTPS-адрес")))?;if u.scheme()!="https"||!u.username().is_empty()||u.password().is_some(){return Err(Error::bad(format!("{k}: нужен HTTPS-адрес без пароля")));}}}
    for k in ["accent","accent_end"] {if let Some(s)=v[k].as_str().filter(|s|!s.is_empty()){if s.len()!=7||!s.starts_with('#')||!s[1..].bytes().all(|b|b.is_ascii_hexdigit()){return Err(Error::bad("Цвет задаётся в формате #RRGGBB"));}}}
    if let Some(a)=v.get("faq"){let a=a.as_array().ok_or_else(||Error::bad("FAQ должен быть списком"))?;if a.len()>30||a.iter().any(|r|r["question"].as_str().is_none_or(|s|s.len()>500)||r["answer"].as_str().is_none_or(|s|s.len()>6000)){return Err(Error::bad("Проверьте вопросы и ответы FAQ"));}}
    if let Some(a)=v.get("docs_links"){let a=a.as_array().ok_or_else(||Error::bad("Документы должны быть списком"))?;if a.len()>20{return Err(Error::bad("Не более 20 документов"));}for r in a {let u=r["url"].as_str().and_then(|s|reqwest::Url::parse(s).ok()).ok_or_else(||Error::bad("У документа нужен HTTPS-адрес"))?;if u.scheme()!="https"||!u.username().is_empty()||u.password().is_some()||r["title"].as_str().is_none_or(|s|s.trim().is_empty()||s.len()>200){return Err(Error::bad("Проверьте название и ссылку документа"));}}}
    if flag(v,"enabled"){for k in ["brand","headline","description","logo","favicon","seo_title","seo_description","accent","accent_end","tariffs_heading"]{if v[k].as_str().is_none_or(|s|s.trim().is_empty()){return Err(Error::bad(format!("Перед публикацией заполните: {k}")));}}}
    if let Some(locales)=v.get("locales") {
        let locales=locales.as_object().ok_or_else(||Error::bad("Переводы должны быть объектом"))?;
        if locales.keys().any(|k|k!="en"){return Err(Error::bad("Поддерживаются русский и английский языки"));}
        if let Some(en)=locales.get("en") {
            let fields=["headline","description","tariffs_heading","faq_heading","seo_title","seo_description","support_label","support_text","docs_text","steps","faq","docs_links"];
            let obj=en.as_object().ok_or_else(||Error::bad("Английский перевод должен быть объектом"))?;
            if obj.keys().any(|k|!fields.contains(&k.as_str())){return Err(Error::bad("Перевод может содержать только тексты и документы"));}
            validate_config(en)?;
            let has_content=obj.values().any(|x|x.as_str().is_some_and(|x|!x.trim().is_empty())||x.as_array().is_some_and(|x|!x.is_empty()));
            if has_content {
                for k in ["headline","description","tariffs_heading","seo_title","seo_description"] {
                    if en[k].as_str().is_none_or(|x|x.trim().is_empty()){return Err(Error::bad(format!("Заполните английский перевод: {k}")));}
                }
                if v["support_url"].as_str().is_some_and(|x|!x.is_empty())&&en["support_label"].as_str().is_none_or(|x|x.trim().is_empty()){return Err(Error::bad("Задайте английскую подпись ссылки поддержки"));}
            }
        }
    }
    Ok(())
}
pub(crate) async fn installation(st:&AppState,h:&HeaderMap,method:&str)->Result<(uuid::Uuid,String)>{
    let token=header(h,"x-cabinet-service-key")?;
    if token.len()!=64{return Err(Error::Unauthorized);}
    let row:Option<(uuid::Uuid,String)>=sqlx::query_as("SELECT id,public_url FROM cabinet_installations WHERE token_hash=$1 AND revoked_at IS NULL").bind(token_hash(token)).fetch_optional(&st.pool).await?;
    let (id,origin)=row.ok_or(Error::Unauthorized)?;
    if !matches!(method,"GET"|"HEAD") && header(h,"origin")?!=origin{return Err(Error::Forbidden);}
    if !flag(&settings(st).await?,"enabled"){return Err(Error::bad("Сайт ещё не опубликован"));}
    sqlx::query("UPDATE cabinet_installations SET last_seen_at=now() WHERE id=$1 AND (last_seen_at IS NULL OR last_seen_at<now()-interval '1 minute')").bind(id).execute(&st.pool).await?;
    Ok((id,origin))
}
async fn throttle(st:&AppState,h:&HeaderMap,install:uuid::Uuid,action:&str,max:i32,seconds:i64)->Result<()>{
    let ip=header(h,"x-cabinet-client-ip")?.parse::<std::net::IpAddr>().map_err(|_|Error::Unauthorized)?;
    let key=hex::encode(token_hash(&format!("{install}:{ip}:{action}")));
    let window=chrono::Utc::now().timestamp()/seconds*seconds;
    let count:i32=sqlx::query_scalar("INSERT INTO cabinet_auth_limits(bucket,window_start,hits) VALUES($1,$2,1) ON CONFLICT(bucket,window_start) DO UPDATE SET hits=cabinet_auth_limits.hits+1 RETURNING hits").bind(key).bind(window).fetch_one(&st.pool).await?;
    // All windows are bounded to <= 1 hour. Keep two days, not an unbounded log.
    sqlx::query("DELETE FROM cabinet_auth_limits WHERE window_start<$1").bind(chrono::Utc::now().timestamp()-172800).execute(&st.pool).await?;
    if count>max{return Err(Error::TooManyRequests);}Ok(())
}
fn csrf(token:&str)->String{hex::encode(token_hash(&format!("cabinet-csrf:{token}")))}
async fn session(tx:&mut sqlx::Transaction<'_,sqlx::Postgres>,client:i64,install:uuid::Uuid,saved:bool)->Result<Value>{
    let token=generate_token();let csrf=csrf(&token);
    sqlx::query("INSERT INTO cabinet_sessions(token_hash,csrf_hash,client_id,installation_id,expires_at) VALUES($1,$2,$3,$4,now()+CASE WHEN $5 THEN interval '30 days' ELSE interval '15 minutes' END)")
        .bind(token_hash(&token)).bind(token_hash(&csrf)).bind(client).bind(install).bind(saved).execute(&mut **tx).await?;
    Ok(json!({"session_token":token,"csrf":csrf,"code_saved":saved}))
}
pub fn routes()->Router<AppState>{
    let private=Router::new().route("/api/cabinet/auth/session",get(session_info))
        .route("/api/cabinet/auth/code-saved",post(saved))
        .route("/api/cabinet/auth/rotate",post(rotate))
        .route("/api/cabinet/auth/code",post(reveal))
        .route("/api/cabinet/auth/code-remember",post(remember))
        .route("/api/cabinet/auth/logout",post(logout))
        .route("/api/cabinet/auth/logout-all",post(logout_all))
        .route("/api/cabinet/auth/telegram",post(link_telegram))
        .route_layer(Extension(CabinetChannel));
    Router::new().merge(private).merge(crate::miniapp::cabinet_client_routes())
        .route("/api/clients/{id}/cabinet-code",get(admin_code_status).post(admin_code_reveal))
        .route("/api/clients/{id}/cabinet-code/reset",post(admin_code_reset))
        .route("/api/cabinet/config",get(config))
        .route("/api/cabinet/catalog",get(catalog))
        .route("/api/cabinet/auth/register",post(register))
        .route("/api/cabinet/auth/login",post(login))
        .route("/api/cabinet-service",get(admin_status).patch(admin_config))
        .route("/api/cabinet-service/installations",post(create_installation))
        .route("/api/cabinet-service/installations/{id}/token",post(install_token))
        .route("/api/cabinet-service/installations/{id}/revoke",post(revoke_installation))
}
async fn config(State(st):State<AppState>,h:HeaderMap)->Result<Json<Value>>{
    installation(&st,&h,"GET").await?;
    let mut conf=settings(&st).await?;
    conf["free_access_available"]=json!(crate::miniapp::free_access_available(&st).await?);
    Ok(Json(conf))
}
async fn catalog(State(st):State<AppState>,h:HeaderMap)->Result<Json<Value>>{
    installation(&st,&h,"GET").await?;
    let paid=flag(&settings(&st).await?,"shop_enabled");
    crate::miniapp::tariff_catalog(&st,None,paid).await.map(Json)
}
#[derive(Deserialize)]struct Register{request_key:String}
async fn register(State(st):State<AppState>,h:HeaderMap,Json(b):Json<Register>)->Result<Json<Value>>{
    let (install,_)=installation(&st,&h,"POST").await?;
    if !flag(&settings(&st).await?,"registration_enabled"){return Err(Error::bad("Регистрация отключена"));}
    if b.request_key.len()!=64||!b.request_key.bytes().all(|b|b.is_ascii_hexdigit()){return Err(Error::bad("Обновите страницу регистрации"));}
    throttle(&st,&h,install,"register",6,3600).await?;
    let key=token_hash(&b.request_key);let mut tx=st.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(i64::from_be_bytes(key[..8].try_into().unwrap())).execute(&mut *tx).await?;
    let existing:Option<(i64,bool)>=sqlx::query_as("SELECT client_id,saved_at IS NOT NULL FROM cabinet_credentials WHERE registration_hash=$1").bind(&key).fetch_optional(&mut *tx).await?;
    let client=if let Some((id,saved))=existing{if saved{return Err(Error::bad("Аккаунт уже создан. Войдите по сохранённому коду"));}id}else{
        let id:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id,status) VALUES($1,$2,'expired') RETURNING id").bind(format!("client-{}",&generate_token()[..8])).bind(uuid::Uuid::new_v4().simple().to_string()).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO subscriptions(client_id,expires_at,device_limit) VALUES($1,NULL,1)").bind(id).execute(&mut *tx).await?;id
    };
    let code=generate_code();sqlx::query("INSERT INTO cabinet_credentials(client_id,code_hash,registration_hash,code_sealed) VALUES($1,$2,$3,$4) ON CONFLICT(client_id) DO UPDATE SET code_hash=EXCLUDED.code_hash,code_sealed=EXCLUDED.code_sealed,rotated_at=now()")
        .bind(client).bind(code_hash(&code).unwrap()).bind(key).bind(sn_core::cabinet::seal_code(&code)?).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM cabinet_sessions WHERE client_id=$1").bind(client).execute(&mut *tx).await?;
    let mut out=session(&mut tx,client,install,false).await?;out["code"]=json!(code);tx.commit().await?;Ok(Json(out))
}
#[derive(Deserialize)]struct Login{code:String}
async fn login(State(st):State<AppState>,h:HeaderMap,Json(b):Json<Login>)->Result<Json<Value>>{
    let (install,_)=installation(&st,&h,"POST").await?;throttle(&st,&h,install,"login",25,900).await?;
    let hash=code_hash(&b.code).ok_or(Error::Unauthorized)?;let mut tx=st.pool.begin().await?;
    let row:Option<(i64,bool)>=sqlx::query_as("SELECT k.client_id,k.saved_at IS NOT NULL FROM cabinet_credentials k JOIN clients c ON c.id=k.client_id WHERE k.code_hash=$1 AND c.deleted_at IS NULL FOR UPDATE OF k")
        .bind(hash).fetch_optional(&mut *tx).await?;
    let (client,saved)=row.ok_or(Error::Unauthorized)?;
    sqlx::query("UPDATE cabinet_credentials SET code_sealed=$2 WHERE client_id=$1 AND code_sealed IS NULL").bind(client).bind(sn_core::cabinet::seal_code(&b.code)?).execute(&mut *tx).await?;
    let out=session(&mut tx,client,install,saved).await?;tx.commit().await?;Ok(Json(out))
}
async fn session_info(State(st):State<AppState>,c:Customer,h:HeaderMap)->Result<Json<Value>>{let _=st;Ok(Json(json!({"csrf":csrf(bearer(&h)?),"code_saved":c.saved})))}
async fn saved(State(st):State<AppState>,c:Customer)->Result<Json<Value>>{
    let mut tx=st.pool.begin().await?;
    sqlx::query("UPDATE cabinet_credentials SET saved_at=COALESCE(saved_at,now()) WHERE client_id=$1").bind(c.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE cabinet_sessions SET expires_at=created_at+interval '30 days' WHERE client_id=$1").bind(c.id).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Json(json!({"ok":true})))
}
async fn reveal(State(st):State<AppState>,c:Customer)->Result<Json<Value>>{
    Ok(Json(json!({"code":sn_core::cabinet::reveal_code(&st.pool,c.id).await?})))
}
async fn remember(State(st):State<AppState>,c:Customer,h:HeaderMap,Json(b):Json<Login>)->Result<Json<Value>>{
    throttle(&st,&h,c.installation.unwrap(),"remember",10,900).await?;
    let hash=code_hash(&b.code).ok_or(Error::Unauthorized)?;
    let sealed=sn_core::cabinet::seal_code(&b.code)?;
    let r=sqlx::query("UPDATE cabinet_credentials SET code_sealed=$3 WHERE client_id=$1 AND code_hash=$2").bind(c.id).bind(hash).bind(sealed).execute(&st.pool).await?;
    if r.rows_affected()!=1{return Err(Error::Unauthorized);}
    Ok(Json(json!({"code":sn_core::cabinet::reveal_code(&st.pool,c.id).await?})))
}
async fn rotate(State(st):State<AppState>,c:Customer,Json(b):Json<Login>)->Result<Json<Value>>{
    let mut tx=st.pool.begin().await?;
    let current:Vec<u8>=sqlx::query_scalar("SELECT code_hash FROM cabinet_credentials WHERE client_id=$1 FOR UPDATE").bind(c.id).fetch_one(&mut *tx).await?;
    if c.saved&&code_hash(&b.code).as_ref()!=Some(&current){return Err(Error::Unauthorized);}
    let code=generate_code();sqlx::query("UPDATE cabinet_credentials SET code_hash=$2,code_sealed=$3,saved_at=NULL,registration_hash=NULL,rotated_at=now() WHERE client_id=$1").bind(c.id).bind(code_hash(&code).unwrap()).bind(sn_core::cabinet::seal_code(&code)?).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM cabinet_sessions WHERE client_id=$1").bind(c.id).execute(&mut *tx).await?;
    let mut out=session(&mut tx,c.id,c.installation.unwrap(),false).await?;out["code"]=json!(code);tx.commit().await?;Ok(Json(out))
}
async fn logout(State(st):State<AppState>,_c:Customer,h:HeaderMap)->Result<Json<Value>>{sqlx::query("DELETE FROM cabinet_sessions WHERE token_hash=$1").bind(token_hash(bearer(&h)?)).execute(&st.pool).await?;Ok(Json(json!({"logged_out":true})))}
async fn logout_all(State(st):State<AppState>,c:Customer)->Result<Json<Value>>{sqlx::query("DELETE FROM cabinet_sessions WHERE client_id=$1").bind(c.id).execute(&st.pool).await?;Ok(Json(json!({"logged_out":true})))}
async fn link_telegram(State(st):State<AppState>,c:Customer)->Result<Json<Value>>{
    c.can_buy()?;let conf=settings(&st).await?;let bot=conf["bot"].as_str().filter(|s|!s.is_empty()&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_')).ok_or_else(||Error::bad("Бот для привязки не настроен"))?;
    let token=sn_core::cabinet::create_link(&st.pool,c.id,c.installation.unwrap()).await?;
    Ok(Json(json!({"url":format!("https://t.me/{bot}?start=cab_{token}")})))
}
async fn admin_status(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>>{
    let installs:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name,'public_url',public_url,'server_ip',server_ip,'placement',placement,'token_prefix',token_prefix,'last_seen_at',last_seen_at,'revoked_at',revoked_at) FROM cabinet_installations ORDER BY created_at DESC").fetch_all(&st.pool).await?;
    let panel=crate::sub_service::panel_public_url_pub(&st).await;
    let mut saved=settings(&st).await?;
    let selection_cleared=if let Some(id)=saved["miniapp_installation_id"].as_str().filter(|s|!s.is_empty()){
        !installs.iter().any(|i|i["id"].as_str()==Some(id)&&i["revoked_at"].is_null())
    }else{false};
    if selection_cleared{saved["miniapp_installation_id"]=json!("");}
    let config=starter_config(saved.clone(),&panel,&st.config.brand_name);
    Ok(Json(json!({"defaults_applied":config!=saved,"miniapp_selection_cleared":selection_cleared,"config":config,"installations":installs,"miniapp_url":sn_core::cabinet::miniapp_url(&st.pool).await?,"panel_url":panel})))
}
async fn admin_config(CurrentAdmin(a):CurrentAdmin,State(st):State<AppState>,Json(mut v):Json<Value>)->Result<Json<Value>>{
    if a.role!="owner"{return Err(Error::Forbidden);}validate_config(&v)?;
    let mut tx=st.pool.begin().await?;
    // Serialize with revocation: a stale browser must not restore a revoked selection.
    sqlx::query("SELECT key FROM settings WHERE key='cabinet.config' FOR UPDATE").fetch_one(&mut *tx).await?;
    let mut selection_cleared=false;
    if let Some(id)=v["miniapp_installation_id"].as_str().filter(|s|!s.is_empty()) {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinet_installations WHERE id::text=$1 AND revoked_at IS NULL)").bind(id).fetch_one(&mut *tx).await?;
        if !exists {v["miniapp_installation_id"]=json!("");selection_cleared=true;}
    }
    sqlx::query("UPDATE settings SET value=$1,updated_by=$2,updated_at=now() WHERE key='cabinet.config'").bind(&v).bind(a.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true,"miniapp_selection_cleared":selection_cleared,"miniapp_installation_id":v["miniapp_installation_id"].as_str().unwrap_or("")})))
}
#[derive(Deserialize)]struct Installation{name:String,public_url:String,server_ip:String,placement:String}
async fn create_installation(CurrentAdmin(a):CurrentAdmin,State(st):State<AppState>,Json(b):Json<Installation>)->Result<Json<Value>>{
    if a.role!="owner"{return Err(Error::Forbidden);}
    let u=reqwest::Url::parse(&b.public_url).map_err(|_|Error::bad("Нужен HTTPS-домен кабинета"))?;
    if u.scheme()!="https"||u.host_str().is_none()||u.host_str().unwrap()=="localhost"||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some()||u.path()!="/"||u.port().is_some()||b.name.trim().is_empty()||b.name.len()>100||!matches!(b.placement.as_str(),"same"|"separate"){return Err(Error::bad("Проверьте домен, название и размещение"));}
    b.server_ip.parse::<std::net::IpAddr>().map_err(|_|Error::bad("Укажите IP сервера"))?;
    for other in [crate::sub_service::panel_public_url_pub(&st).await,crate::sub_service::public_sub_url(&st).await]{if reqwest::Url::parse(&other).ok().is_some_and(|o|o.host_str()==u.host_str()){return Err(Error::bad("Домен кабинета должен отличаться от панели и подписки"));}}
    let id:uuid::Uuid=sqlx::query_scalar("INSERT INTO cabinet_installations(name,public_url,server_ip,placement) VALUES($1,$2,$3,$4) RETURNING id").bind(b.name.trim()).bind(b.public_url.trim_end_matches('/')).bind(b.server_ip).bind(b.placement).fetch_one(&st.pool).await?;
    Ok(Json(json!({"id":id})))
}
async fn install_token(CurrentAdmin(a):CurrentAdmin,State(st):State<AppState>,Path(id):Path<uuid::Uuid>)->Result<Json<Value>>{
    if a.role!="owner"{return Err(Error::Forbidden);}let token=generate_token();
    let r=sqlx::query("UPDATE cabinet_installations SET token_hash=$2,token_prefix=$3,last_seen_at=CASE WHEN revoked_at IS NOT NULL THEN NULL ELSE last_seen_at END,revoked_at=NULL WHERE id=$1").bind(id).bind(token_hash(&token)).bind(&token[..8]).execute(&st.pool).await?;if r.rows_affected()==0{return Err(Error::NotFound);}
    let origin:String=sqlx::query_scalar("SELECT public_url FROM cabinet_installations WHERE id=$1").bind(id).fetch_one(&st.pool).await?;
    let panel=crate::sub_service::panel_public_url_pub(&st).await;
    let quote=|v:&str|format!("'{}'",v.replace('\'',"'\"'\"'"));
    let command=format!("env PANEL_API_URL={} CABINET_PUBLIC_URL={} CABINET_SERVICE_KEY={} bash -c 'set -euo pipefail; command -v curl >/dev/null || {{ apt-get update -qq; apt-get install -y curl ca-certificates; }}; curl -fsS \"$PANEL_API_URL/install-cabinet.sh\" | bash'",quote(&panel),quote(&origin),quote(&token));
    Ok(Json(json!({"token":token,"command":command})))
}
async fn revoke_installation(CurrentAdmin(a):CurrentAdmin,State(st):State<AppState>,Path(id):Path<uuid::Uuid>)->Result<Json<Value>>{
    if a.role!="owner"{return Err(Error::Forbidden);}
    let mut tx=st.pool.begin().await?;
    sqlx::query("SELECT key FROM settings WHERE key='cabinet.config' FOR UPDATE").fetch_one(&mut *tx).await?;
    let r=sqlx::query("UPDATE cabinet_installations SET revoked_at=now(),token_hash=NULL WHERE id=$1").bind(id).execute(&mut *tx).await?;
    if r.rows_affected()==0{return Err(Error::NotFound);}
    sqlx::query("UPDATE settings SET value=jsonb_set(value,'{miniapp_installation_id}','\"\"'::jsonb),updated_by=$2,updated_at=now() WHERE key='cabinet.config' AND value->>'miniapp_installation_id'=$1").bind(id.to_string()).bind(a.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

pub(crate) async fn miniapp_gate(State(st):State<AppState>, request:axum::extract::Request, next:axum::middleware::Next)->Result<axum::response::Response>{
    let (id,_)=installation(&st,request.headers(),request.method().as_str()).await?;
    let conf=settings(&st).await?;
    if conf["miniapp_installation_id"].as_str()!=Some(id.to_string().as_str()) || !bc_miniapp_enabled(&st).await? {return Err(Error::Forbidden);}
    Ok(next.run(request).await)
}
async fn bc_miniapp_enabled(st:&AppState)->Result<bool>{
    Ok(sqlx::query_scalar::<_,Value>("SELECT value FROM settings WHERE key='bot.miniapp_enabled'").fetch_optional(&st.pool).await?==Some(json!(true)))
}

impl Customer {
    pub async fn return_url(&self,st:&AppState)->Result<Option<String>>{
        if !self.cabinet{return Ok(None);}
        Ok(sqlx::query_scalar::<_,String>("SELECT public_url || '/#payments' FROM cabinet_installations WHERE id=$1 AND revoked_at IS NULL").bind(self.installation).fetch_optional(&st.pool).await?)
    }
}

// Code management is restricted to interactive owners/admins, not support or API integrations.
async fn code_admin(st:&AppState,admin:&sn_core::auth::Admin,client:i64)->Result<()>{
    if !matches!(admin.role.as_str(),"owner"|"admin"){return Err(Error::Forbidden);}
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clients WHERE id=$1 AND deleted_at IS NULL)").bind(client).fetch_one(&st.pool).await?;
    if !exists{return Err(Error::NotFound);}Ok(())
}
async fn admin_code_status(State(st):State<AppState>,CurrentAdmin(a):CurrentAdmin,Path(id):Path<i64>)->Result<Json<Value>>{
    code_admin(&st,&a,id).await?;
    let data:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('issued',true,'viewable',code_sealed IS NOT NULL,'updated_at',rotated_at) FROM cabinet_credentials WHERE client_id=$1").bind(id).fetch_optional(&st.pool).await?;
    Ok(Json(data.unwrap_or(json!({"issued":false,"viewable":false}))))
}
async fn admin_code_reveal(State(st):State<AppState>,CurrentAdmin(a):CurrentAdmin,Path(id):Path<i64>)->Result<Json<Value>>{
    code_admin(&st,&a,id).await?;
    Ok(Json(json!({"code":sn_core::cabinet::reveal_code_as(&st.pool,id,"admin",a.id).await?})))
}
#[derive(Deserialize)]struct CodeReset {confirm:bool}
async fn admin_code_reset(State(st):State<AppState>,CurrentAdmin(a):CurrentAdmin,Path(id):Path<i64>,Json(b):Json<CodeReset>)->Result<Json<Value>>{
    code_admin(&st,&a,id).await?;if !b.confirm{return Err(Error::bad("Подтвердите сброс кода доступа"));}
    Ok(Json(json!({"code":sn_core::cabinet::issue_code_as(&st.pool,id,true,"admin",a.id).await?})))
}

#[cfg(test)]
mod language_tests {
    use super::*;
    #[test]fn starter_content_can_be_published_in_both_languages(){
        let saved=json!({"enabled":false,"registration_enabled":false,"shop_enabled":false,"faq":[],"docs_links":[]});
        let mut config=starter_config(saved.clone(),"https://panel.example.com/","Example VPN");
        for key in ["enabled","registration_enabled","shop_enabled","faq","docs_links"]{assert_eq!(config[key],saved[key]);}
        assert_eq!(config["logo"],"https://panel.example.com/customer-brand/starter.svg");
        assert_eq!(config["brand"],"Example VPN");
        config["enabled"]=json!(true);
        assert!(validate_config(&config).is_ok());
        assert!(config["locales"]["en"]["headline"].as_str().is_some_and(|s|!s.is_empty()));
        assert_eq!(starter_config(config.clone(),"https://other.example.com","Other"),config);
    }
    #[test]fn starter_content_preserves_custom_settings_and_fills_only_blanks(){
        let saved=json!({"enabled":true,"brand":"My service","headline":"My headline","description":"  ","accent":"#123456","logo":"https://custom.example.com/logo.svg","support_url":"https://example.com/help","faq":[{"question":"Custom?","answer":"Yes"}],"locales":{"en":{"headline":"My English headline"}}});
        let config=starter_config(saved.clone(),"https://panel.example.com","Ignored");
        for key in ["enabled","brand","headline","accent","logo","support_url","faq"]{assert_eq!(config[key],saved[key]);}
        assert_eq!(config["locales"]["en"]["headline"],"My English headline");
        assert!(config["description"].as_str().is_some_and(|s|!s.trim().is_empty()));
        assert!(validate_config(&config).is_ok());
    }
    fn english()->Value{json!({"headline":"Connect","description":"VPN service","tariffs_heading":"Plans","seo_title":"VPN","seo_description":"VPN plans","steps":[{"title":"Choose","text":"Choose a plan"}],"faq":[],"docs_links":[]})}
    #[test]fn english_content_is_validated_without_changing_service_flags(){
        assert!(validate_config(&json!({"locales":{"en":english()}})).is_ok());
        let mut en=english();en["enabled"]=json!(false);assert!(validate_config(&json!({"locales":{"en":en}})).is_err());
        assert!(validate_config(&json!({"locales":{"en":{"locales":{"en":{}}}}})).is_err());
    }
    #[test]fn incomplete_published_translation_and_unsafe_links_are_rejected(){
        assert!(validate_config(&json!({"locales":{"en":{"headline":"Only a heading"}}})).is_err());
        let mut en=english();en["docs_links"]=json!([{"title":"Terms","url":"javascript:alert(1)"}]);
        assert!(validate_config(&json!({"locales":{"en":en}})).is_err());
        assert!(validate_config(&json!({"locales":{"fr":{}}})).is_err());
        assert!(validate_config(&json!({"locales":{"en":{}}})).is_ok());
    }
}

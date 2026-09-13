//! Настройка выдачи подписок: шаблоны, правила ответов, приложения.
//!
//! Логика та же, что у Remnawave, и она себя оправдывает: клиентских
//! приложений много, каждое хочет свой формат, а список их растёт быстрее
//! релизов панели. Поэтому соответствие «User-Agent → формат» лежит в
//! базе, а не в коде: новое приложение подключается правилом, без
//! пересборки и перезапуска.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn sub_admin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sub/templates", get(templates_list).post(template_create))
        .route(
            "/api/sub/templates/{id}",
            axum::routing::patch(template_update).delete(template_delete),
        )
        .route("/api/sub/rules", get(rules_list).post(rule_create))
        .route("/api/sub/rules-document", get(rules_document).post(rules_document_save))
        .route("/api/sub/template-preview", axum::routing::post(template_preview))
        .route(
            "/api/sub/rules/{id}",
            axum::routing::patch(rule_update).delete(rule_delete),
        )
        .route("/api/sub/page-apps", get(apps_list).post(app_create))
        .route(
            "/api/sub/page-apps/{id}",
            axum::routing::patch(app_update).delete(app_delete),
        )
        .route("/api/sub/preview", axum::routing::post(preview))
}

// ── Templates and response rules ─────────────────────────────────────
use sn_sub::policy::{self,Condition,ResponseHeader};

async fn templates_list(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>>{
    let rows=sqlx::query("SELECT t.*, (SELECT count(*) FROM response_rules WHERE template_id=t.id)+(SELECT count(*) FROM external_squads e WHERE template_id=t.id OR EXISTS(SELECT 1 FROM jsonb_each(e.template_overrides) x WHERE x.value=to_jsonb(t.id)))+(SELECT count(*) FROM hosts WHERE options->'xray_template_id'=to_jsonb(t.id)) AS usages FROM subscription_templates t ORDER BY code,is_default DESC,title,id").fetch_all(&st.pool).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("id"),"code":r.get::<String,_>("code"),"title":r.get::<String,_>("title"),"body":r.get::<String,_>("body"),"is_default":r.get::<bool,_>("is_default"),"usages":r.get::<i64,_>("usages"),"updated_at":r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339()})).collect::<Vec<_>>())))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateBody {code:Option<String>,title:Option<String>,body:Option<String>,is_default:Option<bool>}
fn check_template(code:&str,body:&str)->Result<()>{policy::validate_template(code,body).map_err(Error::bad)}
fn template_title(title:&str)->Result<&str>{let title=title.trim();if title.is_empty()||title.chars().count()>100{return Err(Error::bad("Название шаблона — от 1 до 100 символов"))}Ok(title)}
async fn template_create(_a:CurrentAdmin,State(st):State<AppState>,Json(b):Json<TemplateBody>)->Result<Json<Value>>{
    let code=b.code.as_deref().unwrap_or("");let body=b.body.as_deref().unwrap_or("");check_template(code,body)?;let title=template_title(b.title.as_deref().unwrap_or(code))?;
    let mut tx=st.pool.begin().await?;sqlx::query("SELECT pg_advisory_xact_lock(hashtext('subscription-templates'))").execute(&mut *tx).await?;
    if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM subscription_templates WHERE code=$1 AND title=$2)").bind(code).bind(title).fetch_one(&mut *tx).await?{return Err(Error::bad("Шаблон с таким названием и форматом уже есть"))}
    if b.is_default==Some(true){sqlx::query("UPDATE subscription_templates SET is_default=false WHERE code=$1").bind(code).execute(&mut *tx).await?;}
    let id:i64=sqlx::query_scalar("INSERT INTO subscription_templates(code,title,body,is_default) VALUES($1,$2,$3,$4) RETURNING id").bind(code).bind(title).bind(body).bind(b.is_default.unwrap_or(false)).fetch_one(&mut *tx).await?;tx.commit().await?;Ok(Json(json!({"id":id})))
}
async fn template_update(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>,Json(b):Json<TemplateBody>)->Result<Json<Value>>{
    let mut tx=st.pool.begin().await?;sqlx::query("SELECT pg_advisory_xact_lock(hashtext('subscription-templates'))").execute(&mut *tx).await?;
    let cur=sqlx::query("SELECT * FROM subscription_templates WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let code:String=cur.get("code");if b.code.as_deref().is_some_and(|c|c!=code){return Err(Error::bad("Формат существующего шаблона менять нельзя"))}
    let title=b.title.unwrap_or_else(||cur.get("title"));let title=template_title(&title)?;let body=b.body.unwrap_or_else(||cur.get("body"));check_template(&code,&body)?;
    if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM subscription_templates WHERE code=$1 AND title=$2 AND id<>$3)").bind(&code).bind(title).bind(id).fetch_one(&mut *tx).await?{return Err(Error::bad("Шаблон с таким названием и форматом уже есть"))}
    if b.is_default==Some(true){sqlx::query("UPDATE subscription_templates SET is_default=false WHERE code=$1").bind(&code).execute(&mut *tx).await?;}
    sqlx::query("UPDATE subscription_templates SET title=$2,body=$3,is_default=COALESCE($4,is_default),updated_at=now() WHERE id=$1").bind(id).bind(title).bind(body).bind(b.is_default).execute(&mut *tx).await?;tx.commit().await?;Ok(Json(json!({"ok":true})))
}
async fn template_delete(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>)->Result<Json<Value>>{
    let used:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM response_rules WHERE template_id=$1)+(SELECT count(*) FROM external_squads e WHERE template_id=$1 OR EXISTS(SELECT 1 FROM jsonb_each(e.template_overrides) x WHERE x.value=to_jsonb($1::bigint)))+(SELECT count(*) FROM hosts WHERE options->'xray_template_id'=to_jsonb($1::bigint))").bind(id).fetch_one(&st.pool).await?;
    if used>0{return Err(Error::bad("Шаблон используется: сначала измените назначения правил, хостов и внешних сквадов"))}
    let res=sqlx::query("DELETE FROM subscription_templates WHERE id=$1 AND NOT is_default").bind(id).execute(&st.pool).await?;
    if res.rows_affected()==0{return Err(Error::bad("Шаблон не найден или используется по умолчанию. Сначала выберите встроенный формат либо другой шаблон"))}Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct TemplatePreview {code:String,body:String}
async fn template_preview(_a:CurrentAdmin,Json(b):Json<TemplatePreview>)->Result<Json<Value>>{
    check_template(&b.code,&b.body)?;
    let mut host=sn_sub::formats::stub_host("Пример • Германия");host.address="de.example.com".into();host.port=443;
    let mut second=host.clone();second.remark="Пример • Нидерланды".into();second.address="nl.example.com".into();
    let rendered=policy::render_template(&b.code,&b.body,&[host,second],"Пример подписки").map_err(Error::bad)?;
    Ok(Json(json!({"valid":true,"formatted":policy::format_template(&b.code,&b.body).map_err(Error::bad)?,"rendered":rendered,"synthetic":true})))
}

const RULE_SELECT:&str="SELECT r.*,t.code AS template_code,t.title AS template_title FROM response_rules r LEFT JOIN subscription_templates t ON t.id=r.template_id ORDER BY r.sort_order,r.id";
fn rule_json(r:&sqlx::postgres::PgRow)->Value{json!({"id":r.get::<i64,_>("id"),"sort_order":r.get::<i32,_>("sort_order"),"name":r.get::<String,_>("name"),"ua_pattern":r.get::<String,_>("ua_pattern"),"action":r.get::<String,_>("action"),"is_active":r.get::<bool,_>("is_active"),"template_id":r.get::<Option<i64>,_>("template_id"),"template_code":r.get::<Option<String>,_>("template_code"),"template_title":r.get::<Option<String>,_>("template_title"),"conditions":r.get::<Option<Value>,_>("conditions"),"operator":r.get::<String,_>("operator"),"description":r.get::<String,_>("description"),"response_headers":r.get::<Value,_>("response_headers"),"disable_hwid_check":r.get::<bool,_>("disable_hwid_check")})}
async fn rules_list(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>>{Ok(Json(json!(sqlx::query(RULE_SELECT).fetch_all(&st.pool).await?.iter().map(rule_json).collect::<Vec<_>>()))) }
#[derive(Default,Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleBody{
    name:Option<String>,ua_pattern:Option<String>,action:Option<String>,sort_order:Option<i32>,is_active:Option<bool>,
    #[serde(default,deserialize_with="crate::state::patch_field")]template_id:Option<Option<i64>>,
    #[serde(default,deserialize_with="crate::state::patch_field")]conditions:Option<Option<Vec<Condition>>>,
    operator:Option<String>,description:Option<String>,response_headers:Option<Vec<ResponseHeader>>,disable_hwid_check:Option<bool>,
}
fn check_rule(action:&str,template:Option<i64>)->Result<()>{if !policy::ACTIONS.contains(&action){return Err(Error::bad("Неизвестный формат или действие правила"))}if action=="template"&&template.is_none(){return Err(Error::bad("Выберите шаблон"))}Ok(())}
async fn validate_rule(pool:&sqlx::PgPool,v:&Value)->Result<()>{
    let name=v["name"].as_str().unwrap_or("").trim();if name.is_empty()||name.chars().count()>100{return Err(Error::bad("Название правила — от 1 до 100 символов"))}
    let action=v["action"].as_str().unwrap_or("");let tid=v["template_id"].as_i64();check_rule(action,tid)?;
    if let Some(id)=tid{let code:String=sqlx::query_scalar("SELECT code FROM subscription_templates WHERE id=$1").bind(id).fetch_optional(pool).await?.ok_or_else(||Error::bad("Шаблон не найден"))?;if action!="template"&&action!=code{return Err(Error::bad("Формат правила не совпадает с форматом шаблона"))}}
    if v["conditions"].is_null(){if v["ua_pattern"].as_str().unwrap_or("").trim().is_empty(){return Err(Error::bad("Нужен образец User-Agent или список условий"))}}else{let c:Vec<Condition>=serde_json::from_value(v["conditions"].clone()).map_err(|_|Error::bad("Некорректные условия"))?;policy::validate_conditions(v["operator"].as_str().unwrap_or("AND"),&c).map_err(Error::bad)?;}
    let h:Vec<ResponseHeader>=serde_json::from_value(v["response_headers"].clone()).map_err(|_|Error::bad("Некорректные заголовки"))?;policy::validate_headers(&h).map_err(Error::bad)?;
    if v["description"].as_str().unwrap_or("").len()>2000{return Err(Error::bad("Описание слишком длинное"))}Ok(())
}
fn merge_rule(mut v:Value,b:RuleBody)->Value{
    for (k,x) in [("name",b.name),("ua_pattern",b.ua_pattern),("action",b.action),("operator",b.operator),("description",b.description)]{if let Some(x)=x{v[k]=json!(x.trim())}}
    if let Some(x)=b.template_id{v["template_id"]=json!(x)}if let Some(x)=b.conditions{v["conditions"]=json!(x)}if let Some(x)=b.sort_order{v["sort_order"]=json!(x)}if let Some(x)=b.is_active{v["is_active"]=json!(x)}if let Some(x)=b.response_headers{v["response_headers"]=json!(x)}if let Some(x)=b.disable_hwid_check{v["disable_hwid_check"]=json!(x)}v
}
async fn insert_rule(tx:&mut sqlx::Transaction<'_,sqlx::Postgres>,v:&Value)->Result<i64>{
    Ok(sqlx::query_scalar("INSERT INTO response_rules(name,ua_pattern,action,template_id,sort_order,is_active,conditions,operator,description,response_headers,disable_hwid_check) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id")
    .bind(v["name"].as_str().unwrap()).bind(v["ua_pattern"].as_str().unwrap_or("")).bind(v["action"].as_str().unwrap()).bind(v["template_id"].as_i64()).bind(v["sort_order"].as_i64().unwrap_or(0) as i32).bind(v["is_active"].as_bool().unwrap_or(true)).bind(if v["conditions"].is_null(){None}else{Some(v["conditions"].clone())}).bind(v["operator"].as_str().unwrap_or("AND")).bind(v["description"].as_str().unwrap_or("")).bind(&v["response_headers"]).bind(v["disable_hwid_check"].as_bool().unwrap_or(false)).fetch_one(&mut **tx).await?)
}
async fn rule_create(_a:CurrentAdmin,State(st):State<AppState>,Json(b):Json<RuleBody>)->Result<Json<Value>>{
    let order:i32=sqlx::query_scalar("SELECT COALESCE(max(sort_order),0)+10 FROM response_rules").fetch_one(&st.pool).await?;
    let v=merge_rule(json!({"name":b.ua_pattern.clone().unwrap_or_default(),"ua_pattern":"","action":"base64","template_id":null,"conditions":null,"operator":"AND","description":"","response_headers":[],"disable_hwid_check":false,"sort_order":order,"is_active":true}),b);validate_rule(&st.pool,&v).await?;
    let mut tx=st.pool.begin().await?;let id=insert_rule(&mut tx,&v).await?;tx.commit().await?;Ok(Json(json!({"id":id})))
}
async fn rule_update(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>,Json(b):Json<RuleBody>)->Result<Json<Value>>{
    let rows=sqlx::query(RULE_SELECT).fetch_all(&st.pool).await?;let cur=rows.iter().find(|r|r.get::<i64,_>("id")==id).ok_or(Error::NotFound)?;let v=merge_rule(rule_json(cur),b);validate_rule(&st.pool,&v).await?;
    sqlx::query("UPDATE response_rules SET name=$2,ua_pattern=$3,action=$4,template_id=$5,sort_order=$6,is_active=$7,conditions=$8,operator=$9,description=$10,response_headers=$11,disable_hwid_check=$12 WHERE id=$1")
    .bind(id).bind(v["name"].as_str().unwrap()).bind(v["ua_pattern"].as_str().unwrap_or("")).bind(v["action"].as_str().unwrap()).bind(v["template_id"].as_i64()).bind(v["sort_order"].as_i64().unwrap_or(0) as i32).bind(v["is_active"].as_bool().unwrap_or(true)).bind(if v["conditions"].is_null(){None}else{Some(v["conditions"].clone())}).bind(v["operator"].as_str().unwrap_or("AND")).bind(v["description"].as_str().unwrap_or("")).bind(&v["response_headers"]).bind(v["disable_hwid_check"].as_bool().unwrap_or(false)).execute(&st.pool).await?;
    Ok(Json(json!({"ok":true})))
}
async fn rule_delete(_a:CurrentAdmin,State(st):State<AppState>,Path(id):Path<i64>)->Result<Json<Value>>{let r=sqlx::query("DELETE FROM response_rules WHERE id=$1").bind(id).execute(&st.pool).await?;if r.rows_affected()==0{return Err(Error::NotFound)}Ok(Json(json!({"ok":true})))}
#[derive(Deserialize)]
struct PreviewBody {#[serde(default)]user_agent:String,#[serde(default)]headers:std::collections::HashMap<String,String>}
async fn preview(_a:CurrentAdmin,State(st):State<AppState>,Json(b):Json<PreviewBody>)->Result<Json<Value>>{
    let mut headers=axum::http::HeaderMap::new();
    for (k,v) in b.headers{headers.insert(axum::http::HeaderName::from_bytes(k.as_bytes()).map_err(|_|Error::bad("Некорректное имя заголовка"))?,v.parse().map_err(|_|Error::bad("Некорректное значение заголовка"))?);}
    if !b.user_agent.is_empty(){headers.insert("user-agent",b.user_agent.parse().map_err(|_|Error::bad("Некорректный User-Agent"))?);}
    let rows=sqlx::query(RULE_SELECT).fetch_all(&st.pool).await?;
    for row in rows{
        let r=rule_json(&row);if r["is_active"]!=true{continue}
        let hit=if r["conditions"].is_null(){let ua=headers.get("user-agent").and_then(|v|v.to_str().ok()).unwrap_or("").to_lowercase();r["ua_pattern"].as_str().unwrap_or("").split('|').map(|s|s.trim().to_lowercase()).filter(|s|!s.is_empty()).any(|s|ua.contains(&s))}else{policy::conditions_match(r["operator"].as_str().unwrap_or("AND"),&serde_json::from_value::<Vec<Condition>>(r["conditions"].clone()).unwrap_or_default(),&headers)};
        if hit{return Ok(Json(json!({"matched":true,"rule_id":r["id"],"rule_name":r["name"],"action":r["action"],"template_code":r["template_code"],"template_title":r["template_title"],"headers":r["response_headers"],"disable_hwid_check":r["disable_hwid_check"]})))}
    }Ok(Json(json!({"matched":false,"action":"auto"})))
}

#[derive(Deserialize,serde::Serialize,Default)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct DocumentMods {#[serde(default)]subscription_template:Option<String>,#[serde(default)]headers:Vec<ResponseHeader>,#[serde(default)]disable_hwid_check:bool}
#[derive(Deserialize,serde::Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct DocumentRule {name:String,#[serde(default)]description:String,enabled:bool,operator:String,conditions:Vec<Condition>,response_type:String,#[serde(default)]response_modifications:DocumentMods}
#[derive(Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields)]
struct RulesDocument {version:String,rules:Vec<DocumentRule>}
fn external_action(a:&str)->String{match a{"web_page"=>"BROWSER","base64"=>"XRAY_BASE64","not_found"=>"STATUS_CODE_404","unavailable"=>"STATUS_CODE_451",x=>x}.to_uppercase()}
fn internal_action(a:&str)->String{match a{"BROWSER"=>"web_page","XRAY_BASE64"=>"base64","STATUS_CODE_404"=>"not_found","STATUS_CODE_451"=>"unavailable",x=>x}.to_lowercase()}
async fn rules_document(_a:CurrentAdmin,State(st):State<AppState>)->Result<Json<Value>>{
    let rows=sqlx::query(RULE_SELECT).fetch_all(&st.pool).await?;
    let mut rules=Vec::new();
    for row in rows{let r=rule_json(&row);let legacy=r["conditions"].is_null();let conditions=if legacy{r["ua_pattern"].as_str().unwrap_or("").split('|').map(str::trim).filter(|s|!s.is_empty()).map(|s|Condition{header_name:"user-agent".into(),operator:"CONTAINS".into(),value:s.into(),case_sensitive:false}).collect()}else{serde_json::from_value(r["conditions"].clone()).unwrap_or_default()};
        rules.push(DocumentRule{name:r["name"].as_str().unwrap_or("").into(),description:r["description"].as_str().unwrap_or("").into(),enabled:r["is_active"].as_bool().unwrap_or(true),operator:if legacy{"OR".into()}else{r["operator"].as_str().unwrap_or("AND").into()},conditions,response_type:external_action(if r["action"]=="template"{r["template_code"].as_str().unwrap_or("base64")}else{r["action"].as_str().unwrap_or("base64")}),response_modifications:DocumentMods{subscription_template:r["template_title"].as_str().map(String::from),headers:serde_json::from_value(r["response_headers"].clone()).unwrap_or_default(),disable_hwid_check:r["disable_hwid_check"].as_bool().unwrap_or(false)}});
    }Ok(Json(json!(RulesDocument{version:"1".into(),rules})))
}
async fn rules_document_save(_a:CurrentAdmin,State(st):State<AppState>,Json(b):Json<RulesDocument>)->Result<Json<Value>>{
    if b.version!="1"||b.rules.len()>200{return Err(Error::bad("Ожидается версия 1 и не более 200 правил"))}
    let mut validated=Vec::new();
    for (i,r) in b.rules.into_iter().enumerate(){let action=internal_action(&r.response_type);let tid=if let Some(title)=&r.response_modifications.subscription_template{Some(sqlx::query_scalar::<_,i64>("SELECT id FROM subscription_templates WHERE title=$1 AND code=$2").bind(title).bind(&action).fetch_optional(&st.pool).await?.ok_or_else(||Error::bad(format!("Шаблон {title} ({action}) не найден")))?)}else{None};
        let v=json!({"name":r.name,"description":r.description,"is_active":r.enabled,"operator":r.operator,"conditions":r.conditions,"action":action,"template_id":tid,"response_headers":r.response_modifications.headers,"disable_hwid_check":r.response_modifications.disable_hwid_check,"sort_order":(i as i32+1)*10,"ua_pattern":""});validate_rule(&st.pool,&v).await?;validated.push(v);
    }
    let mut tx=st.pool.begin().await?;sqlx::query("LOCK TABLE response_rules IN EXCLUSIVE MODE").execute(&mut *tx).await?;sqlx::query("DELETE FROM response_rules").execute(&mut *tx).await?;for v in &validated{insert_rule(&mut tx,v).await?;}tx.commit().await?;Ok(Json(json!({"ok":true,"count":validated.len()})))
}

// ── Приложения на странице подписки ──────────────────────────────────

async fn apps_list(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows = sqlx::query(
        "SELECT id, platform, sort_order, name, deeplink, store_url, guide,
                is_active, icon_url, icon_svg
           FROM subscription_page_apps ORDER BY platform, sort_order, id",
    )
    .fetch_all(&st.pool)
    .await?;

    Ok(Json(json!(rows
        .iter()
        .map(|r| json!({
            "id": r.get::<i64, _>("id"),
            "platform": r.get::<String, _>("platform"),
            "sort_order": r.get::<i32, _>("sort_order"),
            "name": r.get::<String, _>("name"),
            "deeplink": r.get::<Option<String>, _>("deeplink"),
            "store_url": r.get::<Option<String>, _>("store_url"),
            "guide": r.get::<Option<String>, _>("guide"),
            "is_active": r.get::<bool, _>("is_active"),
            "icon_url": r.get::<Option<String>, _>("icon_url"),
            "has_icon": r.get::<Option<String>, _>("icon_svg").is_some()
                || r.get::<Option<String>, _>("icon_url").is_some(),
        }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct AppBody {
    platform: Option<String>,
    name: Option<String>,
    deeplink: Option<String>,
    store_url: Option<String>,
    guide: Option<String>,
    sort_order: Option<i32>,
    is_active: Option<bool>,
    icon_url: Option<String>,
}

const PLATFORMS: [&str; 5] = ["ios", "android", "windows", "macos", "linux"];

async fn app_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<AppBody>,
) -> Result<Json<Value>> {
    let platform = b.platform.as_deref().unwrap_or_default().to_lowercase();
    if !PLATFORMS.contains(&platform.as_str()) {
        return Err(Error::bad(format!(
            "платформа должна быть одной из: {}",
            PLATFORMS.join(", ")
        )));
    }
    let name = b.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::bad("нужно название приложения"));
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO subscription_page_apps
            (platform, sort_order, name, deeplink, store_url, guide, is_active, icon_url)
         VALUES ($1, COALESCE($2, (SELECT COALESCE(max(sort_order), 0) + 10
                                     FROM subscription_page_apps WHERE platform = $1)),
                 $3, $4, $5, $6, COALESCE($7, true), $8)
         RETURNING id",
    )
    .bind(&platform)
    .bind(b.sort_order)
    .bind(name)
    .bind(&b.deeplink)
    .bind(&b.store_url)
    .bind(&b.guide)
    .bind(b.is_active)
    .bind(&b.icon_url)
    .fetch_one(&st.pool)
    .await?;

    Ok(Json(json!({ "id": id })))
}

async fn app_update(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<AppBody>,
) -> Result<Json<Value>> {
    let res = sqlx::query(
        "UPDATE subscription_page_apps SET
            name = COALESCE($2, name),
            deeplink = COALESCE($3, deeplink),
            store_url = COALESCE($4, store_url),
            guide = COALESCE($5, guide),
            sort_order = COALESCE($6, sort_order),
            is_active = COALESCE($7, is_active),
            icon_url = COALESCE($8, icon_url)
          WHERE id = $1",
    )
    .bind(id)
    .bind(b.name.as_deref().map(str::trim))
    .bind(&b.deeplink)
    .bind(&b.store_url)
    .bind(&b.guide)
    .bind(b.sort_order)
    .bind(b.is_active)
    .bind(&b.icon_url)
    .execute(&st.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn app_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM subscription_page_apps WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

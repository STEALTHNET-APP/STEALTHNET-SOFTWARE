//! Public cabinet gateway. No database, payment secrets or admin access here.
use axum::{Router,body::Bytes,extract::{State,Path,Query,ConnectInfo,DefaultBodyLimit},http::{HeaderMap,StatusCode,Method,Uri,header},response::{IntoResponse,Response},routing::{get,any}};
use serde_json::{Value,json};
use std::{net::SocketAddr,sync::Arc};
#[derive(Clone)]struct App{http:reqwest::Client,api:String,key:String,origin:String}
type S=Arc<App>;
fn escape(s:&str)->String{s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&#39;")}
fn text<'a>(v:&'a Value,k:&str)->&'a str{v[k].as_str().unwrap_or("")}
fn cookie(h:&HeaderMap)->Option<String>{
    let values:Vec<_>=h.get_all(header::COOKIE).iter().filter_map(|v|v.to_str().ok()).flat_map(|v|v.split(';')).filter_map(|s|s.trim().strip_prefix("__Host-sn-cabinet=")).collect();
    (values.len()==1&&values[0].len()==64&&values[0].bytes().all(|b|b.is_ascii_hexdigit())).then(||values[0].to_owned())
}
fn allowed(path:&str,method:&Method)->bool{
    let get=["config","catalog","me","tariffs","payments","devices","addons","referral","tickets","auth/session"];
    let post=["auth/register","auth/login","auth/logout","auth/logout-all","auth/code-saved","auth/code","auth/code-remember","auth/rotate","auth/telegram","quote","pay","autorenew","addons/pay","tickets"];
    if *method==Method::GET&&get.contains(&path)||*method==Method::POST&&post.contains(&path){return true;}
    let parts:Vec<_>=path.split('/').collect();
    match parts.as_slice(){
        ["tickets",id]=>id.parse::<u64>().is_ok()&&matches!(*method,Method::GET|Method::POST),
        ["payments",id,"check"]=>id.parse::<u64>().is_ok()&&*method==Method::POST,
        ["devices",id]=>!id.is_empty()&&id.len()<=512&&*method==Method::DELETE,
        _=>false
    }
}
fn miniapp_allowed(path:&str,method:&Method)->bool{
    if path.starts_with("auth/")||path=="catalog" {return false;}
    path=="access"&&matches!(*method,Method::GET|Method::POST)||path=="access/reveal"&&*method==Method::POST||allowed(path,method)
}
async fn miniapp_page(State(st):State<S>)->Response{
    let response=st.http.get(format!("{}/api/app/config",st.api)).header("x-cabinet-service-key",&st.key).send().await;
    let Ok(r)=response else{return error(StatusCode::SERVICE_UNAVAILABLE,"Mini App временно недоступно");};
    if !r.status().is_success(){return error(StatusCode::SERVICE_UNAVAILABLE,"Mini App недоступно. Обратитесь к владельцу сервиса");}
    let Ok(config)=r.json::<Value>().await else{return error(StatusCode::BAD_GATEWAY,"Не удалось загрузить приложение");};
    let html=include_str!("../../../../web/app/index.html").replace("{{brand}}",&escape(text(&config,"brand")));
    let mut response=secure(axum::response::Html(html).into_response());
    response.headers_mut().remove("x-frame-options");
    response.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'self' https://telegram.org; style-src 'self' 'unsafe-inline'; img-src 'self' https: data:; connect-src 'self'; font-src 'self'; frame-src 'none'; frame-ancestors https://web.telegram.org https://*.web.telegram.org; base-uri 'none'; form-action 'self'".parse().unwrap());
    response
}
fn secure(mut r:Response)->Response{
    let h=r.headers_mut();
    h.insert(header::CACHE_CONTROL,"no-store".parse().unwrap());
    h.insert("referrer-policy","no-referrer".parse().unwrap());
    h.insert("x-content-type-options","nosniff".parse().unwrap());
    h.insert("x-frame-options","DENY".parse().unwrap());
    h.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' https: data:; connect-src 'self'; font-src 'self'; frame-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    r
}
fn error(status:StatusCode,message:&str)->Response{secure((status,axum::Json(json!({"error":message}))).into_response())}
async fn upstream(st:&App,path:&str)->Option<Value>{
    let r=st.http.get(format!("{}/api/cabinet/{path}",st.api)).header("x-cabinet-service-key",&st.key).send().await.ok()?;
    if !r.status().is_success(){return None;}r.json().await.ok()
}
async fn proxy(State(st):State<S>,Path(path):Path<String>,ConnectInfo(peer):ConnectInfo<SocketAddr>,method:Method,uri:Uri,headers:HeaderMap,body:Bytes)->Response{
    proxy_request(st,path,peer,method,uri,headers,body,false).await
}
async fn miniapp_proxy(State(st):State<S>,Path(path):Path<String>,ConnectInfo(peer):ConnectInfo<SocketAddr>,method:Method,uri:Uri,headers:HeaderMap,body:Bytes)->Response{
    proxy_request(st,path,peer,method,uri,headers,body,true).await
}
async fn proxy_request(st:S,path:String,peer:SocketAddr,method:Method,uri:Uri,headers:HeaderMap,body:Bytes,miniapp:bool)->Response{
    if !(if miniapp {miniapp_allowed(&path,&method)} else {allowed(&path,&method)}){return error(StatusCode::NOT_FOUND,"Маршрут не найден");}
    if method!=Method::GET&&headers.get(header::ORIGIN).and_then(|h|h.to_str().ok())!=Some(st.origin.as_str()){return error(StatusCode::FORBIDDEN,"Откройте кабинет на его основном домене");}
    let ip=if peer.ip().is_loopback(){headers.get("x-real-ip").and_then(|v|v.to_str().ok()).and_then(|v|v.parse::<std::net::IpAddr>().ok()).unwrap_or(peer.ip())}else{peer.ip()};
    // Path was decoded by Axum; re-encode device identifiers instead of interpreting them as URLs.
    let safe_path=if let Some(id)=path.strip_prefix("devices/"){let mut u=reqwest::Url::parse("http://local/").unwrap();u.path_segments_mut().unwrap().extend(["devices",id]);u.path().trim_start_matches('/').to_owned()}else{path.clone()};
    let surface=if miniapp {"app"}else{"cabinet"};
    let mut url=format!("{}/api/{surface}/{safe_path}",st.api);
    if let Some(q)=uri.query(){url.push('?');url.push_str(q);}
    let mut req=st.http.request(method,&url).header("x-cabinet-service-key",&st.key).header("x-cabinet-client-ip",ip.to_string()).header(header::CONTENT_TYPE,"application/json");
    if miniapp {
        if let Some(init)=headers.get("x-telegram-init-data"){req=req.header("x-telegram-init-data",init);}
    }else if let Some(token)=cookie(&headers){req=req.bearer_auth(token);}
    for k in ["origin","x-csrf-token"]{if let Some(v)=headers.get(k){req=req.header(k,v);}}
    let r=match req.body(body).send().await{Ok(r)=>r,Err(_)=>return error(StatusCode::BAD_GATEWAY,"Не удалось связаться с сервисом. Повторите позже")};
    let status=r.status();let mut payload:Value=match r.json().await{Ok(v)=>v,Err(_)=>return error(StatusCode::BAD_GATEWAY,"Сервис вернул некорректный ответ")};
    let session=payload.as_object_mut().and_then(|v|v.remove("session_token")).and_then(|v|v.as_str().map(str::to_owned));
    let logout=payload["logged_out"]==true;
    let mut response=secure((status,axum::Json(payload)).into_response());
    if let Some(token)=session.filter(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())){
        response.headers_mut().insert(header::SET_COOKIE,format!("__Host-sn-cabinet={token}; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000").parse().unwrap());
    }else if logout{response.headers_mut().insert(header::SET_COOKIE,"__Host-sn-cabinet=; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=0".parse().unwrap());}
    response
}
fn english_config(config:&Value)->Value {
    let mut out=config.clone();let en=&config["locales"]["en"];
    for field in ["headline","description","tariffs_heading","faq_heading","seo_title","seo_description","support_label","support_text","docs_text"] {out[field]=json!(text(en,field));}
    for field in ["steps","faq","docs_links"] {out[field]=en[field].as_array().map(|x|json!(x)).unwrap_or(json!([]));}
    out
}
async fn page(State(st):State<S>,Query(q):Query<std::collections::BTreeMap<String,String>>)->Response{
    let en=q.get("lang").is_some_and(|s|s=="en");
    let Some(config)=upstream(&st,"config").await else{return secure((StatusCode::SERVICE_UNAVAILABLE,axum::response::Html("<!doctype html><html lang=\"ru\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Сайт недоступен</title><h1>Сайт временно недоступен</h1><p>Попробуйте открыть страницу позже.</p></html>")).into_response());};
    let config=if en {english_config(&config)}else{config};
    let mut html=include_str!("../../../../web/cabinet/index.html").to_string();
    html=html.replace("{{language}}",if en{"en"}else{"ru"});
    let page_url=format!("{}/{}",st.origin,if en{"?lang=en"}else{""});
    html=html.replace("{{page_url}}",&escape(&page_url));
    if en {for (ru,eng) in [("Выбрать тариф","Choose a plan"),("У меня уже есть код","I already have a code"),("Обновить данные","Refresh data"),("Обновить","Refresh"),("Аккаунт","Account"),("Разделы","Sections")]{html=html.replace(ru,eng);}}

    for key in ["brand","headline","description","logo","favicon","seo_title","seo_description","og_image","tariffs_heading"]{html=html.replace(&format!("{{{{{key}}}}}"),&escape(text(&config,key)));}
    html=html.replace("{{origin}}",&escape(&st.origin));
    let docs=config["docs_links"].as_array().map(|rows|rows.iter().map(|r|format!("<a href=\"{}\" rel=\"noopener noreferrer\">{}</a>",escape(text(r,"url")),escape(text(r,"title")))).collect::<String>()).unwrap_or_default();
    let faq=config["faq"].as_array().map(|rows|rows.iter().map(|r|format!("<details><summary>{}</summary><p>{}</p></details>",escape(text(r,"question")),escape(text(r,"answer")))).collect::<String>()).unwrap_or_default();
    let steps=config["steps"].as_array().map(|rows|rows.iter().enumerate().map(|(i,r)|format!("<li><span>{}</span><div><strong>{}</strong><p>{}</p></div></li>",i+1,escape(text(r,"title")),escape(text(r,"text")))).collect::<String>()).unwrap_or_default();
    html=html.replace("{{steps}}",&steps);
    let Some(catalog)=upstream(&st,"catalog").await else{return error(StatusCode::SERVICE_UNAVAILABLE,"Не удалось загрузить тарифы");};
    let currency=text(&catalog,"currency");
    let plans=catalog["tariffs"].as_array().map(|rows|rows.iter().filter_map(|t|{
        let price=t["prices"].as_array()?.iter().filter(|p|p["currency"]==currency).min_by_key(|p|p["days"].as_i64())?;
        let amount=price["amount_minor"].as_i64()?;let days=price["days"].as_i64()?;
        let localized=&t["locales"]["en"];
        let title=if en&&!text(localized,"title").is_empty(){text(localized,"title")}else{text(t,"title")};
        let description=if en{text(localized,"description")}else{text(t,"description")};
        Some(format!("<article class=\"public-plan\"><h3>{}</h3><p>{}</p><div class=\"public-price\"><strong>{}</strong><span>{} {} {}</span></div><a class=\"btn\" href=\"#login\">{}</a></article>",escape(title),escape(description),escape(&sn_core::money::format_minor(amount,currency)),if en{"for"}else{"за"},days,if en {if days==1{"day"}else{"days"}}else{"дн."},if en{"Choose a plan"}else{"Выбрать тариф"}))
    }).collect::<String>()).unwrap_or_default();
    html=html.replace("{{tariffs}}",if plans.is_empty(){if en{"<p>No plans are available at the moment.</p>"}else{"<p>Сейчас нет доступных тарифов.</p>"}}else{&plans});
    if text(&config,"og_image").is_empty(){html=html.replace("<meta property=\"og:image\" content=\"\">","");}
    html=html.replace("{{docs}}",&docs).replace("{{faq}}",&faq);
    let robots=if config["indexable"]==true&&(!en||!text(&config,"seo_title").is_empty()){"index,follow"}else{"noindex,nofollow"};
    html=html.replace("{{robots}}",robots);
    secure(axum::response::Html(html).into_response())
}
async fn robots(State(st):State<S>)->Response{let allowed=upstream(&st,"config").await.is_some_and(|c|c["indexable"]==true);([(header::CONTENT_TYPE,"text/plain; charset=utf-8")],if allowed{format!("User-agent: *\nAllow: /\nDisallow: /api/\nSitemap: {}/sitemap.xml\n",st.origin)}else{"User-agent: *\nDisallow: /\n".into()}).into_response()}
async fn sitemap(State(st):State<S>)->Response{([(header::CONTENT_TYPE,"application/xml")],format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>{}/</loc></url><url><loc>{}/?lang=en</loc></url></urlset>",escape(&st.origin),escape(&st.origin))).into_response()}
async fn ready(State(st):State<S>)->Response{if upstream(&st,"config").await.is_some(){(StatusCode::OK,"ready").into_response()}else{(StatusCode::SERVICE_UNAVAILABLE,"upstream unavailable").into_response()}}
#[tokio::main]async fn main()->Result<(),Box<dyn std::error::Error>>{
    if std::env::args().nth(1).as_deref()==Some("--version"){println!("sn-cabinet {}",env!("CARGO_PKG_VERSION"));return Ok(());}
    let origin=std::env::var("CABINET_PUBLIC_URL")?.trim_end_matches('/').to_string();let u=reqwest::Url::parse(&origin)?;
    if u.scheme()!="https"||u.path()!="/"||u.query().is_some()||u.fragment().is_some()||u.host_str().is_none()||!u.username().is_empty()||u.password().is_some(){return Err("CABINET_PUBLIC_URL must be an HTTPS origin".into());}
    let api=std::env::var("PANEL_API_URL")?.trim_end_matches('/').to_string();let url=reqwest::Url::parse(&api)?;
    if url.path()!="/"||url.query().is_some()||url.fragment().is_some()||!url.username().is_empty()||url.password().is_some()||url.scheme()!="https"&&!(url.scheme()=="http"&&matches!(url.host_str(),Some("127.0.0.1"|"[::1]"|"localhost"))){return Err("PANEL_API_URL requires TLS except loopback".into());}
    let key=std::env::var("CABINET_SERVICE_KEY")?;if key.len()!=64||!key.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Invalid service key".into());}
    let st=Arc::new(App{http:reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build()?,api,key,origin});
    // A periodic authenticated readiness request makes installation state observable in admin/bot.
    let heartbeat=st.clone();
    tokio::spawn(async move {let mut interval=tokio::time::interval(std::time::Duration::from_secs(30));loop{interval.tick().await;let _=upstream(&heartbeat,"config").await;}});
    let app=Router::new().route("/",get(page)).route("/robots.txt",get(robots)).route("/sitemap.xml",get(sitemap)).route("/health",get(||async{"ok"})).route("/ready",get(ready))
        .route("/api/cabinet/{*path}",any(proxy))
        .route("/api/miniapp/{*path}",any(miniapp_proxy)).route("/app/",get(miniapp_page))
        .route("/app.css",get(||async{([(header::CONTENT_TYPE,"text/css; charset=utf-8")],include_str!("../../../../web/app/app.css"))}))
        .route("/fonts/roboto.ttf",get(||async{([(header::CONTENT_TYPE,"font/ttf"),(header::CACHE_CONTROL,"public, max-age=31536000, immutable")],include_bytes!("../../../../web/app/fonts/roboto.ttf").as_slice())}))
        .route("/guided.css",get(||async{([(header::CONTENT_TYPE,"text/css; charset=utf-8")],include_str!("../../../../web/app/guided.css"))}))
        .route("/app.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../../../../web/app/app.js"))}))
        .route("/customer-en.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../../../../web/app/customer-en.js"))}))
        .route("/customer-i18n.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../../../../web/app/customer-i18n.js"))}))
        .route("/customer-ui.js",get(||async{([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],include_str!("../../../../web/app/customer-ui.js"))}))
        .route("/cabinet.css",get(||async{([(header::CONTENT_TYPE,"text/css; charset=utf-8")],include_str!("../../../../web/cabinet/cabinet.css"))}))
        .layer(DefaultBodyLimit::max(32768)).with_state(st);
    let bind=std::env::var("CABINET_BIND").unwrap_or_else(|_|"127.0.0.1:8090".into());let listener=tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener,app.into_make_service_with_connect_info::<SocketAddr>()).await?;Ok(())
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn gateway_has_no_admin_or_open_proxy_routes(){for p in ["../clients","auth/me","https://example.com","payments/1/refund","tickets/../../settings"]{assert!(!allowed(p,&Method::GET));assert!(!allowed(p,&Method::POST));}assert!(allowed("tickets/12",&Method::POST));assert!(!allowed("devices/one/two",&Method::DELETE));}
    #[test]fn miniapp_cannot_use_browser_auth_endpoints(){assert!(!miniapp_allowed("auth/register",&Method::POST));assert!(!miniapp_allowed("auth/login",&Method::POST));assert!(!miniapp_allowed("catalog",&Method::GET));assert!(miniapp_allowed("me",&Method::GET));assert!(miniapp_allowed("access",&Method::POST));}
    #[test]fn duplicate_and_malformed_cookies_are_rejected(){let mut h=HeaderMap::new();h.insert(header::COOKIE,format!("__Host-sn-cabinet={}","a".repeat(64)).parse().unwrap());assert!(cookie(&h).is_some());h.append(header::COOKIE,format!("__Host-sn-cabinet={}","b".repeat(64)).parse().unwrap());assert!(cookie(&h).is_none());}
}

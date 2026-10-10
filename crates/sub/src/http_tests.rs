use super::*;

async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
    (url, task)
}

#[tokio::test]
async fn short_and_legacy_urls_locales_qr_and_vpn_formats() {
    use axum::routing::post;
    use serde_json::json;
    let panel = Router::new()
        .route("/api/sub/settings", get(|| async { Json(json!({"subscription.title":"Подключение", "subscription.remark_expired":"⛔ Подписка истекла {date}"})) }))
        .route("/api/sub/apps", get(|| async { Json(json!([{"name":"Happ","platform":"android","sort_order":0,"deeplink":"happ://add/{url}","store_url":"https://example.com/download"}])) }))
        .route("/api/sub/rules-effective", get(|| async { Json(json!([])) }))
        .route("/api/sub/templates-effective", get(|| async { Json(json!([])) }))
        .route("/api/sub/{id}", get(|Path(id): Path<String>, headers: HeaderMap| async move {
            assert_eq!(headers["x-service-token"], "isolated-test-token");
            if id != "demo1234" && id != "expired1234" { return axum::http::StatusCode::NOT_FOUND.into_response(); }
            Json(json!({"username":"Тестовый клиент", "status":if id=="expired1234" {"expired"} else {"active"}, "traffic_used_bytes":1073741824i64,"traffic_limit_bytes":10737418240i64,"device_limit":2,"expires_at":"2020-01-01T00:00:00Z", "vpn_uuid":"00000000-0000-4000-8000-000000000001", "hosts":[{"remark":"Frankfurt","address":"node.example.com","port":443,"protocol":"vless","security":"none","network":"tcp"}]})).into_response()
        }))
        .route("/api/sub/{id}/request", post(|| async { Json(json!({"ok":true})) }));
    let (panel_url, panel_task) = serve(panel).await;
    let config = Config { database_url:String::new(), api_bind:String::new(), sub_bind:String::new(),sub_public_url:"https://sub.example.com".into(), brand_name:"VPN Test".into(),web_root:String::new(), sub_mode:"api".into(),panel_url:panel_url.clone(),sub_service_token:Some("isolated-test-token".into()) };
    let state = SubState { source:Source::Api {panel_url, token:"isolated-test-token".into(),http:reqwest::Client::new()}, config, settings:Default::default() };
    let (url, task) = serve(subscription_router(state)).await;
    let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let mut canonical = String::new();
    for path in ["/demo1234", "/s/demo1234"] {
        let response = http.get(format!("{url}{path}?lang=en")).header(header::USER_AGENT,"Mozilla/5.0").send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()[header::CONTENT_LANGUAGE], "en");
        assert!(response.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));
        let html = response.text().await.unwrap();
        assert!(html.contains("<html lang=\"en\""));
        assert!(html.contains("<title>VPN Test — Connection</title>"));
        assert!(html.contains("Connect your device"));
        assert!(html.contains("Тестовый клиент"), "Customer names must not be translated");
        assert!(html.contains("https://sub.example.com/demo1234"));
        assert!(!html.contains("https://sub.example.com/s/demo1234"));
        assert!(!html.contains("demo1234?lang="));
        let ru = http.get(format!("{url}{path}")).header(header::USER_AGENT,"Mozilla/5.0").header(header::ACCEPT_LANGUAGE,"en-US").header(header::COOKIE,"sn.sub.lang=ru").send().await.unwrap().text().await.unwrap();
        assert!(ru.contains("<html lang=\"ru\""));
        for query in ["", "?lang=en", "?lang=ru"] {
            let response = http.get(format!("{url}{path}{query}")).header(header::USER_AGENT,"curl/8.0").send().await.unwrap();
            assert_eq!(response.status(), 200);
            let config = response.text().await.unwrap();
            if canonical.is_empty() { canonical = config.clone(); }
            assert_eq!(config, canonical, "URL aliases and page language must not alter the VPN config");
        }
        let qr = http.get(format!("{url}{path}/qr.svg")).send().await.unwrap();
        assert_eq!(qr.headers()[header::CONTENT_TYPE], "image/svg+xml; charset=utf-8");
        assert_eq!(qr.text().await.unwrap(), page::qr_svg_public("https://sub.example.com/demo1234"));
    }
    let expired = http.get(format!("{url}/expired1234?lang=en")).header(header::USER_AGENT,"Mozilla/5.0").send().await.unwrap().text().await.unwrap();
    assert!(expired.contains("Subscription expired on 01.01.2020"));
    assert!(!expired.contains("happ://add/"));
    assert!(!expired.contains("id=\"subscriptionLink\""));
    for path in ["/missing", "/s/missing", "/missing/qr.svg", "/s/missing/qr.svg", "/bad%2Fpath", "/favicon.ico"] {
        assert_eq!(http.get(format!("{url}{path}")).send().await.unwrap().status(), 404, "{path}");
    }
    assert_eq!(http.get(format!("{url}/health")).send().await.unwrap().text().await.unwrap(), "ok");
    assert_eq!(http.get(format!("{url}/ready")).send().await.unwrap().status(), 200);
    task.abort(); panel_task.abort();
}

#[tokio::test]
async fn early_errors_preserve_status_and_machine_contract_without_reflection() {
    use axum::body::to_bytes;
    for (error, status) in [(Error::NotFound, 404), (Error::Internal("secret https://panel.invalid/token <script>".into()), 500)] {
        let response = subscription_load_error(error, Format::WebPage, locale::Language::En);
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = String::from_utf8(to_bytes(response.into_body(), 10000).await.unwrap().to_vec()).unwrap();
        assert!(body.contains("Unable to open subscription"));
        assert!(!body.contains("secret")); assert!(!body.contains("<script>")); assert!(!body.contains("panel.invalid"));
    }
    for ua in ["Happ Mozilla/5.0", "Stash", "sing-box", "v2rayNG", "curl/8", "", "unknown-client"] {
        let response = subscription_load_error(Error::NotFound, detect_format(ua), locale::Language::En);
        assert_eq!(response.status(), 404);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(to_bytes(response.into_body(), 10000).await.unwrap().as_ref(), "{\"error\":\"не найдено\"}".as_bytes());
    }
}

#[tokio::test]
async fn source_errors_and_state_announcements_use_resolved_reason_without_changing_payloads() {
    use axum::routing::post;
    use serde_json::json;
    use base64::Engine as _;
    let renewed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let panel_renewed = renewed.clone();
    let panel = Router::new()
        .route("/api/sub/settings", get(|| async { Json(json!({"subscription.announce":"Choose a server", "subscription.response_headers":[{"key":"announce","value":"Global header"}]})) }))
        .route("/api/sub/rules-effective", get(|| async { Json(json!([
            {"ua_pattern":"RuleClient","action":"plain","response_headers":[{"key":"announce","value":"Rule announcement"}]},
            {"ua_pattern":"BlockedClient","action":"block","response_headers":[{"key":"announce","value":"Choose a server"}]}
        ])) }))
        .route("/api/sub/templates-effective", get(|| async { Json(json!({})) }))
        .route("/api/sub/apps", get(|| async { Json(json!([])) }))
        .route("/api/sub/{id}", get(move |Path(id): Path<String>| { let renewed=panel_renewed.clone(); async move {
            if id == "missing" { return axum::http::StatusCode::NOT_FOUND.into_response(); }
            if id == "failure" { return (axum::http::StatusCode::BAD_GATEWAY, "private upstream failure").into_response(); }
            let status = match id.as_str() {"renewal" if !renewed.load(std::sync::atomic::Ordering::SeqCst) => "expired","expired" | "limited" | "disabled" => id.as_str(), _ => "active"};
            let settings = if id=="unsupported" {json!({"subscription.require_hwid":true})} else if id=="custom" {json!({"subscription.remark_no_hosts":["Custom <state> {username}","Contact support"]})} else {json!({})};
            Json(json!({"username":"<script>customer</script>","status":status,"expires_at":"2020-01-01T00:00:00Z","traffic_used_bytes":5,"traffic_limit_bytes":10,"device_limit":1,"vpn_uuid":"00000000-0000-4000-8000-000000000001","hosts":if id=="no_hosts" || id=="custom" {json!([])} else {json!([{"remark":"Server","address":"node.example.test","port":443,"protocol":"vless","security":"none","network":"tcp"}])},"external_squad":{"settings":settings}})).into_response()
        }}))
        .route("/api/sub/{id}/device", post(|Path(id): Path<String>| async move {Json(json!({"over_limit":id=="devices","used":1,"limit":1}))}))
        .route("/api/sub/{id}/request", post(|| async { Json(json!({"ok":true})) }));
    let (panel_url, panel_task) = serve(panel).await;
    let config = Config { database_url:String::new(), api_bind:String::new(), sub_bind:String::new(),sub_public_url:"https://sub.example.test".into(), brand_name:"Test".into(),web_root:String::new(), sub_mode:"api".into(),panel_url:panel_url.clone(),sub_service_token:Some("mock-token".into()) };
    let state = SubState { source:Source::Api {panel_url, token:"mock-token".into(),http:reqwest::Client::new()}, config, settings:Default::default() };
    let (url, task) = serve(subscription_router(state)).await;
    let http = reqwest::Client::new();
    for (id, status) in [("missing",404), ("failure",500)] {
        for ua in ["Mozilla/5.0 (iPhone) Safari Telegram", "Mozilla/5.0 (Android) Telegram", "Mozilla/5.0 Safari"] {
            for lang in ["ru","en"] {
                let r=http.get(format!("{url}/s/{id}?lang={lang}")).header(header::USER_AGENT,ua).header(header::ACCEPT,"text/html").send().await.unwrap();
                assert_eq!(r.status().as_u16(),status);
                assert_eq!(r.headers()[header::CONTENT_TYPE],"text/html; charset=utf-8");
                assert_eq!(r.headers()[header::CONTENT_LANGUAGE],lang);
                let body=r.text().await.unwrap();
                assert!(body.contains(if lang=="ru" {"Не удалось открыть подписку"} else {"Unable to open subscription"}));
                assert!(!body.contains("private upstream"));
            }
        }
    }
    for (id, english) in [("expired","Subscription expired"),("limited","Data allowance"),("disabled","Access suspended"),("no_hosts","No locations"),("devices","Devices used"),("unsupported","device ID"),("custom","Custom <state>")] {
        for ua in ["curl/8", "Happ", "v2rayNG", "Clash", "Mihomo", "Stash", "sing-box", "RuleClient"] {
            let mut payload=None;
            for lang in ["ru","en"] {
                let mut request=http.get(format!("{url}/{id}?lang={lang}")).header(header::USER_AGENT,ua);
                if id=="devices" {request=request.header("x-hwid","fixture-device");}
                let r=request.send().await.unwrap();
                assert_eq!(r.status(),200);
                let raw=r.headers()["announce"].to_str().unwrap();
                let announce=raw.strip_prefix("base64:").map(|s|String::from_utf8(base64::engine::general_purpose::STANDARD.decode(s).unwrap()).unwrap()).unwrap_or_else(||raw.to_string());
                assert!(!announce.contains("Choose a server")); assert!(!announce.contains("Global header"));
                if lang=="en" {assert!(announce.contains(english),"{id}/{ua}: {announce}");}
                let body=r.text().await.unwrap();
                if let Some(previous)=&payload {assert_eq!(&body,previous,"Locale must not change machine payload {id}/{ua}");} else {payload=Some(body);}
            }
        }
    }
    let r=http.get(format!("{url}/active")).header(header::USER_AGENT,"Happ").send().await.unwrap();
    assert_eq!(r.headers()["announce"],"Global header");
    assert!(r.text().await.unwrap().contains("node.example.test"));
    let r=http.get(format!("{url}/active")).header(header::USER_AGENT,"RuleClient").send().await.unwrap();
    assert_eq!(r.headers()["announce"],"Rule announcement");
    let r=http.get(format!("{url}/active")).header(header::USER_AGENT,"BlockedClient").send().await.unwrap();
    assert_eq!(r.status(),403); assert!(!r.headers().contains_key("announce"));
    let r=http.get(format!("{url}/renewal")).header(header::USER_AGENT,"Happ").send().await.unwrap();
    assert_ne!(r.headers()["announce"],"Global header");
    renewed.store(true,std::sync::atomic::Ordering::SeqCst);
    let r=http.get(format!("{url}/renewal")).header(header::USER_AGENT,"Happ").send().await.unwrap();
    assert_eq!(r.headers()["announce"],"Global header");
    assert!(r.text().await.unwrap().contains("node.example.test"));
    let body=http.get(format!("{url}/custom?lang=en")).header(header::USER_AGENT,"Mozilla/5.0").send().await.unwrap().text().await.unwrap();
    assert!(body.contains("Custom &lt;state&gt;"));
    assert!(!body.contains("<script>customer</script>"));
    task.abort(); panel_task.abort();
}

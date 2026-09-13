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

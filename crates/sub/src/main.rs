//! Сервис выдачи подписок. Отдельный процесс и отдельный домен:
//! панель может лежать или быть заблокирована, а клиенты продолжат обновляться.

use sn_sub::{formats,policy};
mod page;
mod locale;
#[cfg(test)]
mod http_tests;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use sqlx::Row;

use formats::HostEntry;
use sn_core::{Config, Error, Pool, Result};

/// Источник данных: локальная база или API панели.
///
/// Режим `api` позволяет вынести сабку на отдельный сервер и отдельный домен —
/// тогда блокировка панели не отрезает клиентов от конфигов, а машина сабки
/// не имеет доступа к базе вообще.
#[derive(Clone)]
enum Source {
    Db(Pool),
    Api { panel_url: String, token: String, http: reqwest::Client },
}

#[derive(Clone)]
struct SubState {
    source: Source,
    config: Config,
    /// Настройки меняются раз в месяц, а читаются на каждый запрос подписки.
    /// В режиме `api` это был бы лишний HTTP-поход в панель на каждого клиента.
    settings: std::sync::Arc<tokio::sync::RwLock<SettingsCache>>,
}

/// Правило «User-Agent → формат», заданное администратором в панели.
#[derive(Clone,Default)]
struct Rule {
    /// Подстроки через `|`. Регулярные выражения тут излишни и дают
    /// тихие опечатки, а список приложений всё равно перечисляют руками.
    pattern: String,
    action: String,
    /// Тело шаблона, если правило велит отдавать шаблон.
    template: Option<String>,
    template_code: Option<String>,
    conditions: Option<Vec<policy::Condition>>,
    operator: String,
    response_headers: Vec<policy::ResponseHeader>,
    disable_hwid_check: bool,
}

#[derive(Default)]
struct SettingsCache {
    map: std::collections::HashMap<String, serde_json::Value>,
    rules: Vec<Rule>,
    fetched_at: Option<std::time::Instant>,
}

/// Насколько долго живёт кэш настроек. Полминуты — правка в панели заметна
/// почти сразу, но шквал запросов подписки не превращается в шквал запросов
/// к базе.
const SETTINGS_TTL: std::time::Duration = std::time::Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--version") {
        println!("sn-sub {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    tracing_subscriber::fmt()
        // sn_core здесь обязателен: именно там логируются ошибки БД.
        // Без него «внутренняя ошибка» в ответе не имеет следа в журнале.
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sn_sub=info,sn_core=info".into()),
        )
        .init();

    let config = Config::from_env()?;

    let source = if config.sub_mode == "api" {
        let url = reqwest::Url::parse(&config.panel_url)
            .map_err(|_| Error::bad("некорректный PANEL_URL"))?;
        let loopback = url.host_str().is_some_and(|host| host == "localhost"
            || host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
        if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
            || !url.username().is_empty() || url.password().is_some()
            || url.query().is_some() || url.fragment().is_some() {
            return Err(Error::bad("API панели требует HTTPS; HTTP разрешён только для loopback"));
        }
        let token = config.sub_service_token.clone().ok_or_else(|| {
            Error::Internal("SUB_MODE=api требует SUB_SERVICE_TOKEN".into())
        })?;
        tracing::info!(panel = config.panel_url, "режим: данные через API панели");
        Source::Api {
            panel_url: config.panel_url.clone(),
            token,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| Error::Internal(e.to_string()))?,
        }
    } else {
        tracing::info!("режим: прямой доступ к базе");
        Source::Db(sn_core::db::connect(&config.database_url).await?)
    };

    let state = SubState {
        source,
        config: config.clone(),
        settings: Default::default(),
    };

    let app = subscription_router(state);

    let listener = tokio::net::TcpListener::bind(&config.sub_bind)
        .await
        .map_err(|e| Error::Internal(format!("не смог занять {}: {e}", config.sub_bind)))?;

    tracing::info!("выдача подписок слушает {}", config.sub_bind);
    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(())
}

fn subscription_router(state: SubState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ready", get(readiness))
        .route("/fonts/roboto.ttf",get(||async{([(header::CONTENT_TYPE,"font/ttf"),(header::CACHE_CONTROL,"public, max-age=31536000, immutable")],include_bytes!("../../../web/app/fonts/roboto.ttf").as_slice())}))
        .route("/{short_id}", get(subscription))
        .route("/{short_id}/qr.svg", get(qr_image))
        // Keep existing subscriptions working without redirects or re-imports.
        .route("/s/{short_id}", get(subscription))
        .route("/s/{short_id}/qr.svg", get(qr_image))
        .with_state(state)
}

/// Readiness checks the real data source, without exposing client data or credentials.
async fn readiness(State(st): State<SubState>) -> Response {
    let ok = match &st.source {
        Source::Db(pool) => sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool).await.is_ok(),
        Source::Api { panel_url, token, http } => match http.get(format!("{}/api/sub/settings", panel_url.trim_end_matches('/'))).header("x-service-token", token).send().await {
            Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.map(|v| v.is_object()).unwrap_or(false),
            _ => false,
        },
    };
    (if ok { axum::http::StatusCode::OK } else { axum::http::StatusCode::SERVICE_UNAVAILABLE }, Json(serde_json::json!({"status":if ok {"ready"} else {"unavailable"}}))).into_response()
}

/// Что отдаём клиенту, определяется его User-Agent.
#[derive(Debug, PartialEq)]
enum Format {
    XrayJson,
    Base64,
    Plain,
    Clash,
    Mihomo,
    Stash,
    Block,
    NotFound,
    Unavailable,
    SingBox,
    WebPage,
}

/// Правила маршрутизации по User-Agent.
///
/// Порядок важен: проверяем сверху вниз, первое совпадение побеждает.
/// Браузеры ловим предпоследними — иначе клиент, у которого в UA есть
/// «Mozilla» (а таких много среди Chromium-обёрток), получит HTML вместо конфига.
fn detect_format(ua: &str) -> Format {
    let ua_lower = ua.to_lowercase();
    const CLASH: [&str; 4] = ["clash", "mihomo", "flclash", "stash"];
    const SINGBOX: [&str; 3] = ["sing-box", "singbox", "hiddify"];
    const XRAY_JSON: [&str; 4] = ["happ", "streisand", "v2box", "nekobox"];
    const BASE64: [&str; 3] = ["v2rayng", "v2rayn", "shadowrocket"];

    if ua_lower.contains("stash") {
        Format::Stash
    } else if ua_lower.contains("mihomo") || ua_lower.contains("flclash") {
        Format::Mihomo
    } else if CLASH.iter().any(|m| ua_lower.contains(m)) {
        Format::Clash
    } else if SINGBOX.iter().any(|m| ua_lower.contains(m)) {
        Format::SingBox
    } else if XRAY_JSON.iter().any(|m| ua_lower.contains(m)) {
        Format::XrayJson
    } else if BASE64.iter().any(|m| ua_lower.contains(m)) {
        Format::Base64
    } else if ua_lower.contains("mozilla") {
        Format::WebPage
    } else if ua.is_empty() || ua_lower.contains("curl") {
        Format::Plain
    } else {
        Format::Base64
    }
}

pub struct SubData {
    pub username: String,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub used: i64,
    pub limit: Option<i64>,
    pub device_limit: i32,
    /// Название тарифа — нужно ремаркам-заглушкам («продлите {tariff}»).
    pub tariff: Option<String>,
    pub hosts: Vec<HostEntry>,
    pub external_squad: serde_json::Value,
}

async fn load(source: &Source, short_id: &str) -> Result<SubData> {
    if short_id.is_empty() || short_id.len() > 128 || !short_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') { return Err(Error::NotFound); }
    match source {
        Source::Db(pool) => load_from_db(pool, short_id).await,
        Source::Api { panel_url, token, http } => load_from_api(panel_url, token, http, short_id).await,
    }
}

/// Получение данных через API панели. Формат ответа тот же, что кладёт
/// в базу локальный режим, поэтому дальше код одинаковый.
async fn load_from_api(
    panel_url: &str,
    token: &str,
    http: &reqwest::Client,
    short_id: &str,
) -> Result<SubData> {
    let res = http
        .get(format!("{panel_url}/api/sub/{short_id}"))
        .header("x-service-token", token)
        .send()
        .await
        .map_err(|e| Error::Internal(format!("панель недоступна: {e}")))?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::NotFound);
    }
    if !res.status().is_success() {
        return Err(Error::Internal(format!("панель ответила {}", res.status())));
    }

    let v: serde_json::Value = res
        .json()
        .await
        .map_err(|e| Error::Internal(format!("панель вернула не JSON: {e}")))?;

    let vpn_uuid = v["vpn_uuid"].as_str().unwrap_or_default().to_string();
    let hosts = v["hosts"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|h| HostEntry {
                    remark: h["remark"].as_str().unwrap_or_default().to_string(),
                    address: h["address"].as_str().unwrap_or_default().to_string(),
                    port: h["port"].as_i64().unwrap_or(443) as i32,
                    protocol: h["protocol"].as_str().unwrap_or("vless").to_string(),
                    network: h["network"].as_str().unwrap_or("tcp").to_string(),
                    security: h["security"].as_str().unwrap_or("none").to_string(),
                    sni: h["sni"].as_str().map(String::from),
                    fingerprint: h["fingerprint"].as_str().map(String::from),
                    alpn: h["alpn"].as_str().map(String::from),
                    path: h["path"].as_str().map(String::from),
                    public_key: h["public_key"].as_str().map(String::from),
                    short_id: h["short_id"].as_str().map(String::from),
                    method: h["method"].as_str().map(String::from),
                    service_name: h["service_name"].as_str().map(String::from),
                    server_key: h["server_key"].as_str().map(String::from),
                    host_header: h["host_header"].as_str().map(String::from),
                    options: {let mut v=h["options"].as_object().cloned().unwrap_or_default();if let Some(t)=h["host_template"].as_str(){v.insert("xray_template_body".into(),serde_json::json!(t));}serde_json::Value::Object(v)},
                    uuid: vpn_uuid.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(SubData {
        external_squad:v["external_squad"].clone(),
        username: v["username"].as_str().unwrap_or_default().to_string(),
        status: v["status"].as_str().unwrap_or("expired").to_string(),
        tariff: v["tariff"].as_str().map(String::from),
        expires_at: v["expires_at"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc)),
        used: v["traffic_used_bytes"].as_i64().unwrap_or(0),
        limit: v["traffic_limit_bytes"].as_i64(),
        device_limit: v["device_limit"].as_i64().unwrap_or(1) as i32,
        hosts,
    })
}

async fn load_from_db(pool: &Pool, short_id: &str) -> Result<SubData> {
    let row = sqlx::query(
        // Статус считаем функцией, а не берём хранимое поле: иначе подписка
        // с прошедшей датой продолжает выдавать конфиги, пока воркер не проснётся.
        "SELECT c.id, c.username, c.vpn_uuid,
                effective_status(c.status, s.expires_at, s.traffic_used_bytes,
                                 s.traffic_limit_bytes)::text AS status,
                s.expires_at, s.traffic_used_bytes, s.traffic_limit_bytes, s.device_limit,
                t.title AS tariff
           FROM clients c
           LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
           LEFT JOIN tariffs t       ON t.id = s.tariff_id
          WHERE c.short_id = $1 AND c.deleted_at IS NULL",
    )
    .bind(short_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)?;

    let client_id: i64 = row.get("id");
    let vpn_uuid: uuid::Uuid = row.get("vpn_uuid");

    // Клиент видит только те хосты, чьи инбаунды доступны его сквадам,
    // и только если инбаунд реально поднят на живой ноде.
    // Reality-параметры — только из профиля: копия в строке хоста
    // расходилась с ним при перевыпуске ключей, и локация переставала
    // подключаться, ничего об этом не сообщая. Имя сервера у Reality
    // тоже оттуда; у прочих протоколов SNI задаётся на хосте.
    let host_rows = sqlx::query(
        "SELECT DISTINCT h.id, h.sort_order, h.remark, h.address, h.port,
                CASE WHEN h.options->>'security'='inherit' THEN COALESCE(i.security::text,h.security::text) ELSE h.security::text END AS security,
                COALESCE(i.sni, h.sni) AS sni,
                h.fingerprint, h.alpn, h.path,
                i.public_key, i.short_id,
                h.host_header, h.options, ht.body AS host_template,
                i.protocol, COALESCE(i.network, 'tcp') AS network,
                i.method, i.service_name, i.server_key
           FROM hosts h
           LEFT JOIN subscription_templates ht ON ht.id = (h.options->>'xray_template_id')::bigint AND ht.code='xray_json'
           JOIN inbounds i        ON i.id = h.inbound_id
           JOIN squad_inbounds si ON si.inbound_id = i.id
           JOIN client_squads cs  ON cs.squad_id = si.squad_id
           JOIN node_inbounds ni  ON ni.inbound_id = i.id
           JOIN nodes n           ON n.id = ni.node_id
          WHERE cs.client_id = $1
            AND h.is_enabled
            AND NOT COALESCE(h.options->'exclude_squad_ids','[]'::jsonb) @> jsonb_build_array(cs.squad_id)
            AND n.status = 'online'
            AND n.deleted_at IS NULL
          ORDER BY h.sort_order, h.id",
    )
    .bind(client_id)
    .fetch_all(pool)
    .await?;

    let hosts = host_rows
        .iter()
        .map(|r| HostEntry {
            remark: r.get("remark"),
            address: r.get("address"),
            port: r.get("port"),
            protocol: r.get("protocol"),
            network: r.get("network"),
            security: r.get("security"),
            sni: r.get("sni"),
            fingerprint: r.get("fingerprint"),
            alpn: r.get("alpn"),
            path: r.get("path"),
            public_key: r.get("public_key"),
            short_id: r.get("short_id"),
            method: r.try_get("method").ok().flatten(),
            service_name: r.try_get("service_name").ok().flatten(),
            server_key: r.try_get("server_key").ok().flatten(),
            host_header: r.try_get("host_header").ok().flatten(),
            options:{let mut v=r.get::<serde_json::Value,_>("options").as_object().cloned().unwrap_or_default();if let Some(t)=r.get::<Option<String>,_>("host_template"){v.insert("xray_template_body".into(),serde_json::json!(t));}serde_json::Value::Object(v)},
            uuid: vpn_uuid.to_string(),
        })
        .collect();

    Ok(SubData {
        external_squad:sn_sub::overrides::load_external(pool,client_id).await?,
        username: row.get("username"),
        status: row.get("status"),
        tariff: row.try_get("tariff").unwrap_or(None),
        expires_at: row.try_get("expires_at").unwrap_or(None),
        used: row
            .try_get::<Option<i64>, _>("traffic_used_bytes")
            .unwrap_or(None)
            .unwrap_or(0),
        limit: row.try_get::<Option<i64>, _>("traffic_limit_bytes").unwrap_or(None),
        device_limit: row
            .try_get::<Option<i32>, _>("device_limit")
            .unwrap_or(None)
            .unwrap_or(1),
        hosts,
    })
}

/// Заголовки, которые читают клиентские приложения: остаток трафика, срок
/// подписки, имя профиля, ссылка на поддержку и объявление.
///
/// Всё, что видит человек, берём из настроек панели. Раньше имя собиралось из
/// переменной окружения, а интервал был константой в коде — поменять их можно
/// было только пересборкой сервиса.
fn user_info_headers(d: &SubData, brand: &str, st: &SettingsView) -> HeaderMap {
    let mut h = HeaderMap::new();
    let total = d.limit.unwrap_or(0);
    let expire = d.expires_at.map(|e| e.timestamp()).unwrap_or(0);
    let value = format!(
        "upload=0; download={}; total={}; expire={}",
        d.used, total, expire
    );
    // Ответ подписки кэшировать нельзя ни клиенту, ни прокси по дороге.
    //
    // Без явного заголовка HTTP-клиенты кэшируют эвристически: приложение
    // продолжает показывать прежние локации и остаток трафика ещё долго
    // после того, как в панели всё поменяли. И это персональный ответ —
    // общий кэш на пути отдал бы конфиг одного клиента другому.
    if let Ok(v) = "no-store, no-cache, must-revalidate, max-age=0".parse() {
        h.insert(header::CACHE_CONTROL, v);
    }
    if let Ok(v) = "no-cache".parse() {
        h.insert("pragma", v);
    }

    if let Ok(v) = value.parse() {
        h.insert("subscription-userinfo", v);
    }

    let title = st
        .profile_title
        .clone()
        .unwrap_or_else(|| format!("{brand} - {}", d.username));
    if let Ok(v) = header_text(&title).parse() {
        h.insert("profile-title", v);
    }

    if let Ok(v) = st.update_interval_hours.to_string().parse() {
        h.insert("profile-update-interval", v);
    }

    // Приложение показывает это кнопкой «Поддержка» прямо в подписке.
    if let Some(url) = st.support_url.as_deref() {
        if let Ok(v) = header_url(url).parse() {
            h.insert("support-url", v);
        }
    }

    // Баннер в приложении.
    if let Some(text) = st.announce.as_deref() {
        if let Ok(v) = header_text(text).parse() {
            h.insert("announce", v);
        }
    }
    if let Some(link)=&st.happ_routing{if let Ok(v)=link.parse(){h.insert("routing",v);}}
    policy::apply_headers(&mut h,&st.response_headers,&d.username,&title);
    h
}

/// Готовит текст к отправке заголовком.
///
/// В HTTP-заголовке допустим только ASCII. Кириллицу и эмодзи часть клиентов
/// и промежуточных прокси режет молча — заголовок просто исчезает. Поэтому
/// всё, что вышло за ASCII, кодируем в base64 с префиксом: это соглашение
/// понимают клиентские приложения. Чистый ASCII отдаём как есть, иначе
/// приложения без поддержки префикса покажут человеку строку «base64:…».
/// Готовит ссылку к отправке заголовком.
///
/// Кодировать URL в base64 нельзя — приложение ждёт ссылку и откроет её как
/// есть. Поэтому не-ASCII байты экранируем процентами: получается обычный
/// валидный URL, который одинаково поймут и приложение, и браузер.
fn header_url(url: &str) -> String {
    if url.is_ascii() {
        return url.to_string();
    }
    url.bytes()
        .map(|b| {
            if b.is_ascii() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn header_text(value: &str) -> String {
    if value.is_ascii() {
        return value.to_string();
    }
    use base64::Engine as _;
    format!("base64:{}", base64::engine::general_purpose::STANDARD.encode(value))
}

/// QR-код ссылки подписки картинкой.
///
/// Нужен панели: рисовать «похожий на QR» узор нельзя — он не считается
/// камерой, а выглядит рабочим, и это обнаруживается уже у клиента.
async fn qr_image(State(st): State<SubState>, Path(short_id): Path<String>) -> Result<Response> {
    // Проверяем, что подписка существует: иначе по перебору коротких
    // идентификаторов можно было бы узнать, какие из них заняты.
    load(&st.source, &short_id).await?;

    let cfg = load_settings(&st).await;
    let base = cfg
        .page
        .title
        .as_deref()
        .map(|_| ())
        .map(|_| st.config.sub_public_url.clone())
        .unwrap_or_else(|| st.config.sub_public_url.clone());
    let url = format!("{}/{}", base.trim_end_matches('/'), short_id);

    Ok((
        [
            (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        page::qr_svg_public(&url),
    )
        .into_response())
}

async fn subscription(
    State(st): State<SubState>,
    Path(short_id): Path<String>,
    headers: HeaderMap,
    Query(language): Query<locale::LanguageQuery>,
) -> Result<Response> {
    let lang = locale::Language::for_request(&language, &headers);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut data = load(&st.source, &short_id).await?;
    let mut cfg = load_settings(&st).await;
    apply_external_settings(&mut cfg,&data.external_squad["settings"]);
    for host in &mut data.hosts {sn_sub::overrides::apply_host(host,&data.external_squad["hosts"]);}

    // Сначала правила администратора, и только потом встроенный список:
    // правило должно перекрывать умолчание, иначе настроить выдачу для
    // нового приложения было бы нечем.
    let rules=cached_rules(&st).await;
    let matched=matching_rule(&rules,&headers);
    let mut format=matched.and_then(rule_format).unwrap_or_else(||detect_format(ua));
    let mut template=matched.and_then(|r|r.template.clone());

    // Неизвестное приложение по умолчанию получает base64-список: его понимают
    // почти все. Тумблер в панели переключает такой случай на Xray JSON.
    if matched.is_none() && cfg.json_on_unknown_ua && format == Format::Base64 && !known_agent(ua) {
        format = Format::XrayJson;
    }

    let forced_template=template.is_some();
    if template.is_none(){template=data.external_squad["templates"][format_code(&format)].as_str().map(String::from);}
    let group_template=template.is_some();
    if template.is_none(){template=cfg.default_templates.get(format_code(&format)).cloned();}
    data.hosts.retain(|h|sn_sub::overrides::visible(h,format_code(&format)));
    if forced_template||group_template {for host in &mut data.hosts {if let Some(o)=host.options.as_object_mut(){o.remove("xray_template_body");}}}
    if data.hosts.iter().any(|h|h.options["shuffle"]==true) {
        let positions:Vec<_>=data.hosts.iter().enumerate().filter(|(_,h)|h.options["shuffle"]==true).map(|(i,_)|i).collect();
        let mut shuffled:Vec<_>=positions.iter().map(|i|data.hosts[*i].clone()).collect();shuffle_hosts(&mut shuffled,&short_id);
        for (i,h) in positions.into_iter().zip(shuffled){data.hosts[i]=h;}
    }

    // Без перемешивания все клиенты подключаются к первой локации списка,
    // и одна нода собирает всю нагрузку, пока остальные простаивают.
    if cfg.shuffle_hosts {
        shuffle_hosts(&mut data.hosts, &short_id);
    }

    // Запрос фиксируем всегда — по этому журналу ловят утёкшие ссылки.
    log_request(&st, &short_id, ua, &format!("{format:?}")).await;

    // Учёт устройства. Приложение представляется заголовками; без них
    // (браузер, curl) учитывать нечего — это не устройство клиента.
    let over_limit = if matched.is_some_and(|r|r.disable_hwid_check) {None} else {match read_device(&headers) {
        Some(dev) => match register_device(&st, &short_id, &dev).await {
            DeviceVerdict::OverLimit { used, limit } => Some((used, limit)),
            DeviceVerdict::Allow => None,
        },
        None => None,
    }};
    let unsupported=cfg.require_hwid && format!=Format::WebPage && read_device(&headers).is_none() && !matched.is_some_and(|r|r.disable_hwid_check);

    let brand = cfg.brand.clone().unwrap_or_else(|| st.config.brand_name.clone());
    let mut hdrs = user_info_headers(&data, &brand, &cfg);
    if format == Format::WebPage {
        hdrs.insert(header::CONTENT_LANGUAGE, lang.code().parse().unwrap());
        hdrs.insert(header::VARY, "User-Agent, Accept-Language, Cookie".parse().unwrap());
    }
    if cfg.username_header {
        if let Ok(v) = data.username.parse() {
            hdrs.insert("x-sn-username", v);
        }
    }

    let title=cfg.profile_title.clone().unwrap_or_else(||format!("{brand} - {}",data.username));
    if let Some(rule)=matched{policy::apply_headers(&mut hdrs,&rule.response_headers,&data.username,&title);}
    let status=match format{Format::Block=>Some(403),Format::NotFound=>Some(404),Format::Unavailable=>Some(451),_=>None};
    if let Some(status)=status{return Ok((axum::http::StatusCode::from_u16(status).unwrap(),hdrs,"Доступ к подписке ограничен правилом ответа").into_response())}
    if data.status != "active" || data.hosts.is_empty() || over_limit.is_some() || unsupported {
        let reason_key=if over_limit.is_some(){"devices"}else if unsupported{"unsupported"}else{match data.status.as_str(){"expired"=>"expired","limited"=>"limited","disabled"=>"disabled",_=>"no_hosts"}};
        let fallback=if let Some((used,limit))=over_limit{cfg.remark_devices(&data,used as i64,limit as i64)}else if unsupported{"Приложение не передало HWID. Используйте приложение с поддержкой идентификатора устройства.".into()}else{cfg.remark_for(&data)};
        let reasons:Vec<String>=cfg.remark_lists.get(reason_key).map(|v|v.iter().map(|s|fill_placeholders_dev(s,&data,over_limit.map(|(u,l)|(u as i64,l as i64)))).collect()).unwrap_or_else(||vec![fallback]);
        let reason=reasons.join("\n");
        if format == Format::WebPage {
            let apps=load_apps(&st).await;
            let sub_url=format!("{}/{}",st.config.sub_public_url.trim_end_matches('/'),short_id);
            let fallback_template = if over_limit.is_some() { cfg.remark_devices.as_str() } else if unsupported { "Приложение не передало HWID. Используйте приложение с поддержкой идентификатора устройства." } else { match data.status.as_str() { "expired" => &cfg.remark_expired, "limited" => &cfg.remark_limited, "disabled" => &cfg.remark_disabled, _ => &cfg.remark_no_hosts } };
            let templates = cfg.remark_lists.get(reason_key).map(|v| v.iter().map(String::as_str).collect::<Vec<_>>()).unwrap_or_else(|| vec![fallback_template]);
            let notice = templates.iter().map(|s| fill_placeholders_dev(lang.default_text(s), &data, over_limit.map(|(u,l)| (u as i64,l as i64)))).collect::<Vec<_>>().join("\n");
            return Ok((hdrs,Html(page::render_localized(&data,&brand,&sub_url,&apps,&cfg.page,&notice,lang))).into_response());
        }
        let stubs:Vec<_>=reasons.iter().map(|r|formats::stub_host(r)).collect();
        return Ok(render_hosts(format,&stubs,&reason,None,hdrs));
    }

    let title = cfg
        .profile_title
        .clone()
        .unwrap_or_else(|| format!("{brand} - {}", data.username));

    Ok(match format {
        Format::WebPage => {
            let apps = load_apps(&st).await;
            let sub_url = format!("{}/{}", st.config.sub_public_url.trim_end_matches('/'), short_id);
            let html = page::render_localized(&data, &brand, &sub_url, &apps, &cfg.page, "", lang);
            (hdrs, Html(html)).into_response()
        }
        _ => render_hosts(format, &data.hosts, &title, template.as_deref(), hdrs),
    })
}

/// Отдать список хостов в запрошенном формате.
///
/// Вынесено отдельно, потому что этим же путём уходит и заглушка с
/// причиной: она должна быть таким же валидным конфигом, как обычный
/// список локаций. Веб-страница сюда не попадает — у неё своя вёрстка.
fn render_hosts(
    format: Format,
    hosts: &[HostEntry],
    title: &str,
    template: Option<&str>,
    hdrs: HeaderMap,
) -> Response {
    let code=format_code(&format);
    if format==Format::XrayJson && hosts.iter().any(|h|h.options["xray_template_body"].is_string()) {
        let mut configs=Vec::new();
        for host in hosts {
            let selected=host.options["xray_template_body"].as_str().or(template);
            let rendered=selected.and_then(|t|policy::render_template(code,t,std::slice::from_ref(host),title).ok()).and_then(|s|serde_json::from_str::<serde_json::Value>(&s).ok()).unwrap_or_else(||formats::render_xray_json(std::slice::from_ref(host),title));
            match rendered {serde_json::Value::Array(items)=>configs.extend(items),value=>configs.push(value)}
        }
        return (hdrs,Json(configs)).into_response();
    }
    if let Some(template)=template{
        match policy::render_template(code,template,hosts,title){
            Ok(body)=>return (hdrs,[(header::CONTENT_TYPE,if matches!(code,"xray_json"|"singbox"){"application/json; charset=utf-8"}else if matches!(code,"mihomo"|"clash"|"stash"){"text/yaml; charset=utf-8"}else{"text/plain; charset=utf-8"})],body).into_response(),
            Err(e)=>tracing::error!(error=%e,"не удалось применить шаблон подписки; используется встроенный формат"),
        }
    }
    match format {
        Format::XrayJson=>(hdrs,Json(formats::render_xray_json(hosts,title))).into_response(),
        Format::SingBox=>(hdrs,Json(formats::render_singbox(hosts,title))).into_response(),
        Format::Clash|Format::Mihomo|Format::Stash=>(hdrs,[(header::CONTENT_TYPE,"text/yaml; charset=utf-8")],formats::render_clash(hosts,title)).into_response(),
        Format::Base64=>(hdrs,formats::render_base64(hosts)).into_response(),
        _=>(hdrs,formats::render_plain(hosts)).into_response(),
    }
}

/// Знаем ли мы это приложение в лицо. Нужно, чтобы отличить «клиент просит
/// base64, потому что он v2rayNG» от «клиент неизвестен, отдали base64 наугад».
fn known_agent(ua: &str) -> bool {
    let ua = ua.to_lowercase();
    ["happ", "streisand", "v2box", "nekobox", "v2rayng", "shadowrocket",
     "flclash", "mihomo", "stash", "clash", "sing-box", "hiddify", "v2rayn"]
        .iter()
        .any(|a| ua.contains(a))
}

/// Перемешивает локации детерминированно — свой порядок у каждого клиента,
/// но один и тот же между запросами. Случайный порядок на каждом обновлении
/// заставлял бы приложение считать, что список серверов сменился.
fn shuffle_hosts(hosts: &mut [HostEntry], short_id: &str) {
    let seed = short_id.bytes().fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
    let n = hosts.len();
    for i in (1..n).rev() {
        // Перемешивание Фишера — Йетса на простом линейном конгруэнтном ряде:
        // криптостойкость тут не нужна, нужна повторяемость.
        let r = seed.wrapping_mul(6364136223846793005).wrapping_add(i as u64);
        hosts.swap(i, (r >> 33) as usize % (i + 1));
    }
}

/// Настройки подписки, приведённые к готовым к употреблению значениям.
#[derive(Default, Clone)]
struct SettingsView {
    profile_title: Option<String>,
    update_interval_hours: i64,
    support_url: Option<String>,
    announce: Option<String>,
    happ_routing: Option<String>,
    response_headers: Vec<policy::ResponseHeader>,
    remark_lists: std::collections::HashMap<String,Vec<String>>,
    default_templates: std::collections::HashMap<String,String>,
    require_hwid: bool,
    /// Название бренда. У вынесенной сабки своего нет — приходит из панели.
    brand: Option<String>,
    remark_expired: String,
    remark_limited: String,
    remark_disabled: String,
    remark_no_hosts: String,
    /// Лимит устройств исчерпан. Отдельно от прочих: причина не в
    /// подписке, а в том, сколько устройств уже привязано.
    remark_devices: String,
    json_on_unknown_ua: bool,
    username_header: bool,
    shuffle_hosts: bool,
    page: page::PageSettings,
}

impl SettingsView {
    /// Что клиент увидит списком локаций вместо конфигов.
    fn remark_for(&self, d: &SubData) -> String {
        let tpl = match d.status.as_str() {
            "expired" => &self.remark_expired,
            "limited" => &self.remark_limited,
            "disabled" => &self.remark_disabled,
            _ => &self.remark_no_hosts,
        };
        fill_placeholders(tpl, d)
    }

    /// Что увидит человек, у которого кончились свободные устройства.
    fn remark_devices(&self, d: &SubData, used: i64, limit: i64) -> String {
        fill_placeholders_dev(&self.remark_devices, d, Some((used, limit)))
    }
}

/// Подставляет {date}, {days} и {tariff}. Неизвестные плейсхолдеры оставляем
/// как есть: так опечатку в шаблоне видно сразу, а не в виде пустого места.
fn fill_placeholders(tpl: &str, d: &SubData) -> String {
    fill_placeholders_dev(tpl, d, None)
}

/// То же, но с числами устройств: они известны только на месте проверки
/// лимита, а не из данных подписки.
fn fill_placeholders_dev(tpl: &str, d: &SubData, dev: Option<(i64, i64)>) -> String {
    let date = d
        .expires_at
        .map(|e| e.format("%d.%m.%Y").to_string())
        .unwrap_or_default();
    let days = d
        .expires_at
        .map(|e| (e - Utc::now()).num_days().abs().to_string())
        .unwrap_or_default();
    let (used, limit) = dev.unwrap_or((0, d.device_limit as i64));
    tpl.replace("{date}", &date)
        .replace("{days}", &days)
        .replace("{tariff}", d.tariff.as_deref().unwrap_or(""))
        .replace("{used}", &used.to_string())
        .replace("{limit}", &limit.to_string())
}

/// Подставляет список серверов в шаблон администратора.
///
/// Шаблон меняет только обёртку — DNS, маршрутизацию, входящие порты, —
/// а сами серверы всегда собирает сервис: их формат зависит от протокола
/// и ключей, и править его руками означало бы ломать подключение.
///
/// `None` — шаблона нет, отдаём сгенерированное как есть.
#[cfg(test)]
fn apply_template(template: Option<&str>, servers: &serde_json::Value, title: &str) -> Option<String> {
    let tpl = template?.trim();
    if tpl.is_empty() {
        return None;
    }
    // Шаблон без метки {{SERVERS}} отдал бы клиенту конфиг без единой
    // локации: приложение подключится «успешно» и никуда не пойдёт.
    // Панель такой шаблон сохранить не даёт, но в базе могли остаться
    // старые — лучше вернуть сгенерированное, чем пустоту.
    if !tpl.contains("{{SERVERS}}") {
        tracing::warn!("шаблон без {{SERVERS}} пропущен: отдаём конфиг как есть");
        return None;
    }
    let servers_json = serde_json::to_string(servers).ok()?;
    Some(
        tpl.replace("{{SERVERS}}", &servers_json)
            .replace("{{TITLE}}", title),
    )
}

/// Правила ответов из панели.
///
/// Их держат в базе, а не в коде: клиентских приложений много, список
/// растёт быстрее релизов, и новое приложение должно подключаться
/// правилом, без пересборки сервиса.
async fn fetch_rules(st: &SubState) -> Vec<Rule> {
    match &st.source {
        Source::Db(pool) => sqlx::query(
            "SELECT r.ua_pattern, r.action, t.body AS template, t.code AS template_code, r.conditions, r.operator, r.response_headers, r.disable_hwid_check
               FROM response_rules r
               LEFT JOIN subscription_templates t ON t.id = r.template_id
              WHERE r.is_active ORDER BY r.sort_order, r.id",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| Rule {
            pattern: r.get("ua_pattern"),
            action: r.get("action"),
            template: r.try_get("template").ok().flatten(),
            template_code: r.try_get("template_code").ok().flatten(),
            conditions:r.get::<Option<serde_json::Value>,_>("conditions").and_then(|v|serde_json::from_value(v).ok()),
            operator:r.get("operator"),response_headers:serde_json::from_value(r.get("response_headers")).unwrap_or_default(),disable_hwid_check:r.get("disable_hwid_check"),
        })
        .collect(),
        Source::Api { panel_url, token, http } => {
            let res = http
                .get(format!("{panel_url}/api/sub/rules-effective"))
                .header("x-service-token", token)
                .send()
                .await;
            let arr: Vec<serde_json::Value> = match res {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            arr.iter()
                .map(|v| Rule {
                    pattern: v["ua_pattern"].as_str().unwrap_or_default().to_string(),
                    action: v["action"].as_str().unwrap_or("base64").to_string(),
                    template: v["template"].as_str().map(String::from),
                    template_code:v["template_code"].as_str().map(String::from),
                    conditions:serde_json::from_value(v["conditions"].clone()).ok(),
                    operator:v["operator"].as_str().unwrap_or("AND").into(),
                    response_headers:serde_json::from_value(v["response_headers"].clone()).unwrap_or_default(),
                    disable_hwid_check:v["disable_hwid_check"].as_bool().unwrap_or(false),
                })
                .collect()
        }
    }
}

/// Формат по правилам администратора. `None` — ни одно не подошло,
/// решает встроенный список известных приложений.
fn format_code(f:&Format)->&'static str{match f{Format::XrayJson=>"xray_json",Format::Mihomo=>"mihomo",Format::Stash=>"stash",Format::Clash=>"clash",Format::SingBox=>"singbox",Format::Base64=>"base64",Format::WebPage=>"web_page",Format::Block=>"block",Format::NotFound=>"not_found",Format::Unavailable=>"unavailable",_=>"plain"}}
fn rule_format(r:&Rule)->Option<Format>{
    let action=if r.action=="template"{r.template_code.as_deref().unwrap_or_else(||if r.template.as_deref().is_some_and(|s|s.trim_start().starts_with('{')||s.trim_start().starts_with('[')){"xray_json"}else{"clash"})}else{&r.action};
    match action{"xray_json"=>Some(Format::XrayJson),"mihomo"=>Some(Format::Mihomo),"stash"=>Some(Format::Stash),"clash"=>Some(Format::Clash),"singbox"=>Some(Format::SingBox),"base64"=>Some(Format::Base64),"plain"=>Some(Format::Plain),"web_page"=>Some(Format::WebPage),"block"=>Some(Format::Block),"not_found"=>Some(Format::NotFound),"unavailable"=>Some(Format::Unavailable),_=>None}
}
fn matching_rule<'a>(rules:&'a [Rule],headers:&HeaderMap)->Option<&'a Rule>{
    let ua=headers.get("user-agent").and_then(|v|v.to_str().ok()).unwrap_or("").to_lowercase();
    rules.iter().find(|r|rule_format(r).is_some() && if let Some(conditions)=&r.conditions{policy::conditions_match(&r.operator,conditions,headers)}else{r.pattern.split('|').map(|s|s.trim().to_lowercase()).filter(|s|!s.is_empty()).any(|p|ua.contains(&p))})
}
#[cfg(test)]
fn match_rule(rules:&[Rule],ua:&str)->Option<(Format,Option<String>)>{let mut headers=HeaderMap::new();headers.insert("user-agent",ua.parse().ok()?);let r=matching_rule(rules,&headers)?;Some((rule_format(r)?,r.template.clone()))}

/// Сырые настройки из панели — из базы напрямую или через API.
async fn fetch_settings_map(st: &SubState) -> std::collections::HashMap<String, serde_json::Value> {
    match &st.source {
        Source::Db(pool) => {
            let rows = sqlx::query("SELECT key, value FROM settings
                  WHERE key LIKE 'subscription.%' OR key LIKE 'brand.%' OR key='cabinet.config'")
                .fetch_all(pool)
                .await
                .unwrap_or_default();
            rows.iter()
                .map(|r| {let key=r.get::<String,_>("key");let value=r.get::<serde_json::Value,_>("value");if key=="cabinet.config" {("subscription.page_branding".into(),serde_json::to_value(sn_core::customer_brand::CustomerBrand::from_config(&value)).unwrap_or_default())}else{(key,value)}})
                .collect()
        }
        Source::Api { panel_url, token, http } => {
            let res = http
                .get(format!("{panel_url}/api/sub/settings"))
                .header("x-service-token", token)
                .send()
                .await;
            match res {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(_) => Default::default(),
            }
        }
    }
}

/// Настройки с коротким кэшем: их читает каждый запрос подписки.
async fn load_settings(st: &SubState) -> SettingsView {
    {
        let cache = st.settings.read().await;
        if let Some(at) = cache.fetched_at {
            if at.elapsed() < SETTINGS_TTL {
                return build_view(&cache.map);
            }
        }
    }

    let mut map = fetch_settings_map(st).await;
    let defaults=match &st.source{
        Source::Db(pool)=>sqlx::query("SELECT code,body FROM subscription_templates WHERE is_default").fetch_all(pool).await.unwrap_or_default().iter().map(|r|(r.get::<String,_>("code"),serde_json::json!(r.get::<String,_>("body")))).collect::<serde_json::Map<String,serde_json::Value>>(),
        Source::Api{panel_url,token,http}=>match http.get(format!("{panel_url}/api/sub/templates-effective")).header("x-service-token",token).send().await{Ok(r)=>r.json().await.unwrap_or_default(),Err(_)=>Default::default()},
    };
    map.insert("subscription.default_templates".into(),serde_json::Value::Object(defaults));
    let rules = fetch_rules(st).await;
    let view = build_view(&map);
    let mut cache = st.settings.write().await;
    cache.map = map;
    cache.rules = rules;
    cache.fetched_at = Some(std::time::Instant::now());
    view
}

/// Правила из кэша — тот же срок жизни, что у настроек.
async fn cached_rules(st: &SubState) -> Vec<Rule> {
    st.settings.read().await.rules.clone()
}

fn build_view(map: &std::collections::HashMap<String, serde_json::Value>) -> SettingsView {
    // Пустая строка — это «не задано», а не значение: иначе приложение получит
    // пустой заголовок и покажет пустую кнопку поддержки.
    let text = |key: &str| {
        map.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    // Тумблер мог сохраниться булевым или строкой "true" — принимаем оба.
    let flag = |key: &str, default: bool| {
        map.get(key)
            .and_then(|v| v.as_bool().or_else(|| v.as_str()?.parse().ok()))
            .unwrap_or(default)
    };

    SettingsView {
        profile_title: text("subscription.profile_title"),
        // Число могли сохранить и строкой из формы — принимаем оба вида.
        // Ноль означал бы «обновляться непрерывно», поэтому его отсекаем.
        update_interval_hours: map
            .get("subscription.update_interval_hours")
            .and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
            .filter(|h| *h > 0)
            .unwrap_or(12),
        support_url: text("subscription.support_url"),
        announce: text("subscription.announce"),
        happ_routing:text("subscription.happ_routing"),
        response_headers:map.get("subscription.response_headers").and_then(|v|serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
        require_hwid:flag("subscription.require_hwid",false),
        remark_lists:["expired","limited","disabled","devices","unsupported","no_hosts"].into_iter().filter_map(|key|map.get(&format!("subscription.remark_{key}")).and_then(|v|v.as_array()).map(|a|(key.to_string(),a.iter().filter_map(|v|v.as_str()).filter(|s|!s.trim().is_empty()).map(String::from).collect()))).collect(),
        default_templates:map.get("subscription.default_templates").and_then(|v|serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
        brand: text("brand.name"),
        remark_expired: text("subscription.remark_expired")
            .unwrap_or_else(|| "⛔ Подписка истекла {date}".into()),
        remark_limited: text("subscription.remark_limited")
            .unwrap_or_else(|| "📉 Трафик закончился".into()),
        remark_disabled: text("subscription.remark_disabled")
            .unwrap_or_else(|| "🚫 Доступ приостановлен".into()),
        remark_no_hosts: text("subscription.remark_no_hosts")
            .unwrap_or_else(|| "Нет доступных локаций".into()),
        remark_devices: text("subscription.remark_devices").unwrap_or_else(|| {
            "Занято устройств: {used} из {limit}. Отключите лишнее и обновите подписку".into()
        }),
        json_on_unknown_ua: flag("subscription.json_on_unknown_ua", false),
        username_header: flag("subscription.username_header", true),
        shuffle_hosts: flag("subscription.shuffle_hosts", false),
        page: page::PageSettings {
            branding: sn_core::customer_brand::CustomerBrand::from_config(map.get("subscription.page_branding").unwrap_or(&serde_json::Value::Null)),
            title: text("subscription.title"),
            support_url: text("subscription.support_url"),
            support_text: text("subscription.support_text"),
            bot_url: text("subscription.bot_url"),
            footer_note: text("subscription.footer_note"),
        },
    }
}

fn apply_external_settings(cfg:&mut SettingsView, settings:&serde_json::Value){
    let Some(map)=settings.as_object() else{return};
    for (key,field) in [("subscription.profile_title",&mut cfg.profile_title),("subscription.support_url",&mut cfg.support_url),("subscription.announce",&mut cfg.announce),("subscription.happ_routing",&mut cfg.happ_routing)]{
        if let Some(value)=map.get(key).and_then(|v|v.as_str()){*field=Some(value.to_owned());}
    }
    for (key,field) in [("subscription.title",&mut cfg.page.title),("subscription.support_url",&mut cfg.page.support_url),("subscription.support_text",&mut cfg.page.support_text),("subscription.bot_url",&mut cfg.page.bot_url),("subscription.footer_note",&mut cfg.page.footer_note)]{
        if let Some(value)=map.get(key).and_then(|v|v.as_str()){*field=Some(value.to_owned());}
    }
    for (key,field) in [("subscription.require_hwid",&mut cfg.require_hwid),("subscription.json_on_unknown_ua",&mut cfg.json_on_unknown_ua),("subscription.username_header",&mut cfg.username_header),("subscription.shuffle_hosts",&mut cfg.shuffle_hosts)]{
        if let Some(value)=map.get(key).and_then(|v|v.as_bool()){*field=value;}
    }
    if let Some(value)=map.get("subscription.update_interval_hours").and_then(|v|v.as_i64()){cfg.update_interval_hours=value;}
    if let Some(value)=map.get("subscription.response_headers"){cfg.response_headers=serde_json::from_value(value.clone()).unwrap_or_default();}
    for (key,field) in [("expired",&mut cfg.remark_expired),("limited",&mut cfg.remark_limited),("disabled",&mut cfg.remark_disabled),("devices",&mut cfg.remark_devices),("no_hosts",&mut cfg.remark_no_hosts)]{
        if let Some(value)=map.get(&format!("subscription.remark_{key}")) {
            let lines=if let Some(s)=value.as_str(){vec![s.to_string()]}else{value.as_array().into_iter().flatten().filter_map(|v|v.as_str().map(String::from)).collect()};
            *field=lines.join("\n");cfg.remark_lists.insert(key.into(),lines);
        }
    }
    if let Some(value)=map.get("subscription.remark_unsupported"){let lines=if let Some(s)=value.as_str(){vec![s.to_string()]}else{value.as_array().into_iter().flatten().filter_map(|v|v.as_str().map(String::from)).collect()};cfg.remark_lists.insert("unsupported".into(),lines);}
}

/// Приложения для страницы подписки.
async fn load_apps(st: &SubState) -> Vec<page::App> {
    match &st.source {
        Source::Db(pool) => {
            let rows = sqlx::query(
                "SELECT platform, name, deeplink, store_url, guide, sort_order,
                        icon_url, icon_svg
                   FROM subscription_page_apps
                  WHERE is_active ORDER BY platform, sort_order",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            rows.iter()
                .map(|r| page::App {
                    name: r.get("name"),
                    platform: r.get("platform"),
                    deeplink: r.get("deeplink"),
                    store_url: r.get("store_url"),
                    guide: r.get("guide"),
                    // Первое приложение платформы помечаем рекомендуемым:
                    // человеку нужен один очевидный вариант, а не выбор из пяти.
                    recommended: r.get::<i32, _>("sort_order") <= 1,
                    icon_svg: r.get("icon_svg"),
                    icon_url: r.get("icon_url"),
                })
                .collect()
        }
        Source::Api { panel_url, token, http } => {
            let res = http
                .get(format!("{panel_url}/api/sub/apps"))
                .header("x-service-token", token)
                .send()
                .await;
            let Ok(res) = res else { return Vec::new() };
            let Ok(v) = res.json::<serde_json::Value>().await else { return Vec::new() };
            v.as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|a| page::App {
                            name: a["name"].as_str().unwrap_or_default().to_string(),
                            platform: a["platform"].as_str().unwrap_or_default().to_string(),
                            deeplink: a["deeplink"].as_str().map(String::from),
                            store_url: a["store_url"].as_str().map(String::from),
                            guide: a["guide"].as_str().map(String::from),
                            recommended: a["sort_order"].as_i64().unwrap_or(99) <= 1,
                            icon_svg: a["icon_svg"].as_str().map(String::from),
                            icon_url: a["icon_url"].as_str().map(String::from),
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
    }
}

/// Устройство, с которого пришли за подпиской.
///
/// Клиентские приложения представляются набором заголовков: без них
/// нельзя ни показать список устройств, ни ограничить их число — а
/// именно этим лимит устройств в тарифе и должен работать.
#[derive(Debug, Default, Clone)]
pub struct DeviceInfo {
    pub hwid: String,
    pub platform: Option<String>,
    pub model: Option<String>,
    pub app_version: Option<String>,
}

/// Разбор заголовков устройства.
///
/// Имена взяты те, которыми пользуются существующие клиенты (`x-hwid`,
/// `x-device-os`, `x-ver-os`, `x-device-model`): панель должна работать
/// с теми приложениями, которые у людей уже стоят, а не требовать своих.
/// Дополнительно принимаем `x-device-id` и `x-app-version` — так
/// подписываются наши сборки.
fn read_device(headers: &HeaderMap) -> Option<DeviceInfo> {
    let get = |names: &[&str]| -> Option<String> {
        for n in names {
            if let Some(v) = headers.get(*n).and_then(|v| v.to_str().ok()) {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
        None
    };

    let hwid = get(&["x-hwid", "x-device-id", "x-device-hwid"])?;
    // Чужой длинный идентификатор в базу целиком не кладём: столбец
    // текстовый, но неограниченная строка из заголовка — это чужой ввод.
    let hwid: String = hwid.chars().take(128).collect();

    let platform = get(&["x-device-os", "x-platform", "x-os"]);
    let os_ver = get(&["x-ver-os", "x-os-version"]);
    // «iOS» и «18.4» по отдельности бесполезны, вместе — понятны.
    let platform = match (platform, os_ver) {
        (Some(p), Some(v)) => Some(format!("{p} {v}")),
        (p, v) => p.or(v),
    };

    Some(DeviceInfo {
        hwid,
        platform: platform.map(|s| s.chars().take(64).collect()),
        model: get(&["x-device-model", "x-model"]).map(|s| s.chars().take(64).collect()),
        app_version: get(&["x-app-version", "x-ver-app"]).map(|s| s.chars().take(32).collect()),
    })
}

/// Что делать с запросом после учёта устройства.
#[derive(Debug, PartialEq)]
pub enum DeviceVerdict {
    /// Устройство учтено (или заголовков не было) — отдаём конфиги.
    Allow,
    /// Новое устройство сверх лимита тарифа.
    OverLimit { used: i64, limit: i32 },
}

/// Учёт устройства и проверка лимита.
///
/// Лимит проверяем до вставки и только для нового HWID: уже
/// зарегистрированное устройство должно продолжать работать, даже если
/// лимит потом уменьшили — иначе человек внезапно теряет доступ на
/// телефоне, которым пользовался вчера.
async fn register_device(st: &SubState, short_id: &str, dev: &DeviceInfo) -> DeviceVerdict {
    match &st.source {
        Source::Db(pool) => {
            let row = sqlx::query(
                "SELECT c.id,
                        COALESCE(s.device_limit, 0) AS device_limit,
                        (SELECT count(*) FROM devices d WHERE d.client_id = c.id) AS used,
                        EXISTS (SELECT 1 FROM devices d
                                 WHERE d.client_id = c.id AND d.hwid = $2) AS known
                   FROM clients c
                   LEFT JOIN subscriptions s ON s.client_id = c.id AND s.is_current
                  WHERE c.short_id = $1 AND c.deleted_at IS NULL",
            )
            .bind(short_id)
            .bind(&dev.hwid)
            .fetch_optional(pool)
            .await;

            let Ok(Some(row)) = row else { return DeviceVerdict::Allow };
            let client_id: i64 = row.get("id");
            let limit: i32 = row.get("device_limit");
            let used: i64 = row.get("used");
            let known: bool = row.get("known");

            if !known && limit > 0 && used >= limit as i64 {
                return DeviceVerdict::OverLimit { used, limit };
            }

            let _ = sqlx::query(
                "INSERT INTO devices (client_id, hwid, platform, model, app_version)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (client_id, hwid) DO UPDATE
                    SET last_seen_at = now(),
                        platform    = COALESCE(EXCLUDED.platform, devices.platform),
                        model       = COALESCE(EXCLUDED.model, devices.model),
                        app_version = COALESCE(EXCLUDED.app_version, devices.app_version)",
            )
            .bind(client_id)
            .bind(&dev.hwid)
            .bind(&dev.platform)
            .bind(&dev.model)
            .bind(&dev.app_version)
            .execute(pool)
            .await;

            DeviceVerdict::Allow
        }
        Source::Api { panel_url, token, http } => {
            let res = http
                .post(format!("{panel_url}/api/sub/{short_id}/device"))
                .header("x-service-token", token)
                .json(&serde_json::json!({
                    "hwid": dev.hwid,
                    "platform": dev.platform,
                    "model": dev.model,
                    "app_version": dev.app_version,
                }))
                .send()
                .await;
            // Панель недоступна — отдаём подписку. Учёт устройств не повод
            // оставить человека без интернета.
            match res {
                Ok(r) => {
                    let v = r.json::<serde_json::Value>().await.unwrap_or_default();
                    if v["over_limit"].as_bool().unwrap_or(false) {
                        DeviceVerdict::OverLimit {
                            used: v["used"].as_i64().unwrap_or(0),
                            limit: v["limit"].as_i64().unwrap_or(0) as i32,
                        }
                    } else {
                        DeviceVerdict::Allow
                    }
                }
                Err(_) => DeviceVerdict::Allow,
            }
        }
    }
}

/// Журнал запросов. В режиме API пишем через панель: у сабки на отдельном
/// сервере нет доступа к базе.
async fn log_request(st: &SubState, short_id: &str, ua: &str, code: &str) {
    match &st.source {
        Source::Db(pool) => {
            let _ = sqlx::query(
                "INSERT INTO subscription_requests (client_id, user_agent, response_code)
                 SELECT id, $2, $3 FROM clients WHERE short_id = $1",
            )
            .bind(short_id)
            .bind(ua)
            .bind(code)
            .execute(pool)
            .await;
        }
        Source::Api { panel_url, token, http } => {
            // Журнал не критичен: если панель недоступна, подписка всё равно
            // должна отдаться. Поэтому ошибку глотаем.
            let _ = http
                .post(format!("{panel_url}/api/sub/{short_id}/request"))
                .header("x-service-token", token)
                .json(&serde_json::json!({ "user_agent": ua, "response_code": code }))
                .send()
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn клиентские_приложения_получают_свой_формат() {
        assert_eq!(detect_format("Happ/3.5.0 (iPhone; iOS 18.5)"), Format::XrayJson);
        assert_eq!(detect_format("Streisand/1.2"), Format::XrayJson);
        assert_eq!(detect_format("v2rayNG/1.8.5"), Format::Base64);
        assert_eq!(detect_format("Shadowrocket/2.2"), Format::Base64);
        assert_eq!(detect_format("FlClash/0.8.80"), Format::Mihomo);
        assert_eq!(detect_format("mihomo/1.18"), Format::Mihomo);
        assert_eq!(detect_format("Stash/2.5"), Format::Stash);
        assert_eq!(detect_format("sing-box/1.9"), Format::SingBox);
        assert_eq!(detect_format("Hiddify/2.0"), Format::SingBox);
    }

    #[test]
    fn браузер_получает_страницу_а_curl_ссылки() {
        assert_eq!(
            detect_format("Mozilla/5.0 (Macintosh) Safari/605.1"),
            Format::WebPage
        );
        assert_eq!(detect_format("curl/8.4.0"), Format::Plain);
        assert_eq!(detect_format(""), Format::Plain);
    }

    #[test]
    fn клиент_на_основе_chromium_не_получает_html() {
        // Обёртки на Chromium несут «Mozilla» в UA — правило клиента
        // обязано сработать раньше правила браузера.
        assert_eq!(
            detect_format("Mozilla/5.0 FlClash/0.8.80"),
            Format::Mihomo,
            "клиентское приложение важнее подстроки Mozilla"
        );
    }

    #[test]
    fn пустая_настройка_это_не_значение() {
        // Пустая строка в поле формы означает «не задано». Если пропустить её
        // дальше, приложение получит пустой заголовок и покажет, например,
        // кнопку поддержки, ведущую в никуда.
        let map = [
            ("subscription.profile_title".to_string(), serde_json::json!("")),
            ("subscription.support_url".to_string(), serde_json::json!("   ")),
            ("subscription.announce".to_string(), serde_json::json!("")),
        ]
        .into_iter()
        .collect();

        let v = build_view(&map);
        assert!(v.profile_title.is_none());
        assert!(v.support_url.is_none());
        assert!(v.announce.is_none());
    }

    #[test]
    fn интервал_принимает_число_и_строку() {
        // Форма в панели отправляет то число, то строку — зависит от поля ввода.
        for raw in [serde_json::json!(6), serde_json::json!("6")] {
            let map = [("subscription.update_interval_hours".to_string(), raw)]
                .into_iter()
                .collect();
            assert_eq!(build_view(&map).update_interval_hours, 6);
        }
        // Ноль означал бы «перечитывать непрерывно» — откатываемся к разумному.
        let map = [(
            "subscription.update_interval_hours".to_string(),
            serde_json::json!(0),
        )]
        .into_iter()
        .collect();
        assert_eq!(build_view(&map).update_interval_hours, 12);
        assert_eq!(build_view(&Default::default()).update_interval_hours, 12);
    }

    #[test]
    fn кириллица_в_заголовках_кодируется() {
        // HTTP-заголовок — latin-1. Кириллица и эмодзи в нём недопустимы:
        // без кодирования заголовок молча не вставится, и баннер пропадёт.
        let data = SubData {
            external_squad:serde_json::json!({}),
            username: "user".into(),
            status: "active".into(),
            used: 0,
            limit: None,
            expires_at: None,
            device_limit: 1,
            tariff: None,
            hosts: vec![],
        };
        let view = SettingsView {
            announce: Some("⚡ Новая нода в Польше".into()),
            update_interval_hours: 8,
            support_url: Some("https://t.me/support".into()),
            profile_title: Some("MYVPN".into()),
            ..Default::default()
        };

        let h = user_info_headers(&data, "BRAND", &view);
        // Латиница помещается в заголовок как есть — кодировать её значит
        // показать «base64:…» в приложениях, не знающих про префикс.
        assert_eq!(h["profile-title"], "MYVPN");
        assert_eq!(h["profile-update-interval"], "8");
        assert_eq!(h["support-url"], "https://t.me/support");
        // Ссылка с кириллицей остаётся ссылкой, а не превращается в base64.
        assert_eq!(header_url("https://t.me/бот"), "https://t.me/%D0%B1%D0%BE%D1%82");

        let announce = h["announce"].to_str().unwrap();
        let payload = announce.strip_prefix("base64:").expect("должен быть base64");
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD.decode(payload).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "⚡ Новая нода в Польше");
    }

    #[test]
    fn кириллица_в_имени_профиля_кодируется() {
        let map = [(
            "subscription.profile_title".to_string(),
            serde_json::json!("МОЙ VPN"),
        )]
        .into_iter()
        .collect();
        assert_eq!(header_text("МОЙ VPN"), "base64:0JzQntCZIFZQTg==");
        assert!(build_view(&map).profile_title.is_some());
    }

    #[test]
    fn подписка_запрещена_к_кэшированию() {
        // Персональный ответ, который к тому же меняется при каждой правке
        // в панели. Кэш у клиента = «в панели поменял, в приложении старое»,
        // кэш на прокси = чужой конфиг чужому человеку.
        let data = SubData {
            external_squad:serde_json::json!({}),
            username: "u".into(),
            status: "active".into(),
            used: 0,
            limit: None,
            expires_at: None,
            device_limit: 1,
            tariff: None,
            hosts: vec![],
        };
        let h = user_info_headers(&data, "B", &SettingsView::default());
        let cc = h[header::CACHE_CONTROL].to_str().unwrap();
        assert!(cc.contains("no-store"), "нет no-store: {cc}");
        assert!(cc.contains("max-age=0"), "нет max-age=0: {cc}");
    }

    #[test]
    fn имя_профиля_по_умолчанию_из_бренда() {
        let data = SubData {
            external_squad:serde_json::json!({}),
            username: "vasya".into(),
            status: "active".into(),
            used: 0,
            limit: None,
            expires_at: None,
            device_limit: 1,
            tariff: None,
            hosts: vec![],
        };
        let h = user_info_headers(&data, "BRAND", &SettingsView::default());
        assert_eq!(h["profile-title"], "BRAND - vasya");
    }

    #[test]
    fn бренд_приходит_из_панели() {
        // У сабки на отдельном сервере нет ни базы, ни своих настроек:
        // всё, что видит человек, должно приезжать из панели.
        let map = [("brand.name".to_string(), serde_json::json!("МОЙ VPN"))]
            .into_iter()
            .collect();
        assert_eq!(build_view(&map).brand.as_deref(), Some("МОЙ VPN"));
        assert!(build_view(&Default::default()).brand.is_none());
    }

    #[test]
    fn правило_перекрывает_встроенный_список() {
        // Смысл правил в том, чтобы подключить новое приложение без
        // пересборки. Если встроенный список выигрывает, правила бесполезны.
        let rules = vec![Rule {
            pattern: "happ".into(),
            action: "clash".into(),
            template: None,
                ..Default::default()
        }];
        assert_eq!(detect_format("Happ/3.5"), Format::XrayJson);
        assert_eq!(match_rule(&rules, "Happ/3.5").unwrap().0, Format::Clash);
    }

    #[test]
    fn правило_ищет_любую_подстроку_образца() {
        let rules = vec![Rule {
            pattern: "clash | mihomo|stash".into(),
            action: "clash".into(),
            template: None,
                ..Default::default()
        }];
        for ua in ["FlClash/1.0", "mihomo/1.18", "Stash/2.5"] {
            assert!(match_rule(&rules, ua).is_some(), "не сработало на {ua}");
        }
        assert!(match_rule(&rules, "Happ/3.5").is_none());
    }

    #[test]
    fn шаблон_подставляет_серверы_и_имя() {
        let servers = serde_json::json!([{ "tag": "proxy" }]);
        let out = apply_template(
            Some(r#"{"remarks":"{{TITLE}}","list":{{SERVERS}}}"#),
            &servers,
            "МОЙ VPN",
        )
        .expect("шаблон должен примениться");

        let v: serde_json::Value = serde_json::from_str(&out).expect("должен остаться валидным JSON");
        assert_eq!(v["remarks"], "МОЙ VPN");
        assert_eq!(v["list"][0]["tag"], "proxy");
    }

    #[test]
    fn шаблон_без_метки_серверов_игнорируется() {
        // Такой шаблон выдал бы клиенту конфиг без локаций — это хуже,
        // чем проигнорировать настройку администратора.
        let servers = serde_json::json!([{ "tag": "proxy" }]);
        assert!(apply_template(Some(r#"{"outbounds":[]}"#), &servers, "T").is_none());
    }

    #[test]
    fn пустой_шаблон_не_подменяет_ответ() {
        // Иначе клиент получил бы пустое тело вместо конфига.
        let servers = serde_json::json!([]);
        assert!(apply_template(None, &servers, "T").is_none());
        assert!(apply_template(Some("   "), &servers, "T").is_none());
    }

    #[test]
    fn ремарка_подставляет_плейсхолдеры() {
        let d = SubData {
            external_squad:serde_json::json!({}),
            username: "u".into(),
            status: "expired".into(),
            used: 0,
            limit: None,
            expires_at: Some(Utc::now() - chrono::Duration::days(3)),
            device_limit: 1,
            tariff: Some("PRO".into()),
            hosts: vec![],
        };
        let mut v = SettingsView::default();
        v.remark_expired = "Тариф {tariff} истёк {date}, прошло {days} дн".into();

        let out = v.remark_for(&d);
        assert!(out.contains("PRO"), "{out}");
        assert!(out.contains("3 дн"), "{out}");
        assert!(!out.contains('{'), "плейсхолдер не подставлен: {out}");
    }

    #[test]
    fn порядок_локаций_стабилен_между_запросами() {
        // Свой порядок у каждого клиента, но одинаковый при каждом обновлении:
        // иначе приложение решит, что список серверов сменился, и потеряет
        // выбранную человеком локацию.
        let mut a: Vec<HostEntry> = (0..6).map(|i| host_named(&format!("h{i}"))).collect();
        let mut b = a.clone();
        shuffle_hosts(&mut a, "abc123");
        shuffle_hosts(&mut b, "abc123");
        assert_eq!(names(&a), names(&b));

        let mut c = a.clone();
        shuffle_hosts(&mut c, "xyz999");
        assert_ne!(names(&a), names(&c), "у разных клиентов должен быть разный порядок");

        let mut sorted = names(&a);
        sorted.sort();
        assert_eq!(sorted, vec!["h0", "h1", "h2", "h3", "h4", "h5"], "локация потерялась");
    }

    fn names(h: &[HostEntry]) -> Vec<String> {
        h.iter().map(|x| x.remark.clone()).collect()
    }

    fn host_named(name: &str) -> HostEntry {
        HostEntry {
            remark: name.into(),
            address: "1.2.3.4".into(),
            port: 443,
            protocol: "vless".into(),
            network: "tcp".into(),
            security: "reality".into(),
            sni: None,
            fingerprint: None,
            alpn: None,
            path: None,
            public_key: None,
            short_id: None,
            method: None,
            service_name: None,
            server_key: None,
            host_header: None,
        options: serde_json::json!({}),
            uuid: "u".into(),
        }
    }

    #[test]
    fn регистр_user_agent_не_важен() {
        assert_eq!(detect_format("HAPP/3.5"), Format::XrayJson);
        assert_eq!(detect_format("happ/3.5"), Format::XrayJson);
    }

    #[test]
    fn неизвестный_клиент_получает_base64() {
        assert_eq!(detect_format("SomeNewApp/1.0"), Format::Base64);
    }
}

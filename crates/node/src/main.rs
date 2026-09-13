//! Агент ноды: держит связь с панелью, применяет конфиг и отдаёт трафик.
//!
//! Один статический бинарник без рантайма — на edge-сервер не нужно ставить
//! ничего, кроме самого движка (xray или sing-box).
//!
//! Агент ходит в панель сам: у edge-серверов бывает нет белого IP,
//! а исходящее соединение есть всегда.

mod engine_update;
mod plugins;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};

const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

struct Settings {
    panel_url: String,
    secret: String,
    engine_bin: String,
    config_path: PathBuf,
    sync_interval: u64,
    stats_interval: u64,
    /// Адрес API-инбаунда движка, откуда читаем статистику.
    api_addr: String,
}

impl Settings {
    fn from_env() -> Result<Self, String> {
        let panel_url = std::env::var("PANEL_URL")
            .map_err(|_| "не задан PANEL_URL (например https://panel.example.com)")?;
        let url = reqwest::Url::parse(&panel_url).map_err(|_| "некорректный PANEL_URL")?;
        let loopback = url.host_str().is_some_and(|host| host == "localhost"
            || host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
        if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
            || !url.username().is_empty() || url.password().is_some()
            || url.query().is_some() || url.fragment().is_some() {
            return Err("PANEL_URL должен использовать HTTPS (HTTP разрешён только для loopback)".into());
        }
        let secret = std::env::var("NODE_SECRET").map_err(|_| "не задан NODE_SECRET")?;
        Ok(Settings {
            panel_url: panel_url.trim_end_matches('/').to_string(),
            secret,
            engine_bin: std::env::var("ENGINE_BIN").unwrap_or_else(|_| "xray".into()),
            config_path: std::env::var("ENGINE_CONFIG")
                .unwrap_or_else(|_| "/etc/sn-node/config.json".into())
                .into(),
            sync_interval: env_num("SYNC_INTERVAL_SECS", 15),
            stats_interval: env_num("STATS_INTERVAL_SECS", 60),
            api_addr: std::env::var("ENGINE_API").unwrap_or_else(|_| "127.0.0.1:10085".into()),
        })
    }
}

fn env_num(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[derive(Serialize)]
struct SyncRequest {
    agent_version: String,
    safe_engine_update: bool,
    engine_version: Option<String>,
    config_version: Option<i32>,
    users_version: Option<String>,
    engine_ok: bool,
    engine_error: Option<String>,
    online_count: Option<i32>,
    cpu_percent: Option<f32>,
    ram_percent: Option<f32>,
    uplink_bps: Option<i64>,
    downlink_bps: Option<i64>,
    /// Средняя загрузка за 1/5/15 минут. Мгновенный процент CPU не
    /// отличает пик от постоянной перегрузки — эти три числа отличают.
    la1: Option<f32>,
    la5: Option<f32>,
    la15: Option<f32>,
    /// Медленно меняющееся: панель показывает это в карточке ноды.
    cpu_model: Option<String>,
    cpu_cores: Option<i32>,
    kernel: Option<String>,
    mem_total_bytes: Option<i64>,
    mem_used_bytes: Option<i64>,
    uptime_seconds: Option<i64>,
    iface: Option<String>,
    rx_total_bytes: Option<i64>,
    tx_total_bytes: Option<i64>,
    /// Готовность сервера к плагинам: без nftables и прав NET_ADMIN
    /// они не работают, и панель не должна показывать их включёнными.
    plugins_status: Option<plugins::PluginStatus>,
}

#[derive(Deserialize)]
struct SyncResponse {
    #[serde(default)]
    config_changed: bool,
    #[serde(default)]
    config_version: Option<i32>,
    /// Отпечаток состава клиентов. Меняется при покупке, отзыве и правке
    /// сквада — то есть куда чаще, чем сам конфиг.
    #[serde(default)]
    users_version: Option<String>,
    #[serde(default)]
    config: Option<Value>,
    #[serde(default)]
    users: Vec<UserEntry>,
    /// Собирать ли домены. Приходит из панели с каждой синхронизацией.
    #[serde(default)]
    collect_domains: bool,
    /// Настройки плагинов: фильтры и блокировки.
    #[serde(default)]
    plugins: Option<plugins::PluginConfig>,
    /// Адреса, которые администратор разблокировал в панели.
    #[serde(default)]
    unblock_ips: Vec<String>,
    /// Отметка «перезапустить движок» из панели. Меняется на каждое
    /// нажатие, поэтому агент сравнивает её с уже отработанной.
    #[serde(default)]
    restart_token: Option<i64>,
    /// Какая версия движка должна стоять на ноде. Пусто — панель
    /// версией не управляет, агент ничего не трогает.
    #[serde(default)]
    engine_target: Option<String>,
    /// Отметка «обновить агента». Меняется, когда в панели нажали
    /// обновление; агент сверяет её с отработанной.
    #[serde(default)]
    agent_token: Option<i64>,
}

#[derive(Deserialize, Clone)]
struct UserEntry {
    uuid: String,
    email: String,
    inbound: String,
}

#[tokio::main]
async fn main() {
    // Версия по флагу — до всего остального.
    //
    // На это опирается обновление: скачанный бинарь запускают, чтобы
    // убедиться, что он вообще работает, и ждут завершения. Агент без
    // такого флага ушёл бы в свой обычный цикл и не вернулся никогда —
    // обновление зависло бы вместо того, чтобы пройти или честно упасть.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("sn-node {AGENT_VERSION}");
        return;
    }

    tracing_subscriber::fmt()
        // sn_core здесь обязателен: именно там логируются ошибки БД.
        // Без него «внутренняя ошибка» в ответе не имеет следа в журнале.
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sn_node=info".into()),
        )
        .init();

    let settings = match Settings::from_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Ошибка настройки: {e}");
            std::process::exit(2);
        }
    };

    tracing::info!(
        panel = settings.panel_url,
        engine = settings.engine_bin,
        "агент ноды v{AGENT_VERSION} запускается"
    );

    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("http клиент");

    let mut state = NodeState {
        config_version: None,
        users_version: None,
        engine_error: None,
        engine: None,
        engine_version: detect_engine_version(&settings.engine_bin).await,
        email_to_uuid: HashMap::new(),
        net_prev: None,
        online_count: None,
        access_pos: 0,
        want_domains: false,
        plugins_applied: None,
        plugins_error: None,
        restart_token: None,
        engine_update_failed: None,
        agent_token: None,
        torrent_block: None,
    };

    let mut sync_tick = tokio::time::interval(std::time::Duration::from_secs(settings.sync_interval));
    let mut stats_tick = tokio::time::interval(std::time::Duration::from_secs(settings.stats_interval));

    loop {
        tokio::select! {
            _ = sync_tick.tick() => {
                if let Err(e) = sync(&http, &settings, &mut state).await {
                    // Панель недоступна — это не повод падать: движок продолжает
                    // обслуживать клиентов на последнем применённом конфиге.
                    tracing::warn!(error = %e, "синхронизация не удалась");
                }
            }
            _ = stats_tick.tick() => {
                if let Err(e) = push_stats(&http, &settings, &mut state).await {
                    tracing::warn!(error = %e, "не отправил статистику");
                }
                // Отчёты идут тем же тактом: журнал читается инкрементально,
                // и разносить их по разным таймерам незачем.
                let want = state.want_domains;
                if let Err(e) = send_reports(&http, &settings, &mut state, want).await {
                    tracing::warn!(error = %e, "не отправил отчёты");
                }
            }
        }
    }
}

struct NodeState {
    config_version: Option<i32>,
    /// Отпечаток применённого состава клиентов. Отдельно от версии
    /// конфига: состав меняется на каждой покупке и отзыве.
    users_version: Option<String>,
    /// Последняя причина, по которой движок не работает. Пусто — работает.
    /// Уходит в панель, чтобы нода загоралась красным с объяснением, а не
    /// значилась «в строю», никого не обслуживая.
    engine_error: Option<String>,
    engine: Option<Child>,
    engine_version: Option<String>,
    /// Соответствие email→UUID из последнего конфига: движок отдаёт
    /// статистику по email, а панель принимает по UUID.
    email_to_uuid: HashMap<String, String>,
    /// Прошлый замер счётчиков интерфейса: скорость считается разницей.
    net_prev: Option<(std::time::Instant, i64, i64)>,
    /// Сколько клиентов передавали трафик в прошлом окне — это и есть
    /// «онлайн» на ноде.
    online_count: Option<i32>,
    /// Docker-подобная ротация и рестарт движка обнуляют файл, поэтому
    /// помним и размер: если он уменьшился, читаем с начала.
    access_pos: u64,
    /// Настройки плагинов, применённые в последний раз. Сравниваем,
    /// чтобы не дёргать nftables на каждом опросе.
    plugins_applied: Option<String>,
    /// Последняя причина отказа: чтобы не повторять её в журнале
    /// на каждом опросе, когда она не меняется.
    plugins_error: Option<String>,
    /// Отработанная отметка перезапуска. None означает «ещё не видели»:
    /// после собственного старта агент не должен дёргать движок из-за
    /// давнего нажатия, которое уже было выполнено до перезапуска.
    restart_token: Option<i64>,
    /// Версия, обновление на которую уже не удалось. Повторять её на
    /// каждом опросе — значит ходить в сеть каждые пятнадцать секунд;
    /// ждём, пока в панели укажут другую.
    engine_update_failed: Option<String>,
    /// Отработанная отметка обновления агента.
    agent_token: Option<i64>,
    /// Включён ли блокировщик торрентов: срок блокировки и исключения.
    torrent_block: Option<(u32, Vec<String>)>,
    /// Нужны ли панели домены. Решение живёт в панели, а не в окружении
    /// ноды: иначе владелец не смог бы выключить сбор, не заходя на
    /// каждый сервер.
    want_domains: bool,
}

/// Откуда агент берёт свою новую сборку.
///
/// Use the installed panel build first so local fixes reach its nodes.
/// Explicit AGENT_URL wins; GitHub is the fallback.
fn agent_sources(s: &Settings) -> Vec<String> {
    if let Ok(url) = std::env::var("AGENT_URL") {
        return vec![url];
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            tracing::warn!(arch = other, "нет сборки агента под эту архитектуру");
            return Vec::new();
        }
    };
    let repo = std::env::var("GH_REPO")
        .unwrap_or_else(|_| "STEALTHNET-APP/stealthnet-software".into());
    vec![
        format!("{}/sn-node-linux-{arch}", s.panel_url.trim_end_matches('/')),
        format!("https://github.com/{repo}/releases/latest/download/sn-node-linux-{arch}"),
    ]
}

async fn detect_engine_version(bin: &str) -> Option<String> {
    let out = Command::new(bin).arg("version").output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .map(String::from)
}

async fn sync(
    http: &reqwest::Client,
    s: &Settings,
    state: &mut NodeState,
) -> Result<(), String> {
    let (cpu, ram) = read_load();
    let sys = read_system();
    let net = read_net(state);

    let body = SyncRequest {
        agent_version: AGENT_VERSION.to_string(),
        safe_engine_update: true,
        engine_version: state.engine_version.clone(),
        config_version: state.config_version,
        users_version: state.users_version.clone(),
        // Живость движка проверяем каждый раз: он мог упасть уже после
        // успешного запуска — например, кончилась память.
        engine_ok: state.engine_error.is_none() && engine_alive(state),
        engine_error: state.engine_error.clone(),
        // Онлайн считаем как число клиентов, у которых с прошлого опроса
        // был трафик: у Xray нет понятия «подключённый пользователь», а
        // счётчики он отдаёт только по тем, кто что-то передал.
        online_count: state.online_count,
        cpu_percent: cpu,
        ram_percent: ram,
        uplink_bps: net.as_ref().map(|n| n.tx_bps),
        downlink_bps: net.as_ref().map(|n| n.rx_bps),
        la1: sys.la.map(|l| l.0),
        la5: sys.la.map(|l| l.1),
        la15: sys.la.map(|l| l.2),
        cpu_model: sys.cpu_model.clone(),
        cpu_cores: sys.cpu_cores,
        kernel: sys.kernel.clone(),
        mem_total_bytes: sys.mem_total,
        mem_used_bytes: sys.mem_used,
        uptime_seconds: sys.uptime,
        iface: net.as_ref().map(|n| n.iface.clone()),
        rx_total_bytes: net.as_ref().map(|n| n.rx_total),
        tx_total_bytes: net.as_ref().map(|n| n.tx_total),
        plugins_status: Some({
            let mut st = plugins::probe();
            st.applied = state.plugins_applied.is_some();
            st
        }),
    };

    let res = http
        .post(format!("{}/api/node/sync", s.panel_url))
        .header("x-node-secret", &s.secret)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("панель недоступна: {e}"))?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        // Секрет неверен — сколько ни повторяй, лучше не станет.
        tracing::error!("панель не признала секрет ноды, проверьте NODE_SECRET");
        return Err("неверный секрет".into());
    }

    let sync: SyncResponse = res
        .json()
        .await
        .map_err(|e| format!("панель вернула не то: {e}"))?;

    // Флаг применяем всегда, даже когда конфиг не менялся: тумблер в
    // панели должен срабатывать без перевыпуска конфигурации.
    state.want_domains = sync.collect_domains;

    // Снятия применяем всегда: администратор ждёт, что кнопка в панели
    // подействует сразу, а не после следующей правки настроек.
    for ip in &sync.unblock_ips {
        match plugins::unblock_ip(ip) {
            Ok(()) => tracing::info!(ip, "блокировка снята"),
            // Записи может уже не быть: истёк срок или правила
            // пересоздавались. Это не ошибка.
            Err(e) if e.contains("No such file") => {}
            Err(e) => tracing::warn!(ip, error = %e, "не снял блокировку"),
        }
    }

    // Плагины применяем при каждом изменении, не дожидаясь смены
    // конфига движка: блокировку адреса ждать 15 минут нельзя.
    if let Some(cfg) = &sync.plugins {
        let fingerprint = format!("{cfg:?}");
        // Ничего не включено — не трогаем nftables вовсе. Иначе на ноде
        // без него агент писал предупреждение каждые пятнадцать секунд
        // о том, чего никто не просил.
        if cfg.is_noop() {
            state.plugins_applied = Some(fingerprint);
            state.torrent_block = None;
        } else if state.plugins_applied.as_deref() != Some(&fingerprint) {
            match plugins::apply(cfg) {
                Ok(()) => {
                    state.plugins_applied = Some(fingerprint);
                    state.plugins_error = None;
                    state.torrent_block = cfg.torrent_blocker.enabled.then(|| {
                        (cfg.torrent_blocker.block_duration, cfg.torrent_blocker.ignore_ips.clone())
                    });
                    tracing::info!("правила плагинов применены");
                }
                // Причина обычно не меняется от попытки к попытке —
                // отсутствующий nftables сам не появится. Пишем в журнал
                // только когда сообщение другое, а не каждый раз.
                Err(e) => {
                    if state.plugins_error.as_deref() != Some(e.as_str()) {
                        tracing::warn!(error = %e, "не применил плагины — повторим молча");
                        state.plugins_error = Some(e);
                    }
                }
            }
        }
    }

    // Обновление самого агента.
    //
    // Идёт первым: старый агент не понимает остального, что присылает
    // панель. Нода, поставленная до появления обновления движка, иначе
    // навсегда остаётся со старым Xray — и это видно как «версии разные».
    if let Some(token) = sync.agent_token {
        match state.agent_token {
            // Первый ответ после старта: агент только что поднялся, и
            // подменять себя из-за давнего нажатия незачем.
            None => state.agent_token = Some(token),
            Some(seen) if seen != token => {
                tracing::info!("панель просит обновить агента");
                let sources = agent_sources(s);
                match engine_update::replace_self(&sources).await {
                    Ok(()) => {
                        tracing::info!("агент обновлён — выходим, systemd поднимет новый");
                        // Отметку записывать некому: следующий запуск
                        // увидит её как первую и просто запомнит.
                        std::process::exit(0);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "не обновил агента");
                        // Помним отметку, чтобы не пытаться каждые 15 секунд.
                        state.agent_token = Some(token);
                    }
                }
            }
            _ => {}
        }
    }

    // Обновление движка до версии, заданной в панели.
    //
    // Идёт до перезапуска и до применения конфига: новый конфиг может
    // опираться на то, чего старое ядро не умеет. Например, Hysteria 2
    // как транспорт появилась только в 26.7 — на ядре постарше клиенты
    // отвечают «not hysteria transport», хотя на ноде всё «работает».
    if let Some(target) = sync.engine_target.as_deref() {
        let failed_before = state.engine_update_failed.as_deref() == Some(target);
        if !failed_before
            && engine_update::needs_update(state.engine_version.as_deref(), target)
        {
            tracing::info!(
                from = state.engine_version.as_deref().unwrap_or("?"),
                to = target,
                "обновляем движок"
            );
            match engine_update::install(&s.engine_bin, &s.config_path, target).await {
                Ok(installed) => {
                    let previous=state.engine_version.clone();
                    let start=if std::path::Path::new(&s.config_path).exists() {restart_engine(s,state).await} else {Ok(())};
                    match start {
                        Ok(())=>{state.engine_version=Some(installed.clone());state.engine_update_failed=None;state.engine_error=None;tracing::info!(version=installed,"движок обновлён");}
                        Err(e)=>{
                            state.engine_update_failed=Some(target.to_string());
                            match engine_update::rollback(&s.engine_bin).await {
                                Ok(())=>{state.engine_version=previous;let restored=restart_engine(s,state).await;state.engine_error=Some(format!("обновление до {target} отменено: {e}; откат: {}",if restored.is_ok(){"выполнен"}else{"движок не запустился"}));}
                                Err(rollback)=>{state.engine_version=Some(installed);state.engine_error=Some(format!("{e}; {rollback}"));}
                            }
                        }
                    }
                }
                // Помним неудачу, чтобы не ходить в сеть каждые 15 секунд.
                // Причина уедет в панель вместе со следующим опросом.
                Err(e) => {
                    tracing::error!(error = %e, target, "не обновил движок");
                    state.engine_update_failed = Some(target.to_string());
                    state.engine_error = Some(format!("обновление до {target}: {e}"));
                }
            }
        }
    }

    // Перезапуск по кнопке из панели.
    //
    // Идёт до проверки config_changed: движок просят перезапустить как
    // раз тогда, когда конфиг не менялся — подхватить обновлённый
    // сертификат, вылечить зависшие соединения.
    if let Some(token) = sync.restart_token {
        match state.restart_token {
            // Первый ответ после старта агента: движок и так только что
            // поднялся, повторять незачем — просто запоминаем отметку.
            None => state.restart_token = Some(token),
            Some(seen) if seen != token => {
                tracing::info!("панель просит перезапустить движок");
                match restart_engine(s, state).await {
                    Ok(()) => {
                        state.restart_token = Some(token);
                        state.engine_error = None;
                    }
                    // Отметку не запоминаем: попробуем ещё раз на
                    // следующем опросе, а причина уедет в панель.
                    Err(e) => {
                        tracing::error!(error = %e, "не перезапустил движок");
                        state.engine_error = Some(e);
                    }
                }
            }
            _ => {}
        }
    }

    if !sync.config_changed {
        return Ok(());
    }

    let Some(config) = sync.config else {
        return Ok(());
    };

    // Разделяем два случая: поменялся конфиг или только состав клиентов.
    // Второе случается на каждой покупке, и в журнале это должно
    // читаться как обычное событие, а не как правка конфигурации.
    let only_users = state.config_version == sync.config_version;
    tracing::info!(
        version = ?sync.config_version,
        users = sync.users.len(),
        причина = if only_users { "изменился состав клиентов" } else { "новый конфиг" },
        "применяем"
    );


    let merged = merge_users(config, &sync.users);

    // Проверяем конфиг движком до подмены рабочего.
    //
    // Битый конфиг роняет Xray целиком — вместе со всеми инбаундами,
    // включая те, что работали. Один недописанный путь к сертификату в
    // новом протоколе выключал ноду полностью, и клиенты теряли доступ
    // к локациям, которые к правке отношения не имели.
    //
    // Поэтому: пишем во временный файл, спрашиваем движок, и только при
    // его согласии подменяем рабочий. Иначе остаёмся на прежнем — нода
    // продолжает обслуживать клиентов, а о проблеме сообщаем в панель.
    // Every failure is reported, including a missing executable or unwritable
    // configuration directory. A version is acknowledged only after startup.
    let apply_result: Result<(), String> = async {
        let previous = match std::fs::read(&s.config_path) {
            Ok(bytes) => Some(serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| "Cannot read the previous configuration for rollback".to_string())?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("Cannot read the previous configuration for rollback".into()),
        };
        let probe = s.config_path.with_extension("check.json");
        write_config(&probe, &merged)?;
        let verdict = tokio::time::timeout(std::time::Duration::from_secs(15),
            Command::new(&s.engine_bin).kill_on_drop(true).arg("-test").arg("-c").arg(&probe).output()).await;
        let _ = std::fs::remove_file(&probe);
        let out = verdict.map_err(|_| "Xray validation timed out; previous configuration kept".to_string())?
            .map_err(|_| "Cannot start Xray validation; previous configuration kept".to_string())?;
        if !out.status.success() {
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            return Err(text.lines().find(|l| l.contains("Failed to start") || l.contains("failed to"))
                .unwrap_or("Xray rejected the configuration").chars().take(400).collect());
        }
        write_config(&s.config_path, &merged)?;
        if let Err(error) = restart_engine(s, state).await {
            if let Some(previous) = previous {
                write_config(&s.config_path, &previous).map_err(|e| format!("{error}; rollback write: {e}"))?;
                restart_engine(s, state).await.map_err(|e| format!("{error}; rollback: {e}"))?;
            }
            return Err(error);
        }
        Ok(())
    }.await;
    if let Err(error) = apply_result { state.engine_error = Some(error.clone()); return Err(error); }
    state.email_to_uuid = sync
        .users
        .iter()
        .map(|u| (u.email.clone(), u.uuid.clone()))
        .collect();
    state.config_version = sync.config_version;
    state.users_version = sync.users_version;
    state.engine_error = None;
    Ok(())
}


/// Метод шифрования shadowsocks-инбаунда. Длина ключа зависит от него:
/// 2022-blake3-aes-128-gcm — 16 байт, остальные варианты 2022 — 32.
fn method_of(inbound: &Value) -> String {
    inbound["settings"]["method"]
        .as_str()
        .unwrap_or("2022-blake3-aes-128-gcm")
        .to_string()
}

/// Пароль клиента для shadowsocks-2022 — base64 от ключа нужной длины.
///
/// Берём байты UUID клиента: они уже случайны и уникальны, а привязка к
/// UUID означает, что «отозвать подписку» отключает клиента и здесь —
/// иначе отзыв работал бы для одних протоколов и не работал для других.
fn ss_password(uuid: &str, method: String) -> String {
    use base64::Engine;
    let hex: Vec<char> = uuid.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let raw: Vec<u8> = hex
        .chunks(2)
        .filter_map(|p| {
            let hi = p.first()?.to_digit(16)?;
            let lo = p.get(1)?.to_digit(16)?;
            Some(((hi << 4) | lo) as u8)
        })
        .collect();

    let need = if method.contains("128") { 16 } else { 32 };
    let mut key = raw.clone();
    // 16 байт UUID мало для 32-байтного ключа — добиваем повтором, но не
    // нулями: нулевой хвост одинаков у всех и ослабляет ключ.
    while key.len() < need {
        let tail: Vec<u8> = raw.iter().map(|b| b.rotate_left(3)).collect();
        key.extend_from_slice(&tail);
    }
    key.truncate(need);
    base64::engine::general_purpose::STANDARD.encode(key)
}

/// Вписывает клиентов в инбаунды конфига.
///
/// Панель отдаёт конфиг без клиентов и отдельно их список: так один
/// профиль переиспользуется на многих нодах, а состав клиентов у каждой свой.
fn merge_users(mut config: Value, users: &[UserEntry]) -> Value {
    let Some(inbounds) = config["inbounds"].as_array_mut() else {
        return config;
    };

    for inbound in inbounds.iter_mut() {
        let tag = inbound["tag"].as_str().unwrap_or("").to_string();
        let protocol = inbound["protocol"].as_str().unwrap_or("vless").to_string();
        // Reality поверх голого TCP — единственный случай, где нужен flow.
        // Имя транспорта приводим к одному: в свежих конфигах он зовётся
        // «raw», и без приведения flow тихо не проставлялся бы.
        let vision = inbound["streamSettings"]["security"] == "reality"
            && sn_core::xray_check::normalize_network(
                inbound["streamSettings"]["network"].as_str().unwrap_or("tcp")) == "tcp";

        // У каждого протокола своя форма записи клиента. Раньше всем
        // писалось `{"id": …}` — форма VLESS/VMess, — поэтому trojan и
        // shadowsocks не пускали никого: Xray искал у клиента password,
        // а его не было.
        let clients: Vec<Value> = users
            .iter()
            .filter(|u| u.inbound == tag)
            .map(|u| match protocol.as_str() {
                // Trojan и hysteria опознают клиента по паролю. Берём
                // UUID: он уникален и меняется при отзыве подписки, так
                // что отзыв работает одинаково во всех протоколах.
                "trojan" => json!({ "password": u.uuid, "email": u.email }),
                "hysteria" => json!({ "auth": u.uuid, "email": u.email }),
                "shadowsocks" => json!({
                    // Многопользовательский shadowsocks бывает только в
                    // версии 2022: там у каждого свой ключ. Ключ выводим
                    // из UUID клиента, поэтому отзыв подписки (он меняет
                    // UUID) отключает и здесь.
                    "password": ss_password(&u.uuid, method_of(inbound)),
                    "email": u.email,
                }),
                // vless, vmess и всё остальное на id
                _ => json!({
                    "id": u.uuid,
                    "email": u.email,
                    "flow": if vision { "xtls-rprx-vision" } else { "" },
                }),
            })
            .collect();

        // Служебным инбаундам список клиентов не нужен: dokodemo-door
        // его не читает, а пустое поле в конфиге только сбивает с толку
        // того, кто в него заглянет.
        if protocol == "dokodemo-door" {
            continue;
        }

        if inbound["settings"].is_null() {
            inbound["settings"] = json!({});
        }

        // Xray Hysteria authenticates users by `auth` in its QUIC transport.
        // Clear any shared transport credential: after the last subscriber is
        // revoked it must not become an alternate way into the node.
        if protocol == "hysteria" {
            inbound["settings"]["users"] = json!(clients);
            if let Some(o) = inbound["settings"].as_object_mut() { o.remove("clients"); }
            if let Some(o) = inbound["streamSettings"]["hysteriaSettings"].as_object_mut() { o.remove("auth"); }
        } else {
            inbound["settings"]["clients"] = json!(clients);
        }
    }
    tag_torrent_blocks(&mut config);
    config
}

fn is_torrent_block(line: &str) -> bool {
    line.contains(">> sn-block-bittorrent]") || line.contains("-> sn-block-bittorrent]")
}

fn tag_torrent_blocks(config: &mut Value) {
    let blackholes: Vec<String> = config["outbounds"].as_array().into_iter().flatten()
        .filter(|o| o["protocol"] == "blackhole").filter_map(|o| o["tag"].as_str().map(str::to_owned)).collect();
    if config["outbounds"].as_array().into_iter().flatten().any(|o| o["tag"]=="sn-block-bittorrent" && o["protocol"]!="blackhole") { return; }
    let mut changed = false;
    if let Some(rules)=config["routing"]["rules"].as_array_mut() {
        for rule in rules {
            if rule["protocol"] == json!(["bittorrent"]) && rule["outboundTag"].as_str().is_some_and(|tag| blackholes.iter().any(|b| b==tag)) {
                rule["outboundTag"]=json!("sn-block-bittorrent"); changed=true;
            }
        }
    }
    if changed {
        if let Some(outbounds)=config["outbounds"].as_array_mut() {
            if !outbounds.iter().any(|o|o["tag"]=="sn-block-bittorrent") { outbounds.push(json!({"tag":"sn-block-bittorrent","protocol":"blackhole"})); }
        }
    }
}

/// Путь журнала доступа движка. Читаем его сами: наружу Xray этот
/// журнал не отдаёт, а без него не сказать, кто ходил на торренты.
/// Свод из журнала доступа: заблокированные торренты и домены.
///
/// Читаем только новое с прошлого раза и сразу сворачиваем в счётчики.
/// Сырые строки в панель не уезжают: журнал посещений — самое ценное,
/// что может утечь, и держать его копию в двух местах незачем.
#[derive(Default, Serialize)]
struct AccessReport {
    /// email клиента (в конфиге это его UUID) → сколько заблокировано.
    torrents: HashMap<String, TorrentHit>,
    /// домен → сколько обращений.
    domains: HashMap<String, u32>,
}

#[derive(Default, Serialize)]
struct TorrentHit {
    hits: u32,
    last_target: String,
    /// Адреса, с которых шли попытки. Нужны блокировщику: отрезать
    /// надо источник, а не цель.
    #[serde(skip)]
    sources: Vec<String>,
}

fn read_access_log(state: &mut NodeState, want_domains: bool) -> Option<AccessReport> {
    let path = access_log_path();
    let meta = std::fs::metadata(&path).ok()?;
    let size = meta.len();

    // Файл усох — значит его повернули или движок перезапустился.
    // Читать с прежней позиции в этом случае значит читать мусор.
    if size < state.access_pos {
        state.access_pos = 0;
    }
    if size == state.access_pos {
        return None;
    }

    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(&path).ok()?;
    f.seek(SeekFrom::Start(state.access_pos)).ok()?;

    // Ограничиваем разовое чтение: на нагруженной ноде журнал растёт
    // быстрее, чем идёт опрос, и без предела агент съел бы память.
    const MAX: u64 = 4 * 1024 * 1024;
    let to_read = (size - state.access_pos).min(MAX);
    let mut buf = vec![0u8; to_read as usize];
    f.read_exact(&mut buf).ok()?;
    state.access_pos += to_read;

    let text = String::from_utf8_lossy(&buf);
    let mut out = AccessReport::default();

    for line in text.lines() {
        // Формат строки Xray:
        //   ... accepted tcp:example.com:443 [in -> out] email: <uuid>
        let Some(target) = line
            .split_whitespace()
            .find(|w| w.starts_with("tcp:") || w.starts_with("udp:"))
        else {
            continue;
        };
        let email = line.split("email: ").nth(1).map(|e| e.trim().to_string());

        // Private ranges, ads and custom rules can also use blackhole. Only
        // the dedicated BitTorrent route is evidence for a torrent report.
        if is_torrent_block(line) {
            if let Some(email) = email {
                let e = out.torrents.entry(email).or_default();
                e.hits += 1;
                e.last_target = target.to_string();
                // «from 1.2.3.4:5678» — адрес клиента, с которого шла
                // попытка. Порт отбрасываем: блокируем адрес целиком.
                if let Some(from) = line.split("from ").nth(1).and_then(|v| v.split_whitespace().next()) {
                    if let Some(ip) = from.rsplit_once(':').map(|(a, _)| a) {
                        let ip = ip.trim_matches(|c| c == '[' || c == ']').to_string();
                        if !e.sources.contains(&ip) {
                            e.sources.push(ip);
                        }
                    }
                }
            }
            continue;
        }

        if want_domains {
            // Берём только имя: порт и протокол в статистике не нужны,
            // а IP-адреса не считаем — по ним ничего не понять.
            let host = target
                .split(':')
                .nth(1)
                .unwrap_or_default()
                .trim_start_matches("//");
            if !host.is_empty() && host.contains('.') && !host.chars().all(|c| c.is_ascii_digit() || c == '.') {
                *out.domains.entry(host.to_lowercase()).or_default() += 1;
            }
        }
    }

    if out.torrents.is_empty() && out.domains.is_empty() {
        return None;
    }
    Some(out)
}

fn access_log_path() -> String {
    std::env::var("ENGINE_ACCESS_LOG").unwrap_or_else(|_| "/var/log/sn-node/access.log".into())
}

fn write_config(path: &PathBuf, config: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("не создал {}: {e}", dir.display()))?;
    }
    // Пишем во временный файл и переименовываем: движок никогда не увидит
    // наполовину записанный конфиг.
    // Дописываем путь журнала: он нужен агенту, а панель про файловую
    // систему конкретной ноды ничего не знает и знать не должна.
    let mut config = config.clone();
    let log_path = access_log_path();
    if let Some(dir) = std::path::Path::new(&log_path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    config["log"]["access"] = json!(log_path);
    if config["log"]["loglevel"].is_null() {
        config["log"]["loglevel"] = json!("warning");
    }

    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let text = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let result: Result<(), String> = (|| {
        let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| format!("не заменил конфиг: {e}"))?;
        Ok(())
    })();
    if result.is_err() { let _ = std::fs::remove_file(&tmp); }
    result?;
    Ok(())
}

/// Работает ли движок прямо сейчас.
///
/// Запустить его мало: процесс мог упасть через минуту после старта, и
/// панель об этом узнать неоткуда. Проверяем перед каждой отправкой.
fn engine_alive(state: &mut NodeState) -> bool {
    match state.engine.as_mut() {
        Some(child) => match child.try_wait() {
            // Ok(None) — процесс ещё жив; Ok(Some(_)) — завершился.
            Ok(None) => true,
            _ => false,
        },
        None => false,
    }
}

async fn restart_engine(s: &Settings, state: &mut NodeState) -> Result<(), String> {
    if let Some(child) = state.engine.as_mut() {
        let _ = child.kill().await;
    }

    let mut child = Command::new(&s.engine_bin)
        .arg("run")
        .arg("-c")
        .arg(&s.config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("не запустил {}: {e}", s.engine_bin))?;

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    if let Some(status)=child.try_wait().map_err(|e|e.to_string())? {
        state.engine=None;
        return Err(format!("Xray завершился при запуске: {status}. Проверьте занятые порты и профиль"));
    }
    state.engine = Some(child);
    tracing::info!(engine = s.engine_bin, "движок перезапущен с новым конфигом");
    Ok(())
}

/// Отправка потребления.
///
/// `statsquery -reset` уже возвращает дельту с прошлого опроса, поэтому
/// вычитать ничего не надо. Ключ — это `email` из конфига, куда агент
/// кладёт username; панель сопоставляет по нему, а не по UUID.
async fn push_stats(
    http: &reqwest::Client,
    s: &Settings,
    state: &mut NodeState,
) -> Result<(), String> {
    let current = read_engine_counters(&s.engine_bin, &s.api_addr).await;
    let mut usage = Vec::new();

    for (email, (up, down)) in &current {
        if *up <= 0 && *down <= 0 {
            continue;
        }
        // Панели нужен UUID: сопоставляем по таблице, полученной с конфигом.
        let Some(uuid) = state.email_to_uuid.get(email) else { continue };
        usage.push(json!({ "uuid": uuid, "upload_bytes": up, "download_bytes": down }));
    }

    // Клиенты, у которых был трафик в этом окне, и есть «онлайн» ноды.
    // У Xray нет понятия подключённого пользователя: он отдаёт счётчики
    // только по тем, кто что-то передал с прошлого сброса.
    state.online_count = Some(usage.len() as i32);

    if usage.is_empty() {
        return Ok(());
    }

    http.post(format!("{}/api/node/stats", s.panel_url))
        .header("x-node-secret", &s.secret)
        .json(&json!({ "usage": usage }))
        .send()
        .await
        .map_err(|e| format!("не отправил статистику: {e}"))?;

    tracing::debug!(clients = usage.len(), "статистика отправлена");
    Ok(())
}

/// Отправляет свод из журнала доступа.
///
/// Панель сама решает, нужны ли домены: тумблер живёт там, а не в
/// окружении ноды — иначе владелец не смог бы его выключить, не заходя
/// на каждый сервер.
async fn send_reports(
    http: &reqwest::Client,
    s: &Settings,
    state: &mut NodeState,
    want_domains: bool,
) -> Result<(), String> {
    let Some(report) = read_access_log(state, want_domains) else {
        return Ok(());
    };

    // Блокируем источник в ядре: движок видит соединение уже после
    // установки TCP, а nftables отбрасывает пакет до него — иначе
    // нарушитель продолжает тратить ресурсы ноды.
    let mut blocked = Vec::new();
    if let Some((duration, ignore)) = state.torrent_block.clone() {
        for hit in report.torrents.values() {
            for ip in &hit.sources {
                if ignore.iter().any(|i| i == ip) {
                    continue;
                }
                match plugins::block_ip(ip, duration) {
                    Ok(()) => {
                        tracing::info!(ip, duration, "адрес заблокирован за торренты");
                        blocked.push(ip.clone());
                    }
                    Err(e) => tracing::warn!(ip, error = %e, "не заблокировал адрес"),
                }
            }
        }
    }

    http.post(format!("{}/api/node/reports", s.panel_url))
        .header("x-node-secret", &s.secret)
        .json(&serde_json::json!({
            "torrents": report.torrents,
            "domains": report.domains,
            "blocked_ips": blocked,
            "block_seconds": state.torrent_block.as_ref().map(|(d, _)| *d),
        }))
        .send()
        .await
        .map_err(|e| format!("не отправил отчёты: {e}"))?;

    tracing::debug!(
        torrents = report.torrents.len(),
        domains = report.domains.len(),
        "отчёты отправлены"
    );
    Ok(())
}

/// Счётчики трафика по клиентам.
///
/// Читаем через CLI движка (`xray api statsquery`), а не по gRPC напрямую:
/// это тот же самый интерфейс, но без protobuf-зависимости и генерации кода,
/// а значит агент собирается быстрее и не ломается при смене версии API.
///
/// Формат имён у Xray: `user>>>alice>>>traffic>>>uplink`.
/// Мы просим `reset=true` — движок отдаёт значения и обнуляет счётчики,
/// поэтому пришедшее уже является дельтой с прошлого опроса.
async fn read_engine_counters(bin: &str, api_addr: &str) -> HashMap<String, (i64, i64)> {
    let out = Command::new(bin)
        .args([
            "api",
            "statsquery",
            &format!("--server={api_addr}"),
            "-pattern",
            "user>>>",
            "-reset",
        ])
        .output()
        .await;

    let Ok(out) = out else {
        return HashMap::new();
    };
    if !out.status.success() {
        // Движок может быть без API-инбаунда — это не ошибка агента,
        // просто трафик не собирается. Шумим один раз в лог и живём дальше.
        tracing::debug!("statsquery недоступен, трафик не собирается");
        return HashMap::new();
    }

    parse_stats(&String::from_utf8_lossy(&out.stdout))
}

/// Разбор ответа `statsquery`.
///
/// Ожидаем `{"stat":[{"name":"user>>>alice>>>traffic>>>uplink","value":"123"}]}`.
/// Всё, что не про пользователей, пропускаем: там же лежит статистика
/// по инбаундам и исходящим, а она к лимитам клиентов отношения не имеет.
fn parse_stats(text: &str) -> HashMap<String, (i64, i64)> {
    let Ok(parsed) = serde_json::from_str::<Value>(text) else {
        return HashMap::new();
    };

    let mut map: HashMap<String, (i64, i64)> = HashMap::new();
    for item in parsed["stat"].as_array().cloned().unwrap_or_default() {
        let Some(name) = item["name"].as_str() else { continue };
        // Значение приходит то числом, то строкой — движок непоследователен.
        let value = item["value"]
            .as_i64()
            .or_else(|| item["value"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        if value <= 0 {
            continue;
        }

        let parts: Vec<&str> = name.split(">>>").collect();
        if parts.len() != 4 || parts[0] != "user" {
            continue;
        }
        let entry = map.entry(parts[1].to_string()).or_insert((0, 0));
        match parts[3] {
            "uplink" => entry.0 += value,
            "downlink" => entry.1 += value,
            _ => {}
        }
    }
    map
}

/// Загрузка сервера из /proc. На не-Linux вернём None — агент всё равно
/// предназначен для edge-серверов на Linux.
fn read_load() -> (Option<f32>, Option<f32>) {
    let cpu = std::fs::read_to_string("/proc/loadavg").ok().and_then(|s| {
        let la: f32 = s.split_whitespace().next()?.parse().ok()?;
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
        Some((la / cores * 100.0).min(100.0))
    });

    let ram = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
        let mut total = 0f32;
        let mut avail = 0f32;
        for line in s.lines() {
            if let Some(v) = line.strip_prefix("MemTotal:") {
                total = v.trim().trim_end_matches(" kB").trim().parse().ok()?;
            } else if let Some(v) = line.strip_prefix("MemAvailable:") {
                avail = v.trim().trim_end_matches(" kB").trim().parse().ok()?;
            }
        }
        if total > 0.0 {
            Some((1.0 - avail / total) * 100.0)
        } else {
            None
        }
    });

    (cpu, ram)
}

/// Медленно меняющиеся сведения о сервере.
#[derive(Default)]
struct SystemInfo {
    la: Option<(f32, f32, f32)>,
    cpu_model: Option<String>,
    cpu_cores: Option<i32>,
    kernel: Option<String>,
    mem_total: Option<i64>,
    mem_used: Option<i64>,
    uptime: Option<i64>,
}

fn read_system() -> SystemInfo {
    let mut out = SystemInfo::default();

    if let Ok(s) = std::fs::read_to_string("/proc/loadavg") {
        let mut it = s.split_whitespace();
        if let (Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next()) {
            if let (Ok(a), Ok(b), Ok(c)) = (a.parse(), b.parse(), c.parse()) {
                out.la = Some((a, b, c));
            }
        }
    }

    if let Ok(s) = std::fs::read_to_string("/proc/cpuinfo") {
        // «model name» повторяется для каждого ядра — берём первое
        // вхождение, а ядра считаем по числу строк processor.
        out.cpu_model = s
            .lines()
            .find_map(|l| l.strip_prefix("model name"))
            .and_then(|v| v.split_once(':'))
            .map(|(_, v)| v.trim().to_string());
        let cores = s.lines().filter(|l| l.starts_with("processor")).count();
        if cores > 0 {
            out.cpu_cores = Some(cores as i32);
        }
    }

    out.kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|s| s.trim().to_string());

    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        let kb = |key: &str| -> Option<i64> {
            s.lines()
                .find_map(|l| l.strip_prefix(key))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<i64>()
                .ok()
                .map(|v| v * 1024)
        };
        out.mem_total = kb("MemTotal:");
        // Занятой считаем total минус available: free без кэша показывает
        // почти ноль свободной памяти на любом живом сервере и пугает зря.
        if let (Some(t), Some(a)) = (out.mem_total, kb("MemAvailable:")) {
            out.mem_used = Some(t - a);
        }
    }

    out.uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .map(|v| v as i64);

    out
}

#[derive(Clone)]
struct NetSample {
    iface: String,
    rx_total: i64,
    tx_total: i64,
    rx_bps: i64,
    tx_bps: i64,
}

/// Скорость интерфейса. /proc/net/dev отдаёт счётчики с момента загрузки,
/// поэтому скорость считаем как разницу с прошлым опросом.
fn read_net(state: &mut NodeState) -> Option<NetSample> {
    let text = std::fs::read_to_string("/proc/net/dev").ok()?;

    // Берём интерфейс с наибольшим объёмом принятого: на edge-сервере это
    // и есть внешний. Имя eth0 жёстко брать нельзя — у половины хостеров
    // интерфейс называется иначе (ens3, enp1s0).
    let mut best: Option<(String, i64, i64)> = None;
    for line in text.lines().skip(2) {
        let (name, rest) = line.split_once(':')?;
        let name = name.trim();
        if name == "lo" || name.starts_with("docker") || name.starts_with("veth") {
            continue;
        }
        let f: Vec<i64> = rest
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if f.len() < 9 {
            continue;
        }
        let (rx, tx) = (f[0], f[8]);
        if best.as_ref().map(|(_, r, _)| rx > *r).unwrap_or(true) {
            best = Some((name.to_string(), rx, tx));
        }
    }

    let (iface, rx, tx) = best?;
    let now = std::time::Instant::now();

    let (rx_bps, tx_bps) = match state.net_prev.take() {
        Some((prev_at, prev_rx, prev_tx)) => {
            let secs = now.duration_since(prev_at).as_secs_f64().max(0.001);
            // Счётчики обнуляются при перезагрузке сервера: отрицательная
            // разница означает не отрицательную скорость, а рестарт.
            let d = |cur: i64, prev: i64| if cur >= prev { ((cur - prev) as f64 / secs) as i64 } else { 0 };
            (d(rx, prev_rx) * 8, d(tx, prev_tx) * 8)
        }
        None => (0, 0),
    };
    state.net_prev = Some((now, rx, tx));

    Some(NetSample { iface, rx_total: rx, tx_total: tx, rx_bps, tx_bps })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteria_credentials_use_auth_and_have_no_shared_fallback() {
        let config=json!({"inbounds":[{"tag":"hy","protocol":"hysteria","settings":{"version":2,"clients":[{"password":"legacy"}]},"streamSettings":{"network":"hysteria","hysteriaSettings":{"auth":"shared"}}}]});
        let user=UserEntry{uuid:"user-secret".into(),email:"alice".into(),inbound:"hy".into()};
        let merged=merge_users(config.clone(),&[user]);
        assert_eq!(merged["inbounds"][0]["settings"]["users"][0]["auth"],"user-secret");
        assert!(merged["inbounds"][0]["settings"]["clients"].is_null());
        let empty=merge_users(config,&[]);
        assert_eq!(empty["inbounds"][0]["settings"]["users"],json!([]));
        assert!(empty["inbounds"][0]["streamSettings"]["hysteriaSettings"]["auth"].is_null());
    }
    #[test]
    fn unrelated_blocks_cannot_be_reported_as_torrents() {
        let mut config=json!({"outbounds":[{"protocol":"blackhole","tag":"block"}],"routing":{"rules":[{"protocol":["bittorrent"],"outboundTag":"block"},{"ip":["geoip:private"],"outboundTag":"block"}]}});
        tag_torrent_blocks(&mut config);
        assert_eq!(config["routing"]["rules"][0]["outboundTag"],"sn-block-bittorrent");
        assert_eq!(config["routing"]["rules"][1]["outboundTag"],"block");
        assert!(is_torrent_block("accepted tcp:test:443 [in >> sn-block-bittorrent] email: alice"));
        assert!(!is_torrent_block("accepted tcp:test:443 [in >> block] email: alice"));
    }

    #[test]
    fn клиенты_попадают_только_в_свой_инбаунд() {
        let config = json!({
            "inbounds": [
                { "tag": "vless-reality", "streamSettings": { "security": "reality", "network": "tcp" } },
                { "tag": "vless-ws", "streamSettings": { "security": "tls", "network": "ws" } }
            ]
        });
        let users = vec![
            UserEntry { uuid: "u1".into(), email: "a".into(), inbound: "vless-reality".into() },
            UserEntry { uuid: "u2".into(), email: "b".into(), inbound: "vless-ws".into() },
            UserEntry { uuid: "u3".into(), email: "c".into(), inbound: "vless-reality".into() },
        ];

        let merged = merge_users(config, &users);
        let reality = merged["inbounds"][0]["settings"]["clients"].as_array().unwrap();
        let ws = merged["inbounds"][1]["settings"]["clients"].as_array().unwrap();

        assert_eq!(reality.len(), 2);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0]["id"], "u2");
        // flow нужен только reality поверх tcp
        assert_eq!(reality[0]["flow"], "xtls-rprx-vision");
        assert_eq!(ws[0]["flow"], "");
    }

    #[test]
    fn статистика_движка_разбирается() {
        let json = r#"{"stat":[
            {"name":"user>>>alice>>>traffic>>>uplink","value":"1024"},
            {"name":"user>>>alice>>>traffic>>>downlink","value":"8192"},
            {"name":"user>>>bob>>>traffic>>>downlink","value":500},
            {"name":"inbound>>>vless-reality>>>traffic>>>downlink","value":"99999"},
            {"name":"outbound>>>direct>>>traffic>>>uplink","value":"77777"}
        ]}"#;
        let m = parse_stats(json);
        // Инбаунды и исходящие не должны попасть в потребление клиентов
        assert_eq!(m.len(), 2, "только пользователи: {m:?}");
        assert_eq!(m["alice"], (1024, 8192));
        assert_eq!(m["bob"], (0, 500));
    }

    #[test]
    fn мусор_в_статистике_не_ломает_агент() {
        assert!(parse_stats("не json").is_empty());
        assert!(parse_stats("{}").is_empty());
        assert!(parse_stats(r#"{"stat":[]}"#).is_empty());
        // Нулевые и отрицательные значения игнорируем: слать их панели незачем
        assert!(parse_stats(r#"{"stat":[{"name":"user>>>a>>>traffic>>>uplink","value":"0"}]}"#).is_empty());
        assert!(parse_stats(r#"{"stat":[{"name":"битое-имя","value":"5"}]}"#).is_empty());
    }

    #[test]
    fn конфиг_без_инбаундов_не_ломается() {
        let merged = merge_users(json!({ "outbounds": [] }), &[]);
        assert!(merged["inbounds"].is_null());
    }
}

//! Проверка конфигурации Xray перед сохранением.
//!
//! Сохранить битый конфиг = уронить все ноды профиля разом: агент заберёт
//! его и перезапустит движок, который не поднимется. Поэтому проверяем до,
//! а не после.
//!
//! Двухступенчато: сначала структурные правила (работают везде и всегда),
//! затем, если на хосте есть бинарь движка, — его собственный `-test`.

use serde_json::Value;
use std::io::Write;

// A profile can contain private keys. Concurrent validations must neither
// share a file nor expose its contents to other local users.
struct CheckFile(std::path::PathBuf);
impl CheckFile {
    fn create(config: &Value) -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("sn-check-{}.json", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        let guard = Self(path);
        file.write_all(config.to_string().as_bytes())?;
        Ok(guard)
    }
}
impl Drop for CheckFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}

/// Одно имя транспорта на всю систему.
///
/// Xray переименовал `tcp` в `raw` и принимает оба. Конфиги приходят и
/// с тем, и с другим — свои заготовки, чужие, вставленные руками. Но
/// внутри выбор должен быть один: сборка ссылок и агент на ноде
/// ветвятся на «tcp», и незнакомое имя тихо уводит их не в ту ветку —
/// клиент получает ссылку без `flow`, а разница видна только тем, что
/// «стало медленнее».
pub fn normalize_network(network: &str) -> &str {
    match network {
        "raw" => "tcp",
        иное => иное,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckResult {
    pub valid: bool,
    /// Ошибки: с ними сохранять нельзя.
    pub errors: Vec<String>,
    /// Замечания: сохранить можно, но стоит посмотреть.
    pub warnings: Vec<String>,
    /// Теги инбаундов — панель обновит по ним связи.
    pub inbounds: Vec<InboundInfo>,
    /// Проверял ли конфиг сам движок.
    pub engine_checked: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InboundInfo {
    pub tag: String,
    pub protocol: String,
    pub port: i64,
    pub network: String,
    pub security: String,
    /// Метод шифрования shadowsocks — клиент обязан назвать такой же.
    pub method: Option<String>,
    /// Имя сервиса gRPC — без него клиентский транспорт пуст.
    pub service_name: Option<String>,
    /// Серверный ключ shadowsocks-2022. Клиент подключается парой
    /// «серверный:личный», поэтому он нужен и на стороне подписки.
    pub server_key: Option<String>,
    /// Служебный инбаунд: через него работает сам агент, а не клиенты.
    ///
    /// Такие в панели не показываются и не привязываются к хостам,
    /// сквадам и нодам — выбрать `api-in` локацией нельзя, он слушает
    /// петлю и никого не пускает. Раньше он попадал во все списки
    /// выбора и мозолил глаза, а при неосторожном клике уходил в сквад.
    pub is_service: bool,
    /// Публичный ключ Reality, выведенный из приватного в профиле.
    ///
    /// Клиент проверяет им сервер. Хранить его отдельной копией в строке
    /// хоста значило однажды получить расхождение с профилем — и локацию,
    /// которая «не отвечает» при полностью исправном сервере.
    pub public_key: Option<String>,
    /// Первый shortId из профиля. Клиент обязан назвать один из списка.
    pub short_id: Option<String>,
    /// serverNames[0] — имя, под которое маскируется Reality.
    pub sni: Option<String>,
}

/// Структурная проверка. Ловит ошибки, из-за которых конфиг либо не
/// запустится, либо запустится, но клиенты не подключатся.
pub fn check_structure(config: &Value) -> CheckResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut inbounds = Vec::new();
    if let Err(error)=crate::profile_workflow::validate_shape(config){return CheckResult{valid:false,errors:vec![error],warnings,inbounds,engine_checked:false};}

    let Some(list) = config.get("inbounds").and_then(|v| v.as_array()) else {
        errors.push("нет секции inbounds — ноде нечего слушать".into());
        return CheckResult { valid: false, errors, warnings, inbounds, engine_checked: false };
    };

    if list.is_empty() {
        errors.push("список inbounds пуст".into());
    }

    let mut seen_tags: Vec<String> = Vec::new();
    let mut seen_ports: Vec<(i64, String, u8)> = Vec::new();
    for path in crate::profile_workflow::missing_parameters(config) {
        errors.push(format!("Required parameter / Не заполнено: {path}"));
    }

    for (i, ib) in list.iter().enumerate() {
        let tag = ib["tag"].as_str().unwrap_or_default().to_string();
        let protocol = ib["protocol"].as_str().unwrap_or_default().to_string();
        let port = ib["port"].as_i64().unwrap_or(0);

        if tag.is_empty() {
            errors.push(format!("inbound #{}: пустой tag — на него ссылаются хосты и сквады", i + 1));
        } else if seen_tags.contains(&tag) {
            // Xray стартует, но статистика и маршрутизация ломаются молча.
            errors.push(format!("tag «{tag}» повторяется — теги должны быть уникальны"));
        } else {
            seen_tags.push(tag.clone());
        }

        if protocol.is_empty() {
            errors.push(format!("inbound «{tag}»: не указан protocol"));
        }
        if !(1..=65535).contains(&port) {
            errors.push(format!("inbound «{tag}»: недопустимый порт {port}"));
        } else {
            let listen=ib["listen"].as_str().unwrap_or("0.0.0.0");
            let udp_only=matches!(ib["streamSettings"]["network"].as_str(),Some("kcp"|"hysteria"));
            let networks=ib["settings"]["network"].as_str().unwrap_or("tcp");
            let sockets=if udp_only {2} else if protocol=="shadowsocks" && networks.contains("udp") {if networks.contains("tcp"){3}else{2}} else {1};
            if seen_ports.iter().any(|(p,l,u)| *p==port && *u & sockets != 0 && (l==listen || ["0.0.0.0","::"].contains(&listen) || ["0.0.0.0","::"].contains(&l.as_str()))) {
                errors.push(format!("порт {port} занят дважды"));
            }
            seen_ports.push((port,listen.to_string(),sockets));
        }

        let stream = &ib["streamSettings"];
        let network = normalize_network(stream["network"].as_str().unwrap_or("tcp")).to_string();
        let security = stream["security"].as_str().unwrap_or("none").to_string();

        if security == "reality" {
            let r = &stream["realitySettings"];
            if r["privateKey"].as_str().unwrap_or_default().is_empty() {
                errors.push(format!(
                    "inbound «{tag}»: reality без privateKey — сгенерируйте ключи"
                ));
            }
            // Сайт для маскировки Xray зовёт и `dest`, и `target` —
            // второе имя новее, и именно оно приходит в конфигах из
            // других панелей. Движок понимает оба, проверка обязана
            // тоже: иначе рабочий конфиг отвергается на ровном месте.
            let цель = r["target"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| r["dest"].as_str().filter(|s| !s.is_empty()));
            if цель.is_none() {
                errors.push(format!(
                    "inbound «{tag}»: reality без сайта для маскировки — задайте target"
                ));
            }
            let names = r["serverNames"].as_array().map(|a| a.len()).unwrap_or(0);
            if names == 0 {
                errors.push(format!("inbound «{tag}»: reality без serverNames"));
            }
            if r["shortIds"].as_array().map(|a| a.is_empty()).unwrap_or(true) {
                warnings.push(format!(
                    "inbound «{tag}»: пустой shortIds — допустимо, но лучше задать"
                ));
            }
        }

        if security == "tls" {
            let certs = stream["tlsSettings"]["certificates"].as_array().map(|a| a.len()).unwrap_or(0);
            if certs == 0 {
                warnings.push(format!(
                    "inbound «{tag}»: tls без сертификатов — работает только за обратным прокси"
                ));
            }
        }

        // Клиентов подставляет агент, поэтому непустой список здесь —
        // почти всегда забытая ручная правка.
        if ib["settings"]["clients"].as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            warnings.push(format!(
                "inbound «{tag}»: в конфиге прописаны clients — их подставляет агент, список будет перезаписан"
            ));
        }

        // Служебный ли это инбаунд. dokodemo-door — всегда: он нужен
        // движку для отдачи статистики. socks и http считаем служебными,
        // когда они слушают петлю: наружу их не выставляют, это местные
        // прокси для отладки.
        let listen = ib["listen"].as_str().unwrap_or("");
        let loopback = listen.starts_with("127.") || listen == "::1" || listen == "localhost";
        if port == crate::service_parts::API_PORT && (protocol != "dokodemo-door" || !loopback) {
            errors.push(format!("Reserved statistics port must use a loopback API inbound / Служебный порт статистики требует API-инбаунд на loopback: {tag}"));
        }
        let is_service = protocol == "dokodemo-door"
            || ((protocol == "socks" || protocol == "http") && loopback);

        // Параметры, которые клиент обязан повторить у себя.
        let method = ib["settings"]["method"].as_str().map(String::from);
        let server_key = ib["settings"]["password"].as_str().map(String::from);
        let service_name = stream["grpcSettings"]["serviceName"].as_str().map(String::from);

        // Многопользовательский shadowsocks существует только в версии
        // 2022: у классических методов один пароль на всех, и раздать
        // разным клиентам разные ключи невозможно.
        // Hysteria в Xray называется «hysteria», а не «hysteria2».
        // Версию требуем явно: клиентская сторона без неё не собирается.
        if protocol == "hysteria" {
            match ib["settings"]["version"].as_i64() {
                Some(2) => {}
                Some(v) => errors.push(format!(
                    "inbound «{tag}»: hysteria версии {v} не поддерживается — укажите version: 2"
                )),
                None => warnings.push(format!(
                    "inbound «{tag}»: у hysteria не задан version — клиенты ожидают 2"
                )),
            }
            if security != "tls" {
                warnings.push(format!(
                    "inbound «{tag}»: hysteria без TLS — клиенты подключаются к нему только по TLS"
                ));
            }
            // Транспорт обязателен и ровно один.
            //
            // Движок принимает инбаунд и с обычным tcp — он поднимается и
            // даже обслуживает клиентов на том же ядре. Но клиентские
            // приложения со свежим Xray отвечают «not hysteria transport»
            // и не запускаются вовсе, так что снаружи это выглядит как
            // «сервер не работает». Ловим на сохранении профиля.
            let net = ib["streamSettings"]["network"].as_str().unwrap_or("tcp");
            if net != "hysteria" {
                errors.push(format!(
                    "inbound «{tag}»: у hysteria transport должен быть \"network\": \"hysteria\", \
                     а не \"{net}\" — иначе клиенты отвечают «not hysteria transport»"
                ));
            }
        }

        if protocol == "shadowsocks" {
            // Серверный ключ (psk). Без него движок не стартует вовсе:
            // клиент подключается парой «серверный ключ:личный ключ».
            if ib["settings"]["password"].as_str().unwrap_or_default().is_empty() {
                errors.push(format!(
                    "inbound «{tag}»: shadowsocks без settings.password — это серверный ключ, сгенерируйте его"
                ));
            }
            match method.as_deref() {
                Some(m) if m.starts_with("2022-") => {}
                Some(m) => errors.push(format!(
                    "inbound «{tag}»: метод {m} не поддерживает раздельные ключи клиентов — возьмите 2022-blake3-aes-128-gcm"
                )),
                None => errors.push(format!(
                    "inbound «{tag}»: shadowsocks без settings.method"
                )),
            }
        }
        if network == "grpc" && service_name.as_deref().unwrap_or("").is_empty() {
            warnings.push(format!(
                "inbound «{tag}»: grpc без serviceName — клиенты подключатся к пустому сервису"
            ));
        }

        let reality = &ib["streamSettings"]["realitySettings"];
        let public_key = reality["privateKey"]
            .as_str()
            .and_then(crate::keygen::reality_public_from_private);
        let short_id = reality["shortIds"][0].as_str().map(str::to_string);
        let sni = reality["serverNames"][0].as_str().map(str::to_string);

        inbounds.push(InboundInfo {
            tag, protocol, port, network, security, method, service_name, server_key, is_service,
            public_key, short_id, sni,
        });
    }

    if config.get("outbounds").and_then(|v| v.as_array()).map(|a| a.is_empty()).unwrap_or(true) {
        errors.push("нет outbounds — трафику некуда идти".into());
    }

    // Resolve routing references before publishing a configuration.
    let mut outbound_tags=std::collections::HashSet::new();
    if let Some(list)=config["outbounds"].as_array(){for out in list{
        if let Some(tag)=out["tag"].as_str(){if !outbound_tags.insert(tag){errors.push(format!("Duplicate outbound tag / Повторяется outbound tag: {tag}"));}}
    }}
    if let Some(api)=config["api"]["tag"].as_str(){outbound_tags.insert(api);}
    let balancers:std::collections::HashSet<_>=config["routing"]["balancers"].as_array().into_iter().flatten().filter_map(|v|v["tag"].as_str()).collect();
    if let Some(rules)=config["routing"]["rules"].as_array(){for (index,rule) in rules.iter().enumerate(){
        if let Some(tag)=rule["outboundTag"].as_str(){if !outbound_tags.contains(tag){errors.push(format!("routing.rules[{index}]: unknown outboundTag / неизвестный outboundTag: {tag}"));}}
        if let Some(tag)=rule["balancerTag"].as_str(){if !balancers.contains(tag){errors.push(format!("routing.rules[{index}]: unknown balancerTag / неизвестный balancerTag: {tag}"));}}
    }}

    // Без этого блока агент не соберёт статистику, и лимиты не будут работать.
    let has_stats = config.get("stats").is_some()
        && config["api"]["services"]
            .as_array()
            .map(|a| a.iter().any(|s| s == "StatsService"))
            .unwrap_or(false);
    if !has_stats {
        warnings.push(
            "нет api+stats со StatsService — трафик по клиентам собираться не будет".into(),
        );
    }

    CheckResult {
        valid: errors.is_empty(),
        errors,
        warnings,
        inbounds,
        engine_checked: false,
    }
}

/// Полная проверка: структура плюс `xray -test`, если бинарь доступен.
pub fn check_full(config: &Value, engine_bin: &str) -> CheckResult {
    let mut result = check_structure(config);
    if !result.valid {
        return result;
    }

    // Незаполненные места из заготовок.
    //
    // Движок и сам споткнётся о них, но скажет об этом на своём языке
    // («no such file or directory») и только про первое попавшееся.
    // Здесь причина названа прямо: человек взял заготовку и не дописал
    // ключ или путь к сертификату.
    let text = config.to_string();
    let mut left: Vec<&str> = ["ВАШ_ПРИВАТНЫЙ_КЛЮЧ", "ВАШ_SHORT_ID", "ВАШ_ПУТЬ",
                               "СЕРВЕРНЫЙ_КЛЮЧ", "/ПУТЬ/К/", "ВТОРАЯ_НОДА",
                               "UUID_СЕРВИСНОГО_КЛИЕНТА", "ПУБЛИЧНЫЙ_КЛЮЧ_ВТОРОЙ_НОДЫ"]
        .iter()
        .copied()
        .filter(|p| text.contains(p))
        .collect();
    left.dedup();
    if !left.is_empty() {
        result.valid = false;
        result.errors.push(format!(
            "в конфиге остались незаполненные места из заготовки: {}. \
             Впишите свои значения — с ними движок не запустится",
            left.join(", ")
        ));
        return result;
    }

    let Ok(file) = CheckFile::create(config) else {
        return result;
    };

    let out = (|| -> std::io::Result<std::process::Output> {
        let mut child=std::process::Command::new(engine_bin).arg("-test").arg("-c").arg(&file.0)
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
        let started=std::time::Instant::now();
        loop {
            if child.try_wait()?.is_some(){return child.wait_with_output();}
            if started.elapsed()>std::time::Duration::from_secs(15){let _=child.kill();let _=child.wait();return Err(std::io::Error::new(std::io::ErrorKind::TimedOut,"Xray validation timed out"));}
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    })();
    drop(file);

    match out {
        Ok(out) => {
            result.engine_checked = true;
            if !out.status.success() {
                // Xray печатает причину в stdout, а не в stderr — читаем оба
                // и берём строку с «Failed to start», иначе покажем баннер версии.
                let text = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                let msg = text
                    .lines()
                    .find(|l| l.contains("Failed to start") || l.contains("failed to"))
                    .or_else(|| text.lines().rev().find(|l| !l.trim().is_empty()))
                    .unwrap_or("движок отверг конфиг")
                    .trim()
                    // Внутренние пути Xray админу ничего не говорят —
                    // оставляем последнюю, самую конкретную часть цепочки.
                    .rsplit(" > ")
                    .next()
                    .unwrap_or("движок отверг конфиг")
                    .to_string();
                result.errors.push(format!("движок: {msg}"));
                result.valid = false;
            }
        }
        // Бинаря нет — это нормально: панель может стоять отдельно от нод.
        Err(e) if e.kind()==std::io::ErrorKind::TimedOut => {result.valid=false;result.errors.push("Xray validation timed out / Превышено время проверки Xray".into());}
        Err(_) => result.engine_checked = false,
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn validation_files_are_private_unique_and_removed() {
        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(|| super::CheckFile::create(&serde_json::json!({"key":"first"})).unwrap());
            let second = scope.spawn(|| super::CheckFile::create(&serde_json::json!({"key":"second"})).unwrap());
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(first.0, second.0);
        assert!(std::fs::read_to_string(&first.0).unwrap().contains("first"));
        assert!(std::fs::read_to_string(&second.0).unwrap().contains("second"));
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&first.0).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let paths = [first.0.clone(), second.0.clone()];
        drop(first); drop(second);
        assert!(paths.iter().all(|p| !p.exists()));
    }

    use super::*;
    use serde_json::json;

    fn valid_config() -> Value {
        json!({
            "api": { "tag": "api", "services": ["StatsService"] },
            "stats": {},
            "inbounds": [{
                "tag": "vless-reality", "port": 443, "protocol": "vless",
                "settings": { "clients": [], "decryption": "none" },
                "streamSettings": {
                    "network": "tcp", "security": "reality",
                    "realitySettings": {
                        "dest": "www.cloudflare.com:443",
                        "serverNames": ["www.cloudflare.com"],
                        "privateKey": "abc", "shortIds": ["a1b2"]
                    }
                }
            }],
            "outbounds": [{ "protocol": "freedom", "tag": "direct" }]
        })
    }

    #[test]
    fn служебный_инбаунд_помечается() {
        // api-in существует ради статистики: он слушает петлю и клиентов
        // не принимает. В списках выбора локаций ему не место.
        let cfg = json!({
            "inbounds": [
                { "tag": "api-in", "listen": "127.0.0.1", "port": 10085,
                  "protocol": "dokodemo-door", "settings": { "address": "127.0.0.1" } },
                { "tag": "vless-reality", "port": 443, "protocol": "vless",
                  "settings": { "clients": [], "decryption": "none" },
                  "streamSettings": { "network": "tcp", "security": "reality",
                    "realitySettings": { "dest": "a:443", "serverNames": ["a"],
                      "privateKey": "k", "shortIds": ["a1"] } } },
                { "tag": "local-socks", "listen": "127.0.0.1", "port": 1080,
                  "protocol": "socks", "settings": {} },
            ],
            "outbounds": [{ "protocol": "freedom", "tag": "direct" }]
        });
        let r = check_structure(&cfg);
        let service: Vec<&str> = r.inbounds.iter().filter(|i| i.is_service)
            .map(|i| i.tag.as_str()).collect();
        assert_eq!(service, vec!["api-in", "local-socks"]);
        let client: Vec<&str> = r.inbounds.iter().filter(|i| !i.is_service)
            .map(|i| i.tag.as_str()).collect();
        assert_eq!(client, vec!["vless-reality"]);
    }

    #[test]
    fn корректный_конфиг_проходит() {
        let r = check_structure(&valid_config());
        assert!(r.valid, "ошибки: {:?}", r.errors);
        assert!(r.warnings.is_empty(), "замечания: {:?}", r.warnings);
        assert_eq!(r.inbounds.len(), 1);
        assert_eq!(r.inbounds[0].tag, "vless-reality");
    }

    #[test]
    fn reality_без_ключа_не_проходит() {
        let mut c = valid_config();
        c["inbounds"][0]["streamSettings"]["realitySettings"]["privateKey"] = json!("");
        let r = check_structure(&c);
        assert!(!r.valid);
        assert!(r.errors.iter().any(|e| e.contains("privateKey")), "{:?}", r.errors);
    }

    #[test]
    fn повторяющиеся_теги_ловятся() {
        let mut c = valid_config();
        let dup = c["inbounds"][0].clone();
        c["inbounds"].as_array_mut().unwrap().push(dup);
        let r = check_structure(&c);
        assert!(!r.valid);
        // Повтор тега И повтор порта — обе ошибки реальны
        assert!(r.errors.iter().any(|e| e.contains("повторяется")), "{:?}", r.errors);
    }

    #[test]
    fn пустой_конфиг_даёт_понятную_ошибку() {
        let r = check_structure(&json!({}));
        assert!(!r.valid);
        assert!(r.errors[0].contains("inbounds"));
    }

    #[test]
    fn отсутствие_статистики_только_замечание() {
        let mut c = valid_config();
        c.as_object_mut().unwrap().remove("stats");
        let r = check_structure(&c);
        assert!(r.valid, "без stats конфиг рабочий, просто без учёта трафика");
        assert!(r.warnings.iter().any(|w| w.contains("трафик")));
    }

    #[test]
    fn прописанные_вручную_клиенты_вызывают_замечание() {
        let mut c = valid_config();
        c["inbounds"][0]["settings"]["clients"] = json!([{ "id": "x" }]);
        let r = check_structure(&c);
        assert!(r.valid);
        assert!(r.warnings.iter().any(|w| w.contains("перезаписан")));
    }
}

#[cfg(test)]
mod совместимость {
    use super::*;
    use serde_json::json;

    /// Конфиг ровно в том виде, в каком его отдаёт другая панель.
    fn чужой() -> Value {
        json!({
            "log": { "loglevel": "none" },
            "inbounds": [{
                "tag": "FIN_VLESS", "port": 443, "listen": "0.0.0.0",
                "protocol": "vless",
                "settings": { "clients": [], "decryption": "none" },
                "sniffing": { "enabled": true, "destOverride": ["http","tls","quic"] },
                "streamSettings": {
                    "network": "raw", "security": "reality",
                    "realitySettings": {
                        "target": "example.com:443",
                        "shortIds": [""],
                        "privateKey": base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, [7u8; 32]),
                        "serverNames": ["example.com"],
                    },
                },
            }],
            "outbounds": [
                { "tag": "DIRECT", "protocol": "freedom" },
                { "tag": "BLOCK", "protocol": "blackhole" },
            ],
            "routing": { "rules": [
                { "ip": ["geoip:private"], "outboundTag": "BLOCK" },
                { "protocol": ["bittorrent"], "outboundTag": "BLOCK" },
            ]},
        })
    }

    #[test]
    fn чужой_конфиг_проходит_проверку_без_правок() {
        // Движок сюда не зовём: проверяем свои правила, а не его.
        let r = check_full(&чужой(), "заведомо-нет-такого-бинаря");
        assert!(r.valid, "не должен отвергаться: {:?}", r.errors);
    }

    #[test]
    fn транспорт_raw_читается_как_tcp() {
        // От этого зависит `flow` у Reality и `type=` в ссылке клиенту:
        // незнакомое имя увело бы сборку ссылок не в ту ветку.
        let r = check_full(&чужой(), "заведомо-нет-такого-бинаря");
        let ib = r.inbounds.iter().find(|i| i.tag == "FIN_VLESS").unwrap();
        assert_eq!(ib.network, "tcp");
        assert_eq!(ib.security, "reality");
    }

    #[test]
    fn имя_dest_по_прежнему_принимается() {
        // Старые конфиги никуда не делись — ломать их нельзя.
        let mut c = чужой();
        let r = &mut c["inbounds"][0]["streamSettings"]["realitySettings"];
        r["dest"] = r["target"].take();
        assert!(check_full(&c, "заведомо-нет-такого-бинаря").valid);
    }

    #[test]
    fn без_сайта_маскировки_всё_ещё_ошибка() {
        let mut c = чужой();
        c["inbounds"][0]["streamSettings"]["realitySettings"]["target"] = json!("");
        let r = check_full(&c, "заведомо-нет-такого-бинаря");
        assert!(!r.valid);
        assert!(r.errors[0].contains("маскировки"), "{:?}", r.errors);
    }
}

#[cfg(test)]
mod доступ_по_любому_паролю {
    use super::*;
    use serde_json::json;

    fn гистерия() -> Value {
        json!({
            "inbounds": [{
                "tag": "hy", "port": 8449, "listen": "0.0.0.0", "protocol": "hysteria",
                "settings": { "version": 2, "users": [] },
                "streamSettings": { "network": "hysteria", "security": "tls",
                    "tlsSettings": { "certificates": [
                        { "certificateFile": "/c.pem", "keyFile": "/k.pem" }] } },
            }],
            "outbounds": [{ "protocol": "freedom" }],
        })
    }

    #[test]
    fn hysteria_requires_its_authenticated_transport() {
        let r = check_full(&гистерия(), "нет-такого");
        assert!(r.valid, "{:?}", r.errors);
        assert!(!r.warnings.iter().any(|w| w.contains("НЕ ПРОВЕРЯЕТ пароль")));
        let mut wrong=гистерия();
        wrong["inbounds"][0]["streamSettings"]["network"]=json!("tcp");
        assert!(!check_structure(&wrong).valid);
    }

    #[test]
    fn остальных_протоколов_предупреждение_не_касается() {
        let c = json!({
            "inbounds": [{ "tag": "v", "port": 443, "listen": "0.0.0.0", "protocol": "vless",
                "settings": { "clients": [], "decryption": "none" },
                "streamSettings": { "network": "tcp", "security": "none" } }],
            "outbounds": [{ "protocol": "freedom" }],
        });
        let r = check_full(&c, "нет-такого");
        assert!(!r.warnings.iter().any(|w| w.contains("НЕ ПРОВЕРЯЕТ")), "{:?}", r.warnings);
    }
}

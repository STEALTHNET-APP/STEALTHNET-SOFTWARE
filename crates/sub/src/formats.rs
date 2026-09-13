//! Генерация подписок в форматах клиентских приложений.
//!
//! ВАЖНО (чистая комната): форматы принадлежат проектам Xray-core, sing-box и
//! Clash/mihomo, а не какой-либо панели. Здесь всё написано по их собственной
//! документации; чужой код панелей не заимствовался.

use base64::Engine;
use serde_json::{json, Value};

/// Один хост подписки, уже связанный с конкретным клиентом.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HostEntry {
    pub remark: String,
    pub address: String,
    pub port: i32,
    pub protocol: String,
    pub network: String,
    pub security: String,
    pub sni: Option<String>,
    pub fingerprint: Option<String>,
    pub alpn: Option<String>,
    pub path: Option<String>,
    pub public_key: Option<String>,
    pub short_id: Option<String>,
    pub uuid: String,
    /// Метод шифрования shadowsocks-инбаунда. У остальных протоколов пуст.
    pub method: Option<String>,
    /// Имя сервиса gRPC. Заполняется для транспорта grpc.
    pub service_name: Option<String>,
    /// Серверный ключ shadowsocks-2022.
    pub server_key: Option<String>,
    /// Заголовок Host для ws/httpupgrade/xhttp, если он отличается от SNI.
    pub host_header: Option<String>,
    #[serde(default)]
    pub options: Value,
}

fn q(value: &str) -> String {
    value.bytes().map(|b| if b.is_ascii_alphanumeric()||b"-._~/".contains(&b){(b as char).to_string()}else{format!("%{b:02X}")}).collect()
}
fn fragment(remark:&str)->String{q(remark)}

/// Учётные данные клиента для протокола.
///
/// У VLESS и VMess это UUID, у Trojan — пароль, у Shadowsocks-2022 —
/// ключ, выведенный из того же UUID. Агент вписывает клиента на ноде по
/// тем же правилам, поэтому ссылка и сервер всегда совпадают.
fn ss_key(uuid: &str, method: &str) -> String {
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
    while key.len() < need {
        let tail: Vec<u8> = raw.iter().map(|b| b.rotate_left(3)).collect();
        key.extend_from_slice(&tail);
    }
    key.truncate(need);
    base64::engine::general_purpose::STANDARD.encode(key)
}

impl HostEntry {
    /// Учётные данные, которые уйдут клиенту.
    ///
    /// У VLESS и VMess это UUID, у Trojan — он же в роли пароля, у
    /// Shadowsocks-2022 — выведенный из него ключ. Агент вписывает
    /// клиента на ноде теми же правилами, поэтому ссылка всегда
    /// соответствует тому, что реально ждёт сервер.
    fn credential(&self) -> String {
        match self.protocol.as_str() {
            "shadowsocks" => {
                let user = ss_key(
                    &self.uuid,
                    self.method.as_deref().unwrap_or("2022-blake3-aes-128-gcm"),
                );
                // Многопользовательский shadowsocks-2022 ждёт пару
                // «серверный ключ:личный ключ». Без серверной половины
                // движок отвечает «missing psk» и не пускает никого.
                match self.server_key.as_deref().filter(|k| !k.is_empty()) {
                    Some(srv) => format!("{srv}:{user}"),
                    None => user,
                }
            }
            _ => self.uuid.clone(),
        }
    }

    /// Параметры транспорта для share-link.
    ///
    /// Раньше в ссылку попадал только `path`, поэтому gRPC, xHTTP и
    /// httpupgrade доезжали до клиента без своих настроек — приложение
    /// подключалось «в никуда» и молча не работало.
    fn link_transport(&self, params: &mut Vec<String>) {
        match self.network.as_str() {
            "grpc" => {
                if let Some(sn) = self.service_name.as_ref().filter(|s| !s.is_empty()) {
                    params.push(format!("serviceName={}", q(sn)));
                }
                params.push("mode=gun".into());
            }
            "ws" | "httpupgrade" | "xhttp" | "splithttp" => {
                params.push(format!("path={}", q(self.path.as_deref().unwrap_or("/"))));
                if let Some(h) = self.host_header.as_ref().or(self.sni.as_ref()).filter(|s| !s.is_empty()) {
                    params.push(format!("host={}", q(h)));
                }
            }
            // h2 удалён из Xray 26 (перенесён в XHTTP), но панель ставят и
            // на старые сборки движка — параметры для него оставлены.
            "h2" | "http" => {
                params.push(format!("path={}", q(self.path.as_deref().unwrap_or("/"))));
                if let Some(h) = self.host_header.as_ref().or(self.sni.as_ref()).filter(|s| !s.is_empty()) {
                    params.push(format!("host={}", q(h)));
                }
            }
            // mKCP — поверх UDP. Обфускация заголовком и seed из движка
            // убраны, поэтому в ссылке ничего сверх типа не нужно.
            "kcp" | "mkcp" => {}
            _ => {
                if let Some(path) = self.path.as_ref().filter(|s| !s.is_empty()) {
                    params.push(format!("path={}", q(path)));
                }
            }
        }
    }

    /// Ссылка вида `протокол://…` для клиентских приложений.
    ///
    /// У VMess формат принципиально другой — base64 от JSON, а не
    /// query-строка. Раньше он собирался как VLESS, и такая ссылка не
    /// открывалась ни одним приложением.
    pub fn to_share_link(&self) -> String {
        if self.protocol == "vmess" {
            return self.to_vmess_link();
        }
        if self.protocol == "shadowsocks" {
            return self.to_ss_link();
        }
        if self.protocol == "hysteria" {
            return self.to_hysteria_link();
        }

        let mut params: Vec<String> = Vec::new();
        // encryption обязателен по описанию схемы vless://, даже когда
        // шифрования нет. Мы его не отдавали: часть приложений на такой
        // ссылке молча не создаёт исходящее соединение, и локация просто
        // не появляется. У trojan этого параметра нет.
        if self.protocol == "vless" {
            params.push("encryption=none".into());
        }
        params.push(format!("type={}", self.network));
        params.push(format!("security={}", self.security));

        if let Some(sni) = self.sni.as_ref().filter(|s| !s.is_empty()) {
            params.push(format!("sni={}", q(sni)));
        }
        if let Some(fp) = self.fingerprint.as_ref().filter(|s| !s.is_empty()) {
            params.push(format!("fp={}", q(fp)));
        }
        if let Some(alpn) = self.alpn.as_ref().filter(|s| !s.is_empty()) {
            params.push(format!("alpn={}", q(alpn)));
        }
        self.link_transport(&mut params);
        if let Some(tag)=self.options["tag"].as_str().filter(|s|!s.is_empty()){params.push(format!("tag={}",q(tag)));}
        if let Some(description)=self.options["server_description"].as_str().filter(|s|!s.is_empty()){params.push(format!("description={}",q(description)));}
        if self.options["allow_insecure"]==true && self.security=="tls"{params.push("allowInsecure=1".into());}

        if self.security == "reality" {
            if let Some(pbk) = self.public_key.as_ref().filter(|s| !s.is_empty()) {
                params.push(format!("pbk={}", q(pbk)));
            }
            if let Some(sid) = self.short_id.as_ref().filter(|s| !s.is_empty()) {
                params.push(format!("sid={}", q(sid)));
            }
        }
        // flow актуален только для VLESS + reality поверх голого tcp.
        if self.protocol == "vless" && self.security == "reality" && self.network == "tcp" {
            params.push("flow=xtls-rprx-vision".into());
        }

        format!(
            "{}://{}@{}:{}?{}#{}",
            self.protocol,
            self.credential(),
            self.address,
            self.port,
            params.join("&"),
            fragment(&self.remark)
        )
    }

    /// vmess://base64(json) — исторический формат, описанный в v2ray.
    fn to_vmess_link(&self) -> String {
        let obj = json!({
            "v": "2",
            "ps": self.remark,
            "add": self.address,
            "port": self.port.to_string(),
            "id": self.uuid,
            "aid": "0",
            "scy": "auto",
            "net": self.network,
            "type": "none",
            "host": self.host_header.clone().or_else(|| self.sni.clone()).unwrap_or_default(),
            "path": self.path.clone().unwrap_or_default(),
            "tls": if self.security == "none" { "" } else { &self.security },
            "sni": self.sni.clone().unwrap_or_default(),
            "alpn": self.alpn.clone().unwrap_or_default(),
            "fp": self.fingerprint.clone().unwrap_or_default(),
        });
        format!(
            "vmess://{}",
            base64::engine::general_purpose::STANDARD.encode(obj.to_string())
        )
    }

    /// hysteria2://пароль@host:port?…#remark
    ///
    /// Схема в ссылке — `hysteria2`, как её понимают приложения. Сама же
    /// реализация в Xray идёт поверх обычного транспорта, а не поверх
    /// QUIC, поэтому подключиться смогут клиенты на движке Xray;
    /// отдельный QUIC-сервер hysteria этой ссылке не соответствует.
    fn to_hysteria_link(&self) -> String {
        let mut params: Vec<String> = Vec::new();
        if let Some(sni) = self.sni.as_ref().filter(|s| !s.is_empty()) {
            params.push(format!("sni={}", q(sni)));
        }
        params.push("insecure=0".into());
        format!(
            "hysteria2://{}@{}:{}?{}#{}",
            self.credential(),
            self.address,
            self.port,
            params.join("&"),
            fragment(&self.remark)
        )
    }

    /// ss://base64(method:key)@host:port#remark
    fn to_ss_link(&self) -> String {
        let method = self.method.as_deref().unwrap_or("2022-blake3-aes-128-gcm");
        let userinfo = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(format!("{}:{}", method, self.credential()));
        format!(
            "ss://{}@{}:{}#{}",
            userinfo,
            self.address,
            self.port,
            fragment(&self.remark)
        )
    }

    /// streamSettings для outbound: транспорт и шифрование.
    fn stream_settings(&self) -> Value {
        // Hysteria 2 — собственный транспорт, а не протокол поверх TCP.
        //
        // Xray отдаёт `proxy/hysteria: not hysteria transport`, если в
        // streamSettings стоит что-то другое. Раньше сюда попадал `tcp`
        // (транспорт инбаунда), и подписка не запускалась в приложениях
        // со свежим ядром — при том что сам инбаунд на ноде работал.
        // Значение из базы здесь не спрашиваем: у этого протокола
        // другого транспорта не бывает.
        let network = if self.protocol == "hysteria" { "hysteria" } else { &self.network };

        let mut stream = json!({
            "network": network,
            "security": if self.security == "none" { "none" } else { &self.security },
        });

        if self.protocol == "hysteria" {
            stream["hysteriaSettings"] = json!({"version":2,"auth":self.credential()});
        }

        match self.security.as_str() {
            "reality" => {
                stream["realitySettings"] = json!({
                    "serverName": self.sni.clone().unwrap_or_default(),
                    "fingerprint": self.fingerprint.clone().unwrap_or_else(|| "chrome".into()),
                    "publicKey": self.public_key.clone().unwrap_or_default(),
                    "shortId": self.short_id.clone().unwrap_or_default(),
                    // «/», а не пустая строка. Поле — путь, и пустое
                    // значение вне описания формата: Xray его прощает,
                    // а часть клиентов на таком конфиге не поднимается
                    // вовсе — локация просто не отвечает на проверку связи.
                    "spiderX": "/",
                });
            }
            "tls" => {
                stream["tlsSettings"] = json!({
                    "serverName": self.sni.clone().unwrap_or_default(),
                    "fingerprint": self.fingerprint.clone().unwrap_or_default(),
                    "alpn": self.alpn.clone()
                        .map(|a| a.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>())
                        .unwrap_or_default(),
                });
            }
            _ => {}
        }

        // Настройки транспорта. Их отсутствие для gRPC/xHTTP/h2 означало
        // конфиг, который загружается, но не соединяется.
        let host = self.host_header.clone().or_else(|| self.sni.clone()).unwrap_or_default();
        let path = self.path.clone().unwrap_or_else(|| "/".into());
        match self.network.as_str() {
            "ws" => {
                stream["wsSettings"] = json!({ "path": path, "headers": { "Host": host } });
            }
            "httpupgrade" => {
                stream["httpupgradeSettings"] = json!({ "path": path, "host": host });
            }
            "xhttp" | "splithttp" => {
                stream["xhttpSettings"] = json!({ "path": path, "host": host, "mode": "auto" });
            }
            "grpc" => {
                stream["grpcSettings"] = json!({
                    "serviceName": self.service_name.clone().unwrap_or_default(),
                    "multiMode": false,
                });
            }
            "h2" | "http" => {
                stream["httpSettings"] = json!({
                    "path": path,
                    "host": if host.is_empty() { vec![] } else { vec![host] },
                });
            }
            "kcp" | "mkcp" => {
                // Пустой объект, а не отсутствие секции: клиент должен
                // явно понять, что транспорт mKCP, а не голый TCP.
                stream["kcpSettings"] = json!({ "mtu": 1350, "tti": 50 });
            }
            _ => {}
        }
        if self.security=="tls" && self.options.get("allow_insecure").is_some(){stream["tlsSettings"]["allowInsecure"]=self.options["allow_insecure"].clone();}
        for key in ["sockopt","finalmask"]{if self.options[key].is_object(){stream[key]=self.options[key].clone();}}
        if matches!(self.network.as_str(),"xhttp"|"splithttp") {if let Some(options)=self.options["xhttp"].as_object(){for (key,value) in options {stream["xhttpSettings"][key]=value.clone();}}}
        stream
    }

    /// outbound для конфига Xray.
    ///
    /// Форма settings зависит от протокола: у VLESS/VMess это `vnext`, у
    /// Trojan и Shadowsocks — `servers`. Раньше всем строился `vnext`,
    /// и конфиг с trojan-хостом Xray просто не принимал.
    pub fn to_xray_outbound(&self) -> Value {
        let settings = match self.protocol.as_str() {
            "trojan" => json!({
                "servers": [{
                    "address": self.address,
                    "port": self.port,
                    "password": self.credential(),
                }]
            }),
            "shadowsocks" => json!({
                "servers": [{
                    "address": self.address,
                    "port": self.port,
                    "method": self.method.clone().unwrap_or_else(|| "2022-blake3-aes-128-gcm".into()),
                    "password": self.credential(),
                    "uot": true,
                }]
            }),
            // Server location belongs to the protocol; auth to hysteriaSettings.
            "hysteria" => json!({
                "version": 2,
                "address": self.address,
                "port": self.port,
            }),
            "vmess" => json!({
                "vnext": [{
                    "address": self.address,
                    "port": self.port,
                    "users": [{ "id": self.uuid, "alterId": 0, "security": "auto" }],
                }]
            }),
            // vless и всё, что ведёт себя как он
            _ => json!({
                "vnext": [{
                    "address": self.address,
                    "port": self.port,
                    "users": [{
                        "id": self.uuid,
                        "encryption": "none",
                        "flow": if self.security == "reality" && self.network == "tcp" {
                            "xtls-rprx-vision"
                        } else { "" },
                    }],
                }]
            }),
        };

        let mut outbound=json!({
            "tag": self.remark,
            "protocol": self.protocol,
            "settings": settings,
            "streamSettings": self.stream_settings(),
        });
        if self.options["mux"].is_object(){outbound["mux"]=self.options["mux"].clone();}
        outbound
    }

    /// proxy-запись для Clash / mihomo (YAML).
    pub fn to_clash_proxy(&self) -> String {
        let mut lines = vec![
            format!("  - name: {}", serde_json::to_string(&self.remark).unwrap()),
            format!("    type: {}", self.protocol),
            format!("    server: {}", self.address),
            format!("    port: {}", self.port),
            format!("    uuid: {}", self.uuid),
            "    udp: true".to_string(),
        ];
        if self.protocol == "vless" {
            lines.push("    encryption: none".into());
        }
        lines.push(format!("    network: {}", self.network));

        if self.security == "reality" {
            lines.push("    tls: true".into());
            if let Some(sni) = &self.sni {
                lines.push(format!("    servername: {sni}"));
            }
            lines.push("    reality-opts:".into());
            lines.push(format!(
                "      public-key: {}",
                self.public_key.clone().unwrap_or_default()
            ));
            if let Some(sid) = self.short_id.as_ref().filter(|s| !s.is_empty()) {
                lines.push(format!("      short-id: {sid}"));
            }
            lines.push(format!(
                "    client-fingerprint: {}",
                self.fingerprint.clone().unwrap_or_else(|| "chrome".into())
            ));
            if self.network == "tcp" {
                lines.push("    flow: xtls-rprx-vision".into());
            }
        } else if self.security == "tls" {
            lines.push("    tls: true".into());
            if let Some(sni) = &self.sni {
                lines.push(format!("    servername: {sni}"));
            }
        }

        if self.network == "ws" {
            lines.push("    ws-opts:".into());
            lines.push(format!(
                "      path: {}",
                self.path.clone().unwrap_or_else(|| "/".into())
            ));
        }
        if let Some(value)=self.options["allow_insecure"].as_bool(){if self.security=="tls"{lines.push(format!("    skip-cert-verify: {value}"));}}
        if let Some(description)=self.options["server_description"].as_str().filter(|s|!s.is_empty()){lines.push(format!("    description: {}",serde_json::to_string(description).unwrap()));}
        lines.join("\n")
    }
}

/// Base64 от списка share-ссылок — исторический формат подписки.
/// Хост-заглушка: причина вместо списка локаций.
///
/// Когда отдавать нечего — истёк срок, кончился трафик, исчерпан лимит
/// устройств, — приложению всё равно нужен **валидный конфиг того
/// формата, который оно просило**. Раньше в таких случаях уходила строка
/// вида `# причина`: для Happ, который ждёт Xray JSON, это невалидный
/// JSON, и человек видел ошибку разбора вместо объяснения.
///
/// Поэтому собираем настоящую запись, у которой в имени — причина, а
/// адрес заведомо никуда не ведёт. Приложение показывает её в списке
/// серверов; подключиться к ней нельзя, да и незачем.
pub fn stub_host(reason: &str) -> HostEntry {
    HostEntry {
        remark: reason.to_string(),
        // Петля и порт 1: соединение обрывается сразу и не уходит наружу.
        address: "127.0.0.1".into(),
        port: 1,
        protocol: "vless".into(),
        network: "tcp".into(),
        security: "none".into(),
        sni: None,
        fingerprint: None,
        alpn: None,
        path: None,
        public_key: None,
        short_id: None,
        // Фиксированный нулевой UUID: это не клиент, а надпись.
        uuid: "00000000-0000-0000-0000-000000000000".into(),
        method: None,
        service_name: None,
        server_key: None,
        host_header: None,
        options: serde_json::json!({}),
    }
}

pub fn render_base64(hosts: &[HostEntry]) -> String {
    let links: Vec<String> = hosts.iter().map(|h| h.to_share_link()).collect();
    base64::engine::general_purpose::STANDARD.encode(links.join("\n"))
}

/// Открытый список ссылок — некоторые клиенты просят именно его.
pub fn render_plain(hosts: &[HostEntry]) -> String {
    hosts
        .iter()
        .map(|h| h.to_share_link())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Полный конфиг Xray с маршрутизацией и балансировкой по локациям.
/// Подписка в формате Xray JSON (Happ, Streisand, v2box, NekoBox).
///
/// Отдаём **массив** конфигов — по одному на хост. Приложение строит из него
/// список серверов и подписывает каждый полем `remarks`. Если вернуть один
/// объект со всеми хостами в `outbounds`, в списке появится ровно одна
/// безымянная запись: выбрать локацию будет нечем, а лишние outbound'ы
/// окажутся мёртвым грузом — трафик всё равно уходит в первый по порядку.
pub fn render_xray_json(hosts: &[HostEntry], _profile_title: &str) -> Value {
    Value::Array(hosts.iter().map(one_xray_config).collect())
}

/// Один сервер — один самодостаточный конфиг.
///
/// Порты локальных входов у всех локаций одни и те же, и это нормально:
/// клиент держит поднятым один конфиг за раз. Разводить их по локациям
/// я пробовал, решив, что в этом причина «n/a» у проверки связи, — причина
/// оказалась другой (её не было вовсе), а расхождение с тем, как делают
/// все, осталось бы на ровном месте.
fn one_xray_config(host: &HostEntry) -> Value {
    let socks = 10808;
    let http = 10809;
    let mut proxy = host.to_xray_outbound();
    // Правила маршрутизации ссылаются на теги: тег прокси должен быть
    // предсказуемым, а не названием локации с флагом и пробелами.
    proxy["tag"] = json!("proxy");

    json!({
        // Только имя локации: название подписки приложение уже получило
        // заголовком profile-title, и повторять его в каждой строке списка
        // незачем — оно съедает всю видимую ширину.
        "remarks": host.remark,
        "log": { "loglevel": "warning" },
        // Замер задержки. Это не украшение: клиентские приложения
        // показывают в списке локаций именно эти цифры, а сами ничего
        // не меряют. Без блока напротив каждой локации стоит «n/a» —
        // и выглядит это как «сервер не отвечает», хотя он работает.
        //
        // Механизм принадлежит Xray: он раз в несколько минут ходит
        // GET-запросом через каждый перечисленный исходящий и запоминает
        // время ответа. Тайм-аут короткий нарочно — «медленно» для выбора
        // локации то же самое, что «недоступно».
        "burstObservatory": {
            "subjectSelector": ["proxy"],
            "pingConfig": {
                "destination": "https://www.gstatic.com/generate_204",
                "interval": "3m",
                "sampling": 3,
                "timeout": "2s",
                "httpMethod": "GET"
            }
        },
        // queryStrategy обязателен там же, где domainStrategy: без него
        // разрешение имён и маршрутизация расходятся в том, какие адреса
        // считать подходящими.
        "dns": {
            "servers": [{ "address": "1.1.1.1", "skipFallback": false }],
            "queryStrategy": "IPIfNonMatch"
        },
        "inbounds": [
            // sniffing нужен не только маршрутизации: по нему клиент
            // определяет, куда на самом деле идёт соединение. Без него
            // часть приложений не может даже проверить связь — им нечего
            // показать в качестве адреса назначения.
            { "tag": "socks", "port": socks, "protocol": "socks",
              "listen": "127.0.0.1", "settings": { "udp": true },
              "sniffing": { "enabled": true,
                            "destOverride": ["http", "tls", "quic"],
                            "routeOnly": false } },
            { "tag": "http", "port": http, "protocol": "http", "listen": "127.0.0.1",
              "sniffing": { "enabled": true,
                            "destOverride": ["http", "tls", "quic"],
                            "routeOnly": false } }
        ],
        // direct и block обязаны существовать: routing ниже ссылается на них
        // по тегу, а Xray несуществующий тег молча игнорирует — правило просто
        // никогда не сработает, и локальный трафик уйдёт в туннель.
        "outbounds": [
            proxy,
            { "tag": "direct", "protocol": "freedom" },
            { "tag": "block", "protocol": "blackhole" }
        ],
        // Локальные сети перечисляем диапазонами, а НЕ через `geoip:private`:
        // это правило требует файл geoip.dat, которого у клиента может не быть,
        // и тогда не грузится весь конфиг целиком, а не одно правило.
        "routing": {
            // IPIfNonMatch, а не AsIs: в режиме туннеля приложение отдаёт
            // движку голые адреса, и правила по доменам без разрешения
            // имён просто не срабатывают.
            "domainStrategy": "IPIfNonMatch",
            "rules": [
                // QUIC гасим. Он ходит по udp:443, и браузер предпочитает
                // его всему остальному; через туннель, рассчитанный на TCP,
                // это выглядит как «страницы не открываются» при исправном
                // соединении. Заблокированный QUIC браузер переживает —
                // молча возвращается на TCP.
                { "type": "field", "network": "udp", "port": "443", "outboundTag": "block" },
                { "type": "field", "protocol": ["bittorrent"], "outboundTag": "block" },
                // Локальные имена мимо туннеля: иначе принтер или роутер
                // по имени ищется на другом конце света.
                { "type": "field", "outboundTag": "direct", "domain": [
                    "localhost", "localhost.localdomain", "local", "*.local",
                    "*.localdomain", "*.lan", "*.internal"
                ]},
                { "type": "field", "outboundTag": "direct", "ip": [
                    "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8", "169.254.0.0/16",
                    "172.16.0.0/12", "192.168.0.0/16", "::1/128", "fc00::/7", "fe80::/10"
                ]},
                // Всё остальное — в туннель, явным правилом.
                //
                // Этого правила не было: мы полагались на то, что движок
                // отправит непопавшее в первый исходящий. В обычном Xray
                // так и есть, но приложение в режиме туннеля отдаёт ему и
                // UDP, для которого умолчание работает иначе. Итог был
                // такой: Hysteria (она несёт UDP сама) работала, а VLESS
                // поверх TCP — нет, при полностью исправном сервере.
                { "type": "field", "network": "tcp,udp", "outboundTag": "proxy" }
            ]
        }
    })
}

/// Конфиг Clash / mihomo.
pub fn render_clash(hosts: &[HostEntry], profile_title: &str) -> String {
    let profile_title=profile_title.replace(['\r','\n']," ");
    let names: Vec<String> = hosts
        .iter()
        .map(|h| format!("      - {}", serde_json::to_string(&h.remark).unwrap()))
        .collect();
    let proxies: Vec<String> = hosts.iter().map(|h| h.to_clash_proxy()).collect();

    format!(
        "# {profile_title}\n\
         mixed-port: 7890\n\
         allow-lan: false\n\
         mode: rule\n\
         log-level: warning\n\
         \n\
         proxies:\n{}\n\
         \n\
         proxy-groups:\n\
         \x20 - name: \"Авто\"\n\
         \x20   type: url-test\n\
         \x20   url: http://www.gstatic.com/generate_204\n\
         \x20   interval: 300\n\
         \x20   proxies:\n{}\n\
         \x20 - name: \"Выбор\"\n\
         \x20   type: select\n\
         \x20   proxies:\n\
         \x20     - \"Авто\"\n{}\n\
         \n\
         rules:\n\
         \x20 - GEOIP,private,DIRECT\n\
         \x20 - MATCH,Выбор\n",
        proxies.join("\n"),
        names.join("\n"),
        names.join("\n")
    )
}

/// Конфиг sing-box.
pub fn render_singbox(hosts: &[HostEntry], profile_title: &str) -> Value {
    let outbounds: Vec<Value> = hosts
        .iter()
        .map(|h| {
            let mut o = json!({
                "type": h.protocol,
                "tag": h.remark,
                "server": h.address,
                "server_port": h.port,
                "uuid": h.uuid,
            });
            if h.security == "reality" {
                o["tls"] = json!({
                    "enabled": true,
                    "server_name": h.sni.clone().unwrap_or_default(),
                    "utls": { "enabled": true,
                              "fingerprint": h.fingerprint.clone().unwrap_or_else(|| "chrome".into()) },
                    "reality": { "enabled": true,
                                 "public_key": h.public_key.clone().unwrap_or_default(),
                                 "short_id": h.short_id.clone().unwrap_or_default() },
                });
                if h.network == "tcp" {
                    o["flow"] = json!("xtls-rprx-vision");
                }
            } else if h.security == "tls" {
                o["tls"] = json!({
                    "enabled": true,
                    "server_name": h.sni.clone().unwrap_or_default(),
                });
            }
            if h.network == "ws" {
                o["transport"] = json!({
                    "type": "ws",
                    "path": h.path.clone().unwrap_or_else(|| "/".into()),
                });
            }
            if h.security=="tls"{if let Some(value)=h.options["allow_insecure"].as_bool(){o["tls"]["insecure"]=json!(value);}}
            o
        })
        .collect();

    let tags: Vec<String> = hosts.iter().map(|h| h.remark.clone()).collect();
    let mut all = outbounds;
    all.push(json!({ "type": "selector", "tag": profile_title, "outbounds": tags }));
    all.push(json!({ "type": "direct", "tag": "direct" }));

    json!({
        "log": { "level": "warn" },
        "dns": { "servers": [{ "address": "1.1.1.1" }] },
        "inbounds": [{ "type": "mixed", "tag": "mixed-in",
                       "listen": "127.0.0.1", "listen_port": 2080 }],
        "outbounds": all,
    })
}

#[cfg(test)]
mod tests {

    /// Хост нужного протокола и транспорта — конструктор для проверок.
    fn host_of(protocol: &str, network: &str, security: &str) -> HostEntry {
        HostEntry {
            remark: "🇩🇪 Тест".into(),
            address: "de.example.net".into(),
            port: 443,
            protocol: protocol.into(),
            network: network.into(),
            security: security.into(),
            sni: Some("de.example.net".into()),
            fingerprint: Some("chrome".into()),
            alpn: None,
            path: Some("/tunnel".into()),
            public_key: Some("PUBKEY".into()),
            short_id: Some("ab12".into()),
            method: Some("2022-blake3-aes-128-gcm".into()),
            service_name: Some("grpcsvc".into()),
            server_key: None,
            host_header: None,
        options: serde_json::json!({}),
            uuid: "8a2f1c9e-3b47-4e5d-9f01-c2ab34d96e11".into(),
        }
    }

    #[test]
    fn vmess_ссылка_это_base64_от_json() {
        let link = host_of("vmess", "ws", "tls").to_share_link();
        assert!(link.starts_with("vmess://"), "получили: {link}");
        let raw = base64::engine::general_purpose::STANDARD
            .decode(link.trim_start_matches("vmess://"))
            .expect("после vmess:// должен быть base64");
        let v: Value = serde_json::from_slice(&raw).expect("внутри должен быть JSON");
        assert_eq!(v["add"], "de.example.net");
        assert_eq!(v["net"], "ws");
        assert_eq!(v["tls"], "tls");
    }

    #[test]
    fn trojan_уходит_паролем_а_не_vnext() {
        let h = host_of("trojan", "tcp", "tls");
        let out = h.to_xray_outbound();
        assert!(out["settings"]["vnext"].is_null(), "у trojan не должно быть vnext");
        assert_eq!(out["settings"]["servers"][0]["password"], h.uuid);
        assert!(h.to_share_link().starts_with("trojan://"));
    }

    #[test]
    fn hysteria_отдаёт_пароль_плоскими_полями() {
        // Вложенные servers/vnext роняют разбор на стороне клиента —
        // проверено живым движком, поэтому форма закреплена тестом.
        let h = host_of("hysteria", "raw", "tls");
        let out = h.to_xray_outbound();
        assert_eq!(out["settings"]["version"], 2);
        assert_eq!(out["settings"]["address"], "de.example.net");
        assert_eq!(out["streamSettings"]["hysteriaSettings"]["auth"], h.uuid);
        assert!(out["settings"]["password"].is_null());
        assert!(out["settings"]["servers"].is_null());
        assert!(out["settings"]["vnext"].is_null());
        assert!(h.to_share_link().starts_with("hysteria2://"));
    }

    #[test]
    fn shadowsocks_клеит_серверный_ключ_с_личным() {
        // Многопользовательский режим ждёт пару «серверный:личный».
        // Без серверной половины движок отвечает «missing psk».
        let mut h = host_of("shadowsocks", "tcp", "none");
        h.server_key = Some("SERVERPSK".into());
        let pass = h.to_xray_outbound()["settings"]["servers"][0]["password"]
            .as_str().unwrap().to_string();
        assert!(pass.starts_with("SERVERPSK:"), "получили: {pass}");
        assert_eq!(pass.split(':').count(), 2);
    }

    #[test]
    fn shadowsocks_несёт_метод_и_ключ() {
        let h = host_of("shadowsocks", "tcp", "none");
        let out = h.to_xray_outbound();
        assert_eq!(out["settings"]["servers"][0]["method"], "2022-blake3-aes-128-gcm");
        let key = out["settings"]["servers"][0]["password"].as_str().unwrap();
        // 16 байт в base64 — 24 символа с выравниванием.
        assert_eq!(key.len(), 24, "ключ для 128-битного метода должен быть 16 байт");

        let link = h.to_share_link();
        assert!(link.starts_with("ss://"));
        let userinfo = link.trim_start_matches("ss://").split('@').next().unwrap();
        let decoded = String::from_utf8(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(userinfo).unwrap()
        ).unwrap();
        assert!(decoded.starts_with("2022-blake3-aes-128-gcm:"), "получили: {decoded}");
    }

    #[test]
    fn ключ_shadowsocks_привязан_к_uuid() {
        // Отзыв подписки меняет UUID — значит, должен менять и ключ,
        // иначе отзыв работал бы не для всех протоколов.
        let mut a = host_of("shadowsocks", "tcp", "none");
        let key_a = a.to_xray_outbound()["settings"]["servers"][0]["password"].clone();
        a.uuid = "11111111-2222-3333-4444-555555555555".into();
        let key_b = a.to_xray_outbound()["settings"]["servers"][0]["password"].clone();
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn транспорты_доезжают_до_клиента() {
        // Каждый транспорт обязан положить в конфиг свою секцию: без неё
        // подключение уходит «в никуда» и молча не работает.
        for (net, section) in [
            ("ws", "wsSettings"),
            ("grpc", "grpcSettings"),
            ("httpupgrade", "httpupgradeSettings"),
            ("xhttp", "xhttpSettings"),
            ("h2", "httpSettings"),
            ("kcp", "kcpSettings"),
        ] {
            let out = host_of("vless", net, "tls").to_xray_outbound();
            assert!(
                !out["streamSettings"][section].is_null(),
                "транспорт {net}: нет секции {section}"
            );
        }
        let grpc = host_of("vless", "grpc", "tls").to_xray_outbound();
        assert_eq!(grpc["streamSettings"]["grpcSettings"]["serviceName"], "grpcsvc");
    }

    #[test]
    fn имя_сервиса_grpc_попадает_в_ссылку() {
        let link = host_of("vless", "grpc", "tls").to_share_link();
        assert!(link.contains("serviceName=grpcsvc"), "получили: {link}");
    }

    #[test]
    fn flow_только_для_vless_reality_поверх_tcp() {
        assert!(host_of("vless", "tcp", "reality").to_share_link().contains("flow=xtls-rprx-vision"));
        assert!(!host_of("vless", "ws", "reality").to_share_link().contains("flow="));
        // У trojan этого параметра нет вовсе.
        assert!(!host_of("trojan", "tcp", "reality").to_share_link().contains("flow="));
    }

    use super::*;

    fn reality_host() -> HostEntry {
        HostEntry {
            remark: "🇳🇱 Амстердам".into(),
            address: "ams.example.net".into(),
            port: 443,
            protocol: "vless".into(),
            network: "tcp".into(),
            security: "reality".into(),
            sni: Some("yahoo.com".into()),
            fingerprint: Some("chrome".into()),
            alpn: None,
            path: None,
            public_key: Some("PBK123".into()),
            short_id: Some("ab12".into()),
            method: None,
            service_name: None,
            server_key: None,
            host_header: None,
        options: serde_json::json!({}),
            uuid: "8a2f1c9e-3b47-4e5d-9f01-c2ab34d96e11".into(),
        }
    }

    fn ws_host() -> HostEntry {
        HostEntry {
            remark: "🇹🇷 Стамбул".into(),
            address: "ist.example.net".into(),
            port: 443,
            protocol: "vless".into(),
            network: "ws".into(),
            security: "tls".into(),
            sni: Some("ist.example.net".into()),
            fingerprint: None,
            alpn: Some("h2,http/1.1".into()),
            path: Some("/ws".into()),
            public_key: None,
            short_id: None,
            method: None,
            service_name: None,
            server_key: None,
            host_header: None,
        options: serde_json::json!({}),
            uuid: "8a2f1c9e-3b47-4e5d-9f01-c2ab34d96e11".into(),
        }
    }

    #[test]
    fn share_link_содержит_обязательные_параметры_reality() {
        let link = reality_host().to_share_link();
        assert!(link.starts_with("vless://8a2f1c9e-3b47-4e5d-9f01-c2ab34d96e11@ams.example.net:443?"));
        for part in ["security=reality", "sni=yahoo.com", "pbk=PBK123", "sid=ab12", "fp=chrome"] {
            assert!(link.contains(part), "нет «{part}» в {link}");
        }
        // flow нужен только для reality+tcp
        assert!(link.contains("flow=xtls-rprx-vision"));
    }

    #[test]
    fn ws_хост_не_получает_flow_и_reality_полей() {
        let link = ws_host().to_share_link();
        assert!(!link.contains("flow="), "flow не применим к ws: {link}");
        assert!(!link.contains("pbk="));
        assert!(link.contains("type=ws"));
        assert!(link.contains("path=/ws"));
    }

    #[test]
    fn пробелы_в_ремарке_не_ломают_ссылку() {
        let mut h = reality_host();
        h.remark = "NL Amsterdam #1".into();
        let link = h.to_share_link();
        let frag = link.split('#').nth(1).unwrap();
        assert!(!frag.contains(' '), "пробел в фрагменте: {frag}");
        assert!(frag.contains("%20"), "пробел должен быть закодирован: {frag}");
        // Решётка внутри ремарки закодирована, значит '#' в ссылке ровно один —
        // разделитель фрагмента. Иначе клиент обрежет имя локации.
        assert_eq!(link.matches('#').count(), 1, "лишний '#' сломает разбор: {link}");
        assert!(frag.contains("%231"), "решётка не закодирована: {frag}");
    }

    #[test]
    fn base64_декодируется_обратно_в_ссылки() {
        let hosts = vec![reality_host(), ws_host()];
        let encoded = render_base64(&hosts);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .expect("валидный base64");
        let text = String::from_utf8(decoded).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().all(|l| l.starts_with("vless://")));
    }

    #[test]
    fn xray_json_отдаёт_конфиг_на_каждый_хост() {
        // Приложения строят список серверов из массива и подписывают
        // каждый пункт полем remarks. Один объект = одна безымянная запись.
        let hosts = vec![reality_host(), ws_host()];
        let cfg = render_xray_json(&hosts, "TEST");
        let list = cfg.as_array().expect("подписка должна быть массивом");
        assert_eq!(list.len(), 2);

        // Имя локации — то, что человек видит в списке серверов.
        assert_eq!(list[0]["remarks"], hosts[0].remark);
        assert_eq!(list[1]["remarks"], hosts[1].remark);

        let first = &list[0]["outbounds"][0];
        assert_eq!(first["tag"], "proxy");
        assert_eq!(first["protocol"], "vless");
        assert_eq!(first["streamSettings"]["security"], "reality");
        assert_eq!(first["streamSettings"]["realitySettings"]["publicKey"], "PBK123");

        let second = &list[1]["outbounds"][0];
        assert_eq!(second["streamSettings"]["network"], "ws");
        assert_eq!(second["streamSettings"]["wsSettings"]["path"], "/ws");
    }

    #[test]
    fn у_каждого_правила_есть_живой_outbound() {
        // Xray не ругается на несуществующий outboundTag — правило просто
        // никогда не сработает. Такую опечатку ловит только этот тест.
        let cfg = render_xray_json(&[reality_host(), ws_host()], "T");
        for c in cfg.as_array().unwrap() {
            let tags: Vec<&str> = c["outbounds"].as_array().unwrap()
                .iter().map(|o| o["tag"].as_str().unwrap()).collect();
            for r in c["routing"]["rules"].as_array().unwrap() {
                let want = r["outboundTag"].as_str().unwrap();
                assert!(tags.contains(&want), "правило шлёт в несуществующий {want}: {tags:?}");
            }
        }
    }

    #[test]
    fn clash_валидный_yaml_со_всеми_прокси() {
        let hosts = vec![reality_host(), ws_host()];
        let yaml = render_clash(&hosts, "TEST");
        assert!(yaml.contains("proxies:"));
        assert!(yaml.contains("proxy-groups:"));
        assert!(yaml.contains("reality-opts:"));
        assert!(yaml.contains("public-key: PBK123"));
        assert!(yaml.contains("ws-opts:"));
        // каждая нода должна попасть и в proxies, и в обе группы
        assert_eq!(yaml.matches("🇳🇱 Амстердам").count(), 3);
    }

    #[test]
    fn singbox_собирает_селектор_из_всех_нод() {
        let hosts = vec![reality_host(), ws_host()];
        let cfg = render_singbox(&hosts, "TEST");
        let outs = cfg["outbounds"].as_array().unwrap();
        // 2 ноды + селектор + direct
        assert_eq!(outs.len(), 4);
        assert_eq!(outs[0]["tls"]["reality"]["enabled"], true);
        assert_eq!(outs[1]["transport"]["type"], "ws");
        assert_eq!(outs[2]["type"], "selector");
        assert_eq!(outs[2]["outbounds"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn конфиг_не_требует_внешних_файлов() {
        // geoip.dat / geosite.dat есть не у каждого клиента. Если сослаться
        // на них в routing, Xray не загрузит конфиг ЦЕЛИКОМ — не работает
        // ничего, а сообщение об ошибке уводит совсем в другую сторону.
        let cfg = render_xray_json(&[reality_host()], "T").to_string();
        assert!(!cfg.contains("geoip:"), "ссылка на geoip.dat: {cfg}");
        assert!(!cfg.contains("geosite:"), "ссылка на geosite.dat: {cfg}");
    }

    #[test]
    fn локальные_сети_идут_напрямую() {
        let cfg = render_xray_json(&[reality_host()], "T");
        let rules = cfg[0]["routing"]["rules"].as_array().unwrap();
        // Правил с `direct` теперь два — по доменам и по адресам; ищем
        // именно адресное.
        let direct = rules.iter()
            .find(|r| r["outboundTag"] == "direct" && r["ip"].is_array())
            .expect("правило локальных сетей");
        let ips = direct["ip"].as_array().unwrap();
        assert!(ips.iter().any(|i| i == "192.168.0.0/16"));
        assert!(ips.iter().any(|i| i == "127.0.0.0/8"));
    }

    #[test]
    fn весь_остальной_трафик_уходит_в_туннель_явно() {
        // Этого правила не было: полагались на то, что движок отправит
        // непопавшее в первый исходящий. В приложении с туннелем так не
        // выходит для UDP — Hysteria работала, а VLESS поверх TCP нет,
        // при полностью исправном сервере.
        let cfg = render_xray_json(&[reality_host()], "T");
        let rules = cfg[0]["routing"]["rules"].as_array().unwrap();
        let последнее = rules.last().expect("хотя бы одно правило");
        assert_eq!(последнее["outboundTag"], "proxy");
        assert_eq!(последнее["network"], "tcp,udp");

        // И порядок: правило-перехватчик обязано быть последним, иначе
        // оно съест локальные сети и торренты.
        assert!(rules.len() >= 5, "правил меньше, чем ожидалось: {rules:?}");
        assert_eq!(rules[0]["port"], "443", "QUIC гасим первым");
        assert_eq!(rules[0]["outboundTag"], "block");
    }

    #[test]
    fn пустая_подписка_не_падает() {
        assert_eq!(render_plain(&[]), "");
        assert_eq!(render_base64(&[]), "");
        assert_eq!(render_xray_json(&[], "T").as_array().unwrap().len(), 0);
        assert!(render_clash(&[], "T").contains("proxies:"));
    }

    /// Заглушка должна быть валидным конфигом того формата, который
    /// просило приложение. Раньше вместо неё уходила строка «# причина»:
    /// Happ ждёт Xray JSON, получал невалидный JSON и показывал ошибку
    /// разбора вместо объяснения, почему нет серверов.
    #[test]
    fn stub_renders_in_every_format() {
        let reason = "Занято устройств: 3 из 3. Отключите лишнее";
        let hosts = [stub_host(reason)];

        // Xray JSON: разбирается, и причина — в имени сервера.
        let x = render_xray_json(&hosts, "T");
        let arr = x.as_array().expect("массив конфигов");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["remarks"], reason);

        // sing-box и Clash: тоже валидны и содержат причину.
        let sb = render_singbox(&hosts, "T");
        assert!(serde_json::to_string(&sb).unwrap().contains(reason));
        assert!(render_clash(&hosts, "T").contains(reason));

        // base64: декодируется в ссылку, в которой причина — подпись.
        let b64 = render_base64(&hosts);
        let raw = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let text = String::from_utf8(raw).unwrap();
        assert!(text.starts_with("vless://"), "ссылка: {text}");
        assert!(text.contains(&crate::formats::q(reason)) || text.contains(reason), "{text}");

        assert!(render_plain(&hosts).starts_with("vless://"));
    }

    /// Заглушка не должна никуда вести: адрес петлевой, порт заведомо
    /// закрытый. Иначе приложение станет упорно долбиться наружу.
    #[test]
    fn stub_goes_nowhere() {
        let h = stub_host("причина");
        assert_eq!(h.address, "127.0.0.1");
        assert_eq!(h.port, 1);
        assert_eq!(h.security, "none");
    }

    #[test]
    fn у_каждой_локации_есть_замер_задержки() {
        // Приложения не меряют задержку сами — они показывают то, что
        // намерил движок. Без этого блока в списке напротив локации
        // стоит «n/a», и человек читает это как «сервер не работает».
        let cfg = render_xray_json(&[reality_host(), ws_host()], "T");
        for c in cfg.as_array().expect("массив конфигов") {
            let o = &c["burstObservatory"];
            assert_eq!(o["subjectSelector"][0], "proxy", "мерить нужно исходящий прокси");
            assert_eq!(o["pingConfig"]["httpMethod"], "GET");
            assert!(o["pingConfig"]["destination"].as_str()
                        .is_some_and(|d| d.starts_with("https://")),
                    "проба должна идти по https");
        }
    }

    #[test]
    fn локальные_порты_привычные() {
        // Клиент держит поднятым один конфиг за раз, поэтому одинаковые
        // порты у всех локаций — норма. Я разводил их по локациям, решив,
        // что в этом причина «n/a»; причина оказалась в отсутствии замера.
        let cfg = render_xray_json(&[reality_host(), ws_host()], "T");
        for c in cfg.as_array().unwrap() {
            assert_eq!(c["inbounds"][0]["port"], 10808);
            assert_eq!(c["inbounds"][1]["port"], 10809);
        }
    }
}

//! Служебная часть конфига: учёт трафика.
//!
//! Чтобы считать трафик, движку нужен служебный вход на петле и включённая
//! статистика: агент ходит к нему по gRPC на `127.0.0.1:10085`. Всё это —
//! четыре блока, к самой раздаче доступа отношения не имеющие.
//!
//! Требовать их от человека неправильно. Конфиг переносят из другой
//! панели, берут из статьи, пишут руками — и в нём этих блоков нет.
//! Раньше такой конфиг сохранялся как есть, поднимался, работал — и
//! молча не считал ни байта: подписки не заканчивались по трафику, а
//! в панели у всех стоял ноль. Поэтому дописываем сами.
//!
//! Ничего чужого не трогаем: если блок уже есть, оставляем как есть.
//! Единственное исключение — флаги статистики в `policy`, без них
//! счётчики пустые даже при всём остальном на месте.

use serde_json::{json, Value};

/// Порт служебного входа. Тот же, что агент ждёт в `ENGINE_API`.
pub const API_PORT: i64 = 10085;
/// Тег служебного входа и выхода статистики.
pub const API_TAG: &str = "api";
pub const API_INBOUND_TAG: &str = "api-in";

/// Дописать в конфиг всё, без чего не считается трафик.
///
/// Возвращает `(конфиг, что_добавили)` — список для человека: он должен
/// понимать, что сохранённый конфиг отличается от вставленного.
pub fn ensure_service_parts(config: &Value) -> (Value, Vec<String>) {
    let mut cfg = config.clone();
    let mut добавлено = Vec::new();

    if !cfg.is_object() {
        return (cfg, добавлено);
    }

    // ── имя транспорта ──
    //
    // Xray зовёт один и тот же транспорт и `tcp`, и `raw`, и принимает
    // оба. Но решение «нужен ли flow» принимают порознь три стороны:
    // панель при сборке ссылки, агент при записи клиентов в конфиг ноды
    // и сам движок. Пока имя разное, они расходятся — и расходятся тихо.
    //
    // Так и вышло: панель считала `raw` тем же `tcp` и выдавала ссылку
    // с `xtls-rprx-vision`, а агент старой сборки сравнивал имя буквально,
    // видел `raw` и заводил клиента без flow. Порт открыт, рукопожатие
    // Reality проходит, а соединение молча висит до таймаута.
    //
    // Поэтому имя приводим в самом конфиге, который уезжает на ноды:
    // тогда согласие не зависит от того, когда на ноде обновят агента.
    let mut тронули_транспорт = false;
    if let Some(список) = cfg["inbounds"].as_array_mut() {
        for i in список {
            if i["streamSettings"]["network"] == "raw" {
                i["streamSettings"]["network"] = json!("tcp");
                тронули_транспорт = true;
            }
        }
    }
    if тронули_транспорт {
        добавлено.push("транспорт raw → tcp (то же самое, но одним именем)".into());
    }

    // ── api ──
    if cfg.get("api").and_then(|v| v.get("tag")).is_none() {
        cfg["api"] = json!({ "tag": API_TAG, "services": ["StatsService"] });
        добавлено.push("api".into());
    }

    // Preserve other API services, while always enabling managed traffic stats.
    if let Some(api) = cfg["api"].as_object_mut() {
        let services = api.entry("services").or_insert_with(|| json!([]));
        if let Some(list) = services.as_array_mut() {
            if !list.iter().any(|s| s == "StatsService") { list.push(json!("StatsService")); добавлено.push("api StatsService".into()); }
        }
    }

    // ── stats ──
    if cfg.get("stats").is_none() {
        cfg["stats"] = json!({});
        добавлено.push("stats".into());
    }

    // ── policy ──
    //
    // Здесь именно дописываем флаги, а не пропускаем готовый блок:
    // policy заводят и ради других настроек (таймауты, буферы), и тогда
    // блок есть, а счётчиков нет.
    let mut тронули_policy = false;
    {
        let уровень = вложенный(&mut cfg, &["policy", "levels", "0"]);
        for поле in ["statsUserUplink", "statsUserDownlink"] {
            if уровень.get(поле) != Some(&json!(true)) {
                уровень[поле] = json!(true);
                тронули_policy = true;
            }
        }
    }
    {
        let system = вложенный(&mut cfg, &["policy", "system"]);
        for поле in ["statsInboundUplink", "statsInboundDownlink"] {
            if system.get(поле) != Some(&json!(true)) {
                system[поле] = json!(true);
                тронули_policy = true;
            }
        }
    }
    if тронули_policy {
        добавлено.push("policy (счётчики трафика)".into());
    }

    // ── служебный вход ──
    //
    // Ищем по порту, а не по тегу: тег в чужом конфиге может быть любым,
    // а занять петлю дважды одним портом нельзя — движок не поднимется.
    if !cfg["inbounds"].is_array() {
        cfg["inbounds"] = json!([]);
    }
    let занят = cfg["inbounds"]
        .as_array()
        .map(|a| a.iter().any(|i| i["port"].as_i64() == Some(API_PORT)))
        .unwrap_or(false);
    if !занят {
        if let Some(a) = cfg["inbounds"].as_array_mut() {
            // Первым в списке: служебный вход не должен теряться среди
            // рабочих, когда конфиг читают глазами.
            a.insert(0, json!({
                "tag": API_INBOUND_TAG,
                "listen": "127.0.0.1",
                "port": API_PORT,
                "protocol": "dokodemo-door",
                "settings": { "address": "127.0.0.1" },
            }));
            добавлено.push("служебный вход api-in".into());
        }
    }

    // ── правило маршрутизации ──
    //
    // Тег берём у того входа, что реально слушает служебный порт: если
    // конфиг пришёл со своим служебным входом под другим именем, правило
    // должно указывать на него, а не на выдуманный «api-in».
    let тег_входа = cfg["inbounds"]
        .as_array()
        .and_then(|a| a.iter().find(|i| i["port"].as_i64() == Some(API_PORT)))
        .and_then(|i| i["tag"].as_str())
        .unwrap_or(API_INBOUND_TAG)
        .to_string();
    let тег_api = cfg["api"]["tag"].as_str().unwrap_or(API_TAG).to_string();

    {
        let routing = вложенный(&mut cfg, &["routing"]);
        if !routing["rules"].is_array() {
            routing["rules"] = json!([]);
        }
    }
    let есть = cfg["routing"]["rules"]
        .as_array()
        .map(|a| {
            a.iter().any(|r| {
                r["outboundTag"].as_str() == Some(&тег_api)
                    && r["inboundTag"]
                        .as_array()
                        .map(|t| t.iter().any(|x| x.as_str() == Some(&тег_входа)))
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !есть {
        if let Some(a) = cfg["routing"]["rules"].as_array_mut() {
            // Строго первым: маршрутизация срабатывает по первому
            // совпадению, и чужое правило «всё в блок» перехватило бы
            // служебный запрос раньше нашего.
            a.insert(0, json!({
                "type": "field",
                "inboundTag": [тег_входа],
                "outboundTag": тег_api,
            }));
            добавлено.push("правило маршрутизации к api".into());
        }
    }

    (cfg, добавлено)
}

/// Вложенный объект по пути, создавая недостающие звенья.
///
/// Чужой конфиг бывает каким угодно: `policy` нет вовсе, есть, но не
/// объект, есть с `levels`, но без нуля. Разбирать это по месту — три
/// проверки на каждое поле; здесь один проход.
fn вложенный<'a>(корень: &'a mut Value, путь: &[&str]) -> &'a mut Value {
    let mut узел = корень;
    for ключ in путь {
        if !узел[*ключ].is_object() {
            узел[*ключ] = json!({});
        }
        узел = узел.get_mut(*ключ).expect("объект только что создан");
    }
    узел
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Конфиг ровно в том виде, в каком его отдаёт другая панель:
    /// без api, stats, policy и служебного входа.
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
    fn чужой_конфиг_получает_учёт_трафика() {
        let (cfg, добавлено) = ensure_service_parts(&чужой());

        assert_eq!(cfg["api"]["tag"], "api");
        assert!(cfg["stats"].is_object());
        assert_eq!(cfg["policy"]["levels"]["0"]["statsUserUplink"], true);
        assert_eq!(cfg["policy"]["system"]["statsInboundUplink"], true);

        let входы = cfg["inbounds"].as_array().unwrap();
        assert_eq!(входы[0]["port"], API_PORT, "служебный вход первым");
        assert_eq!(входы[0]["listen"], "127.0.0.1");
        // Рабочий инбаунд не потерялся и не изменился.
        assert_eq!(входы[1]["tag"], "FIN_VLESS");
        assert_eq!(входы[1]["streamSettings"]["realitySettings"]["target"], "example.com:443");

        let правила = cfg["routing"]["rules"].as_array().unwrap();
        assert_eq!(правила[0]["outboundTag"], "api", "правило к api — первым");
        assert_eq!(правила[0]["inboundTag"][0], "api-in");
        // Чужие правила на месте и в прежнем порядке.
        assert_eq!(правила[1]["outboundTag"], "BLOCK");
        assert_eq!(правила[2]["protocol"][0], "bittorrent");

        assert!(!добавлено.is_empty(), "человеку надо сказать, что дописали");
    }

    #[test]
    fn транспорт_приводится_к_одному_имени() {
        // Панель, агент и движок решают «нужен ли flow» порознь. Пока
        // имя разное, они расходятся молча: ссылка уходит с
        // xtls-rprx-vision, а клиента на ноде заводят без него —
        // соединение висит до таймаута при открытом порте.
        let (cfg, добавлено) = ensure_service_parts(&чужой());
        let рабочий = cfg["inbounds"].as_array().unwrap().iter()
            .find(|i| i["tag"] == "FIN_VLESS").unwrap();
        assert_eq!(рабочий["streamSettings"]["network"], "tcp");
        assert!(добавлено.iter().any(|s| s.contains("транспорт")),
                "человеку надо сказать: {добавлено:?}");
        assert_eq!(рабочий["streamSettings"]["realitySettings"]["target"], "example.com:443");
        assert_eq!(рабочий["streamSettings"]["security"], "reality");
    }

    #[test]
    fn повторный_проход_ничего_не_меняет() {
        // Иначе при каждом сохранении конфиг обрастал бы копиями
        // служебного входа и правил.
        let (раз, _) = ensure_service_parts(&чужой());
        let (два, добавлено) = ensure_service_parts(&раз);
        assert_eq!(раз, два);
        assert!(добавлено.is_empty(), "во второй раз добавлять нечего: {добавлено:?}");
    }

    #[test]
    fn свой_служебный_вход_не_дублируется() {
        // Конфиг уже со статистикой, но под другими именами: занимать
        // порт 10085 второй раз нельзя — движок не поднимется.
        let свой = json!({
            "api": { "tag": "МОЙ_API", "services": ["StatsService"] },
            "stats": {},
            "inbounds": [{ "tag": "мой-вход", "port": 10085, "listen": "127.0.0.1",
                           "protocol": "dokodemo-door",
                           "settings": { "address": "127.0.0.1" } }],
            "outbounds": [{ "protocol": "freedom" }],
        });
        let (cfg, _) = ensure_service_parts(&свой);

        let входы = cfg["inbounds"].as_array().unwrap();
        assert_eq!(входы.len(), 1, "второй служебный вход не нужен");
        assert_eq!(входы[0]["tag"], "мой-вход");
        assert_eq!(cfg["api"]["tag"], "МОЙ_API", "чужой тег не переписываем");

        // Правило должно указывать на существующие теги, а не на наши.
        let правило = &cfg["routing"]["rules"][0];
        assert_eq!(правило["inboundTag"][0], "мой-вход");
        assert_eq!(правило["outboundTag"], "МОЙ_API");
    }

    #[test]
    fn чужой_policy_дополняется_а_не_затирается() {
        let с_политикой = json!({
            "policy": { "levels": { "0": { "handshake": 4, "connIdle": 300 } } },
            "inbounds": [], "outbounds": [],
        });
        let (cfg, _) = ensure_service_parts(&с_политикой);
        assert_eq!(cfg["policy"]["levels"]["0"]["handshake"], 4, "чужое сохранено");
        assert_eq!(cfg["policy"]["levels"]["0"]["connIdle"], 300);
        assert_eq!(cfg["policy"]["levels"]["0"]["statsUserUplink"], true, "наше дописано");
    }

    #[test]
    fn пустой_конфиг_не_роняет() {
        let (cfg, _) = ensure_service_parts(&json!({}));
        assert_eq!(cfg["inbounds"].as_array().unwrap().len(), 1);
        assert!(cfg["routing"]["rules"].is_array());
    }
}

#[cfg(test)] mod managed_stats_tests { use super::*; #[test] fn existing_api_retains_services_and_enables_accounting(){let (v,_)=ensure_service_parts(&json!({"api":{"tag":"custom-api","services":["HandlerService"]},"policy":{"levels":{"0":{"statsUserUplink":false,"connIdle":200}}},"inbounds":[]}));assert_eq!(v["api"]["services"],json!(["HandlerService","StatsService"]));assert_eq!(v["policy"]["levels"]["0"]["statsUserUplink"],true);assert_eq!(v["policy"]["levels"]["0"]["connIdle"],200);}}

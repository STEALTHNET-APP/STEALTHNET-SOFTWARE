//! Генерация ключей для протоколов.
//!
//! Панель — источник истины по ключам: приватная часть уходит в конфиг ноды,
//! публичная в подписку клиента. Генерировать их на ноде нельзя — тогда
//! панель не сможет собрать клиенту рабочую ссылку.

use base64::Engine;
use rand::RngCore;

/// Пара ключей Reality плюс short id.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RealityKeys {
    /// Уходит в `realitySettings.privateKey` конфига ноды. Наружу не отдаём.
    pub private_key: String,
    /// Уходит клиенту как `pbk` — по нему он проверяет сервер.
    pub public_key: String,
    /// `sid`: короткий идентификатор, разный у разных клиентских групп.
    pub short_id: String,
}

/// Ключи X25519 в том виде, в котором их ждёт Xray:
/// base64-url без выравнивания.
pub fn generate_reality_keys() -> RealityKeys {
    use x25519_dalek::{PublicKey, StaticSecret};

    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let secret = StaticSecret::from(seed);
    let public = PublicKey::from(&secret);

    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;

    // shortId — произвольные 8 hex-символов. Пустой тоже допустим,
    // но с ним сложнее развести разные группы клиентов.
    let mut sid = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut sid);

    RealityKeys {
        private_key: b64.encode(secret.to_bytes()),
        public_key: b64.encode(public.as_bytes()),
        short_id: hex::encode(sid),
    }
}

/// Публичный ключ Reality из приватного.
///
/// Клиент проверяет сервер по публичной половине, и назвать её нужно
/// ровно ту, что соответствует приватной в конфиге ноды. Хранить обе
/// половины раздельно — значит однажды получить расхождение; выводим
/// вторую из первой.
///
/// `None` — приватный ключ не разобрался: не base64 или не 32 байта.
pub fn reality_public_from_private(private_key: &str) -> Option<String> {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    // Xray принимает и стандартный алфавит, и url-safe; приводим к одному.
    let normalized = private_key.trim().replace('+', "-").replace('/', "_");
    let normalized = normalized.trim_end_matches('=');
    let raw = b64.decode(normalized).ok()?;
    let bytes: [u8; 32] = raw.try_into().ok()?;
    let secret = x25519_dalek::StaticSecret::from(bytes);
    Some(b64.encode(x25519_dalek::PublicKey::from(&secret).as_bytes()))
}

/// Подставить в заготовку профиля секреты, которые незачем спрашивать.
///
/// Заготовки приходят с местами заглавными буквами. Часть из них —
/// просто случайные значения: ключи Reality, короткий идентификатор,
/// пароль Shadowsocks, путь веб-сокета. Требовать их от человека
/// бессмысленно вдвойне: генератор ключей живёт на странице профиля, а
/// страница появляется только после создания — заготовку с Reality
/// нельзя было создать вообще.
///
/// Домены и пути к сертификатам не трогаем: их не выдумать, и подставить
/// туда что попало — значит отдать движку заведомо нерабочий конфиг.
///
/// Возвращает конфиг с заполненными местами.
pub fn fill_secrets(config: &serde_json::Value) -> serde_json::Value {
    let mut text = config.to_string();

    if text.contains("ВАШ_ПРИВАТНЫЙ_КЛЮЧ") || text.contains("ВАШ_SHORT_ID") {
        let k = generate_reality_keys();
        text = text.replace("ВАШ_ПРИВАТНЫЙ_КЛЮЧ", &k.private_key);
        text = text.replace("ВАШ_SHORT_ID", &k.short_id);
    }

    if text.contains("СЕРВЕРНЫЙ_КЛЮЧ") {
        // Shadowsocks 2022 ждёт ровно 32 байта в base64 — с обычным
        // паролем движок откажется стартовать.
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        let ключ = base64::engine::general_purpose::STANDARD.encode(raw);
        text = text.replace("СЕРВЕРНЫЙ_КЛЮЧ", &ключ);
    }

    if text.contains("ВАШ_ПУТЬ") {
        // Случайный путь лучше угадываемого: перебор известных путей —
        // первое, чем пробуют вслепую нащупать веб-сокет.
        let mut raw = [0u8; 6];
        rand::thread_rng().fill_bytes(&mut raw);
        text = text.replace("ВАШ_ПУТЬ", &hex::encode(raw));
    }

    if text.contains("UUID_СЕРВИСНОГО_КЛИЕНТА") {
        text = text.replace("UUID_СЕРВИСНОГО_КЛИЕНТА", &uuid::Uuid::new_v4().to_string());
    }

    serde_json::from_str(&text).unwrap_or_else(|_| config.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn заготовка_получает_свои_секреты() {
        // Заготовку с Reality нельзя было создать вообще: ключи просят
        // в генераторе на странице профиля, а страница появляется
        // только после создания.
        let шаблон = serde_json::json!({
            "inbounds": [{
                "streamSettings": { "realitySettings": {
                    "privateKey": "ВАШ_ПРИВАТНЫЙ_КЛЮЧ",
                    "shortIds": ["ВАШ_SHORT_ID"],
                }},
            }],
        });
        let г = fill_secrets(&шаблон);
        let t = г.to_string();
        assert!(!t.contains("ВАШ_"), "остались места: {t}");

        let priv_ = г["inbounds"][0]["streamSettings"]["realitySettings"]["privateKey"]
            .as_str().unwrap();
        assert_eq!(priv_.len(), 43);
        // Ключ должен быть настоящим: из него выводится публичная
        // половина, которую увидит клиент.
        assert!(reality_public_from_private(priv_).is_some());

        let sid = г["inbounds"][0]["streamSettings"]["realitySettings"]["shortIds"][0]
            .as_str().unwrap();
        assert_eq!(sid.len(), 8);
    }

    #[test]
    fn ключ_shadowsocks_нужной_длины() {
        // SS-2022 ждёт ровно 32 байта в base64; с иным движок не встанет.
        let г = fill_secrets(&serde_json::json!({ "password": "СЕРВЕРНЫЙ_КЛЮЧ" }));
        let ключ = г["password"].as_str().unwrap();
        let raw = base64::engine::general_purpose::STANDARD.decode(ключ).unwrap();
        assert_eq!(raw.len(), 32);
    }

    #[test]
    fn каждой_заготовке_свои_значения() {
        let ш = serde_json::json!({ "k": "ВАШ_ПРИВАТНЫЙ_КЛЮЧ", "p": "/ВАШ_ПУТЬ" });
        assert_ne!(fill_secrets(&ш), fill_secrets(&ш));
    }

    #[test]
    fn домены_и_сертификаты_не_выдумываем() {
        // Их не угадать, и подстановка наугад дала бы конфиг, который
        // молча не поднимется.
        let ш = serde_json::json!({ "certificateFile": "/ПУТЬ/К/fullchain.pem",
                                    "address": "ВТОРАЯ_НОДА" });
        assert_eq!(fill_secrets(&ш), ш);
    }

    #[test]
    fn ключи_имеют_нужный_формат() {
        let k = generate_reality_keys();
        // Xray ждёт base64-url без '=' — иначе конфиг не запустится
        assert_eq!(k.private_key.len(), 43, "приватный: {}", k.private_key);
        assert_eq!(k.public_key.len(), 43, "публичный: {}", k.public_key);
        assert!(!k.private_key.contains('='));
        assert!(!k.private_key.contains('+'));
        assert!(!k.private_key.contains('/'));
        assert_eq!(k.short_id.len(), 8);
        assert!(k.short_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ключи_не_повторяются() {
        let a = generate_reality_keys();
        let b = generate_reality_keys();
        assert_ne!(a.private_key, b.private_key);
        assert_ne!(a.public_key, b.public_key);
        assert_ne!(a.short_id, b.short_id);
    }

    #[test]
    fn публичный_ключ_выводится_из_чужого_приватного() {
        // То же, но функцией, которой пользуется разбор профиля: ключи
        // там пришли из конфига, а не от нашего генератора.
        let k = generate_reality_keys();
        assert_eq!(reality_public_from_private(&k.private_key).as_deref(), Some(k.public_key.as_str()));
        // Мусор не должен превращаться в правдоподобный ключ.
        assert_eq!(reality_public_from_private("не-ключ"), None);
        assert_eq!(reality_public_from_private(""), None);
    }

    #[test]
    fn публичный_ключ_выводится_из_приватного() {
        // Если это сломается, клиент не сможет проверить сервер
        // и соединение будет молча падать.
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
        let k = generate_reality_keys();
        let priv_bytes: [u8; 32] = B64.decode(&k.private_key).unwrap().try_into().unwrap();
        let secret = x25519_dalek::StaticSecret::from(priv_bytes);
        let derived = B64.encode(x25519_dalek::PublicKey::from(&secret).as_bytes());
        assert_eq!(derived, k.public_key);
    }
}

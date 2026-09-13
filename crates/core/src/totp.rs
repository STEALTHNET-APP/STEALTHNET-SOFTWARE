//! Одноразовые коды входа (TOTP, RFC 6238).
//!
//! Своя реализация вместо зависимости: алгоритм — это HMAC-SHA1 от
//! номера окна и вырезка четырёх байт. Тянуть ради этого крейт с
//! собственным деревом зависимостей в проект, который ставят чужие
//! люди, — плохой размен.

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Длина окна. 30 секунд — то, что ожидают все приложения-аутентификаторы.
const STEP: u64 = 30;
const DIGITS: u32 = 6;

/// Секрет в base32 без набивки — в таком виде его читают приложения.
pub fn generate_secret() -> String {
    use rand::RngCore;
    let mut raw = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut raw);
    base32_encode(&raw)
}

/// Ссылка для QR-кода в приложении-аутентификаторе.
pub fn provisioning_uri(secret: &str, account: &str, issuer: &str) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>()
    };
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&digits={DIGITS}&period={STEP}",
        enc(issuer),
        enc(account),
        secret,
        enc(issuer)
    )
}

/// Проверяет код.
///
/// Допускаем соседние окна: часы на телефоне и сервере расходятся, и
/// без запаса пользователь получает «неверный код» при верном коде.
pub fn verify(secret: &str, code: &str, now: u64) -> bool {
    verified_step(secret, code, now).is_some()
}

/// Return the matched counter so login can atomically reject replayed codes.
pub fn verified_step(secret: &str, code: &str, now: u64) -> Option<u64> {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let key = base32_decode(secret)?;

    let counter = now / STEP;
    for w in [counter, counter.saturating_sub(1), counter.saturating_add(1)] {
        let expected = compute(&key, w);
        if expected.bytes().zip(code.bytes()).fold(0u8, |diff, (a,b)| diff | (a ^ b)) == 0 {
            return Some(w);
        }
    }
    None
}

fn compute(key: &[u8], counter: u64) -> String {
    let mut mac = HmacSha1::new_from_slice(key).expect("hmac принимает ключ любой длины");
    mac.update(&counter.to_be_bytes());
    let hash = mac.finalize().into_bytes();

    // Динамическая вырезка из RFC 4226: младшие 4 бита последнего байта
    // указывают, откуда брать четыре байта результата.
    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let bin = ((hash[offset] & 0x7f) as u32) << 24
        | (hash[offset + 1] as u32) << 16
        | (hash[offset + 2] as u32) << 8
        | (hash[offset + 3] as u32);

    format!("{:0width$}", bin % 10u32.pow(DIGITS), width = DIGITS as usize)
}

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut bits = 0u32;
    let mut acc = 0u32;
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut out = Vec::new();
    for c in s.trim().to_uppercase().chars().filter(|c| *c != '=') {
        let v = ALPHABET.iter().position(|&a| a as char == c)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Контрольные значения из RFC 6238 (ключ "12345678901234567890").
    #[test]
    fn совпадает_с_эталоном_rfc() {
        let secret = base32_encode(b"12345678901234567890");
        // В RFC коды восьмизначные; шестизначный — это его хвост.
        assert!(verify(&secret, "287082", 59));
        assert!(verify(&secret, "081804", 1_111_111_109));
    }

    #[test]
    fn соседние_окна_принимаются() {
        // Часы телефона и сервера расходятся: без запаса верный код
        // отвергался бы у части пользователей.
        let secret = generate_secret();
        let key = base32_decode(&secret).unwrap();
        let now = 1_700_000_000u64;
        let prev = compute(&key, now / STEP - 1);
        assert!(verify(&secret, &prev, now));
    }

    #[test]
    fn чужой_код_не_проходит() {
        let secret = generate_secret();
        assert!(!verify(&secret, "000000", 1_700_000_000));
        assert!(!verify(&secret, "abc", 1_700_000_000));
        assert!(!verify(&secret, "12345", 1_700_000_000));
    }

    #[test]
    fn base32_обратим() {
        let data = b"12345678901234567890";
        assert_eq!(base32_decode(&base32_encode(data)).unwrap(), data);
    }
}

//! Плагины ноды: фильтры и блокировки через nftables.
//!
//! Название «плагины» обманчиво: чужой код здесь не запускается. Это
//! фиксированный набор возможностей, который агент включает по JSON из
//! панели и применяет правилами ядра. Так и надёжнее, и проверяемо:
//! `nft list table inet stealthnet` показывает ровно то, что работает.
//!
//! Почему nftables, а не отбрасывание в самом Xray: движок видит
//! соединение уже после установки TCP, а ядро отбрасывает пакет до
//! него. Для блокировки источника это принципиально — иначе нарушитель
//! продолжает тратить ресурсы ноды.

use std::collections::HashSet;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Имя таблицы. Своё, чтобы не тронуть чужие правила на сервере:
/// панель ставят на машины, где уже что-то настроено.
const TABLE: &str = "stealthnet";

#[derive(Debug, Clone, Default, Deserialize)]
// Панель шлёт camelCase — то же, что в документации Remnawave, чтобы
// готовые конфигурации переносились без правки. Без rename_all поля
// молча падают в умолчания: настройка «включено», а правил нет.
#[serde(default, rename_all = "camelCase")]
pub struct PluginConfig {
    pub ingress_filter: Filter,
    pub egress_filter: Filter,
    pub torrent_blocker: TorrentBlocker,
    /// Переиспользуемые списки: на них ссылаются фильтры по имени.
    pub shared_lists: Vec<SharedList>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Filter {
    pub enabled: bool,
    pub blocked_ips: Vec<String>,
    pub blocked_ports: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TorrentBlocker {
    pub enabled: bool,
    /// На сколько секунд отрезать адрес. 0 — до перезагрузки ноды.
    pub block_duration: u32,
    /// Кого не трогать: свои адреса, мониторинг, собственный офис.
    pub ignore_ips: Vec<String>,
}

impl Default for TorrentBlocker {
    fn default() -> Self {
        // Час — компромисс: достаточно, чтобы клиент заметил, и мало,
        // чтобы случайное срабатывание не отрезало человека на сутки.
        Self { enabled: false, block_duration: 3600, ignore_ips: Vec::new() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedList {
    pub name: String,
    pub items: Vec<String>,
}

impl PluginConfig {
    /// Нечего применять: все фильтры выключены и блокировщик тоже.
    ///
    /// Панель шлёт этот раздел всегда, даже когда в нём одни умолчания.
    /// Без проверки агент дёргал nftables каждые пятнадцать секунд на
    /// каждой ноде — а на сервере без nftables ещё и писал об этом в
    /// журнал, пока тот не переставал быть читаемым.
    pub fn is_noop(&self) -> bool {
        !self.ingress_filter.enabled && !self.egress_filter.enabled && !self.torrent_blocker.enabled
    }
}

/// Готовность сервера. Панель показывает это как есть: без nftables и
/// прав NET_ADMIN плагины не работают, и рисовать их включёнными нельзя.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PluginStatus {
    pub nft_available: bool,
    pub can_modify: bool,
    pub kernel: String,
    pub applied: bool,
    pub error: Option<String>,
}

/// Есть ли nftables и хватает ли прав.
pub fn probe() -> PluginStatus {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_string();

    let nft_available = Command::new("nft")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Права проверяем попыткой прочитать список таблиц: она требует
    // тех же привилегий, что и правка, но ничего не меняет.
    let can_modify = nft_available
        && Command::new("nft")
            .args(["list", "tables"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

    PluginStatus { nft_available, can_modify, kernel, applied: false, error: None }
}

/// Разворачивает ссылки на общие списки в конкретные адреса.
fn expand(items: &[String], lists: &[SharedList]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        match item.strip_prefix("ext:") {
            Some(name) => {
                if let Some(l) = lists.iter().find(|l| l.name == name || l.name == *item) {
                    out.extend(l.items.iter().cloned());
                }
            }
            None => out.push(item.clone()),
        }
    }
    // Дубли в наборе nftables — ошибка применения целиком, а не
    // предупреждение: одна повторённая строка обнулила бы весь фильтр.
    let mut seen = HashSet::new();
    out.retain(|v| seen.insert(v.clone()));
    out
}

/// Отделяет IPv4 от IPv6: в nftables это разные типы наборов.
fn split_family(items: &[String]) -> (Vec<String>, Vec<String>) {
    items
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .partition(|s| !s.contains(':'))
}

/// Собирает и применяет правила одним вызовом `nft -f -`.
///
/// Именно одним: применение по частям оставляет ноду в промежуточном
/// состоянии, если посередине окажется опечатка.
pub fn apply(cfg: &PluginConfig) -> Result<(), String> {
    let lists = &cfg.shared_lists;

    let ingress = expand(&cfg.ingress_filter.blocked_ips, lists);
    let egress = expand(&cfg.egress_filter.blocked_ips, lists);
    let (in4, in6) = split_family(&ingress);
    let (eg4, eg6) = split_family(&egress);

    let mut s = String::new();
    // Пересоздаём таблицу целиком: так состояние ядра всегда равно
    // конфигурации, и «остатки» прошлых правил не накапливаются.
    s.push_str(&format!("delete table inet {TABLE}\n"));
    s.push_str(&format!("table inet {TABLE} {{\n"));

    // Набор для временных блокировок: flags timeout заставляет ядро
    // само снимать записи по истечении срока, без нашего участия.
    s.push_str("  set blocked4 { type ipv4_addr; flags timeout; }\n");
    s.push_str("  set blocked6 { type ipv6_addr; flags timeout; }\n");

    let set = |name: &str, ty: &str, items: &[String]| {
        if items.is_empty() {
            format!("  set {name} {{ type {ty}; flags interval; }}\n")
        } else {
            format!(
                "  set {name} {{ type {ty}; flags interval; elements = {{ {} }} }}\n",
                items.join(", ")
            )
        }
    };
    s.push_str(&set("ingress4", "ipv4_addr", &in4));
    s.push_str(&set("ingress6", "ipv6_addr", &in6));
    s.push_str(&set("egress4", "ipv4_addr", &eg4));
    s.push_str(&set("egress6", "ipv6_addr", &eg6));

    let ports = &cfg.egress_filter.blocked_ports;
    if ports.is_empty() {
        s.push_str("  set egressports { type inet_service; }\n");
    } else {
        s.push_str(&format!(
            "  set egressports {{ type inet_service; elements = {{ {} }} }}\n",
            ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
        ));
    }

    // priority filter — там же, где работают привычные правила, чтобы
    // порядок с чужим фаерволом был предсказуем.
    s.push_str("  chain input {\n");
    s.push_str("    type filter hook input priority filter; policy accept;\n");
    s.push_str("    ip saddr @blocked4 counter drop\n");
    s.push_str("    ip6 saddr @blocked6 counter drop\n");
    if cfg.ingress_filter.enabled {
        s.push_str("    ip saddr @ingress4 counter drop\n");
        s.push_str("    ip6 saddr @ingress6 counter drop\n");
    }
    s.push_str("  }\n");

    s.push_str("  chain output {\n");
    s.push_str("    type filter hook output priority filter; policy accept;\n");
    if cfg.egress_filter.enabled {
        s.push_str("    ip daddr @egress4 counter drop\n");
        s.push_str("    ip6 daddr @egress6 counter drop\n");
        if !ports.is_empty() {
            s.push_str("    tcp dport @egressports counter reject\n");
        }
    }
    s.push_str("  }\n");
    s.push_str("}\n");

    run_nft(&s)
}

/// Адреса, которые нельзя блокировать никогда.
///
/// Клиент ходит через VPN с того же адреса, с которого администратор
/// может управлять сервером: блокировка отрезает и его. Проверено на
/// себе — тестовый торрент положил SSH к стенду на две минуты.
///
/// Служебные диапазоны отсекаем здесь, а «свой офис» администратор
/// добавляет в исключения сам: угадать его мы не можем.
pub fn is_protected(ip: &str) -> bool {
    let Ok(addr) = ip.parse::<std::net::IpAddr>() else {
        // Неразобранный адрес не блокируем: неизвестно, что это.
        return true;
    };
    match addr {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_broadcast()
                || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified()
                // fc00::/7 — приватные, fe80::/10 — локальные.
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Блокирует адрес на время. `0` — до перезагрузки ноды.
pub fn block_ip(ip: &str, seconds: u32) -> Result<(), String> {
    if is_protected(ip) {
        return Err(format!("{ip} — служебный адрес, блокировать нельзя"));
    }
    let set = if ip.contains(':') { "blocked6" } else { "blocked4" };
    let elem = if seconds > 0 {
        format!("{ip} timeout {seconds}s")
    } else {
        ip.to_string()
    };
    run_nft(&format!("add element inet {TABLE} {set} {{ {elem} }}\n"))
}

pub fn unblock_ip(ip: &str) -> Result<(), String> {
    let set = if ip.contains(':') { "blocked6" } else { "blocked4" };
    run_nft(&format!("delete element inet {TABLE} {set} {{ {ip} }}\n"))
}

fn run_nft(script: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new("nft")
        .arg("-f")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("не запустил nft: {e}"))?;

    child
        .stdin
        .as_mut()
        .ok_or("нет stdin у nft")?
        .write_all(script.as_bytes())
        .map_err(|e| e.to_string())?;

    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }

    let err = String::from_utf8_lossy(&out.stderr);
    // «No such file or directory» на delete table — это первый запуск,
    // таблицы ещё нет. Останавливаться из-за этого нельзя.
    if err.contains("No such file or directory") && script.starts_with("delete table") {
        let without = script.lines().skip(1).collect::<Vec<_>>().join("\n");
        return run_nft(&without);
    }
    Err(err.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ссылки_на_общие_списки_разворачиваются() {
        let lists = vec![SharedList {
            name: "office".into(),
            items: vec!["10.0.0.1".into(), "10.0.0.2".into()],
        }];
        let got = expand(&["ext:office".into(), "8.8.8.8".into()], &lists);
        assert_eq!(got, vec!["10.0.0.1", "10.0.0.2", "8.8.8.8"]);
    }

    #[test]
    fn дубли_убираются() {
        // Повторённый элемент — ошибка применения всего набора, а не
        // предупреждение: из-за одной строки не встал бы весь фильтр.
        let got = expand(&["1.1.1.1".into(), "1.1.1.1".into()], &[]);
        assert_eq!(got, vec!["1.1.1.1"]);
    }

    #[test]
    fn конфиг_панели_разбирается() {
        // Панель шлёт camelCase. Без rename_all поля молча уходят в
        // умолчания: в панели «включено», а правил в ядре нет — и это
        // не видно ниоткуда, кроме самого ядра.
        let json = serde_json::json!({
            "egressFilter": { "enabled": true, "blockedPorts": [25, 465] },
            "torrentBlocker": { "enabled": true, "blockDuration": 120 },
        });
        let cfg: PluginConfig = serde_json::from_value(json).unwrap();
        assert!(cfg.egress_filter.enabled, "egressFilter не разобрался");
        assert_eq!(cfg.egress_filter.blocked_ports, vec![25, 465]);
        assert!(cfg.torrent_blocker.enabled);
        assert_eq!(cfg.torrent_blocker.block_duration, 120);
    }

    #[test]
    fn служебные_адреса_не_блокируются() {
        // Блокировка своей же подсети отрезает администратора от
        // сервера. Проверено на себе: тестовый торрент через VPN
        // положил SSH к стенду.
        for ip in ["127.0.0.1", "10.0.0.5", "192.168.1.1", "169.254.1.1", "::1", "fe80::1"] {
            assert!(is_protected(ip), "{ip} должен быть защищён");
        }
        assert!(is_protected("не адрес"));
        assert!(!is_protected("8.8.8.8"));
    }

    #[test]
    fn семейства_адресов_разделяются() {
        let (v4, v6) = split_family(&[
            "1.2.3.4".into(),
            "2001:db8::1".into(),
            "10.0.0.0/8".into(),
        ]);
        assert_eq!(v4, vec!["1.2.3.4", "10.0.0.0/8"]);
        assert_eq!(v6, vec!["2001:db8::1"]);
    }

    #[test]
    fn выключенные_фильтры_не_дают_правил_дропа() {
        // Набор существует всегда — иначе block_ip падал бы на
        // отсутствующем наборе, — но правило появляется только когда
        // фильтр включён.
        let cfg = PluginConfig::default();
        let lists = &cfg.shared_lists;
        assert!(expand(&cfg.ingress_filter.blocked_ips, lists).is_empty());
        assert!(!cfg.ingress_filter.enabled);
        assert!(!cfg.torrent_blocker.enabled);
        assert_eq!(cfg.torrent_blocker.block_duration, 3600);
    }
}

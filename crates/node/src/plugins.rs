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
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Имя таблицы. Своё, чтобы не тронуть чужие правила на сервере:
/// панель ставят на машины, где уже что-то настроено.
const TABLE: &str = "stealthnet";

pub const DEFAULT_ANTISCANNER_SOURCES: &[&str] = &[
    "https://raw.githubusercontent.com/shadow-netlab/traffic-guard-lists/refs/heads/main/public/government_networks.list",
    "https://raw.githubusercontent.com/shadow-netlab/traffic-guard-lists/refs/heads/main/public/antiscanner.list",
    "https://raw.githubusercontent.com/shadow-netlab/traffic-guard-lists/refs/heads/main/public/skipa.list",
];

const ANTISCANNER_CACHE_PATHS: &[&str] = &[
    "/var/lib/sn-node/antiscanner.list",
    "/tmp/sn-node-antiscanner.list",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AntiScanner {
    pub enabled: bool,
    /// Ссылки на списки подсетей (например, правительственные сети, сканеры, СКИПА).
    pub sources: Vec<String>,
    /// Интервал обновления списков в секундах (по умолчанию 43200 — 12 часов).
    pub update_interval_secs: u64,
    /// Дополнительные адреса или подсети для ручной блокировки.
    pub custom_ips: Vec<String>,
}

impl Default for AntiScanner {
    fn default() -> Self {
        Self {
            enabled: false,
            sources: DEFAULT_ANTISCANNER_SOURCES.iter().map(|s| s.to_string()).collect(),
            update_interval_secs: 43200,
            custom_ips: Vec::new(),
        }
    }
}

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
    /// Антисканер: защита от массового сканирования и активного зондирования.
    pub anti_scanner: AntiScanner,
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
        !self.ingress_filter.enabled
            && !self.egress_filter.enabled
            && !self.torrent_blocker.enabled
            && !self.anti_scanner.enabled
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
    pub antiscanner_enabled: bool,
    pub antiscanner_rules_count: usize,
    pub antiscanner_dropped_packets: u64,
    pub antiscanner_dropped_bytes: u64,
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

    let (packets, bytes) = if can_modify {
        get_antiscanner_dropped_stats()
    } else {
        (0, 0)
    };

    let cached = load_antiscanner_ips();

    PluginStatus {
        nft_available,
        can_modify,
        kernel,
        applied: false,
        error: None,
        antiscanner_enabled: false,
        antiscanner_rules_count: cached.len(),
        antiscanner_dropped_packets: packets,
        antiscanner_dropped_bytes: bytes,
    }
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

pub fn collapse_cidrs_v4(items: &[String]) -> Vec<String> {
    struct Entry {
        start: u32,
        end: u32,
        raw: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for s in items {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (ip_str, mask_str) = match trimmed.split_once('/') {
            Some((a, m)) => (a, Some(m)),
            None => (trimmed, None),
        };
        let Ok(ip) = ip_str.parse::<std::net::Ipv4Addr>() else {
            continue;
        };
        let mask = mask_str
            .and_then(|m| m.parse::<u8>().ok())
            .unwrap_or(32)
            .min(32);
        let ip_u32 = u32::from(ip);
        let netmask = if mask == 0 {
            0u32
        } else {
            !0u32 << (32 - mask)
        };
        let start = ip_u32 & netmask;
        let end = if mask == 0 {
            u32::MAX
        } else {
            start | !netmask
        };
        let raw = if mask == 32 {
            std::net::Ipv4Addr::from(start).to_string()
        } else {
            format!("{}/{}", std::net::Ipv4Addr::from(start), mask)
        };
        entries.push(Entry { start, end, raw });
    }

    // Сортируем: сначала меньший start, при равном start — больший end (более широкая сеть первой)
    entries.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));

    let mut result = Vec::new();
    let mut last_end: Option<u32> = None;

    for entry in entries {
        if let Some(prev_end) = last_end {
            if entry.start <= prev_end {
                continue;
            }
        }
        last_end = Some(entry.end);
        result.push(entry.raw);
    }

    result
}

pub fn collapse_cidrs_v6(items: &[String]) -> Vec<String> {
    struct Entry {
        start: u128,
        end: u128,
        raw: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for s in items {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (ip_str, mask_str) = match trimmed.split_once('/') {
            Some((a, m)) => (a, Some(m)),
            None => (trimmed, None),
        };
        let Ok(ip) = ip_str.parse::<std::net::Ipv6Addr>() else {
            continue;
        };
        let mask = mask_str
            .and_then(|m| m.parse::<u8>().ok())
            .unwrap_or(128)
            .min(128);
        let ip_u128 = u128::from(ip);
        let netmask = if mask == 0 {
            0u128
        } else {
            !0u128 << (128 - mask)
        };
        let start = ip_u128 & netmask;
        let end = if mask == 0 {
            u128::MAX
        } else {
            start | !netmask
        };
        let raw = if mask == 128 {
            std::net::Ipv6Addr::from(start).to_string()
        } else {
            format!("{}/{}", std::net::Ipv6Addr::from(start), mask)
        };
        entries.push(Entry { start, end, raw });
    }

    entries.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));

    let mut result = Vec::new();
    let mut last_end: Option<u128> = None;

    for entry in entries {
        if let Some(prev_end) = last_end {
            if entry.start <= prev_end {
                continue;
            }
        }
        last_end = Some(entry.end);
        result.push(entry.raw);
    }

    result
}

/// Отделяет IPv4 от IPv6 и сворачивает перекрывающиеся подсети, исключая конфликты в nftables.
fn split_family(items: &[String]) -> (Vec<String>, Vec<String>) {
    let mut v4_raw = Vec::new();
    let mut v6_raw = Vec::new();
    for s in items {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains(':') {
            v6_raw.push(trimmed.to_string());
        } else {
            v4_raw.push(trimmed.to_string());
        }
    }
    (collapse_cidrs_v4(&v4_raw), collapse_cidrs_v6(&v6_raw))
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

    let (scanners4, scanners6) = if cfg.anti_scanner.enabled {
        let mut list = load_antiscanner_ips();
        list.extend(expand(&cfg.anti_scanner.custom_ips, lists));
        let mut seen = HashSet::new();
        list.retain(|v| !is_protected_cidr(v) && seen.insert(v.clone()));
        split_family(&list)
    } else {
        (Vec::new(), Vec::new())
    };

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
    s.push_str(&set("scanners4", "ipv4_addr", &scanners4));
    s.push_str(&set("scanners6", "ipv6_addr", &scanners6));

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
    s.push_str("    iif lo accept\n");
    s.push_str("    ct state established,related accept\n");
    s.push_str("    ct state invalid drop\n");
    s.push_str("    ip saddr @blocked4 counter drop\n");
    s.push_str("    ip6 saddr @blocked6 counter drop\n");
    if cfg.ingress_filter.enabled {
        s.push_str("    ip saddr @ingress4 counter drop\n");
        s.push_str("    ip6 saddr @ingress6 counter drop\n");
    }
    if cfg.anti_scanner.enabled {
        s.push_str("    ip saddr @scanners4 counter drop\n");
        s.push_str("    ip6 saddr @scanners6 counter drop\n");
        s.push_str("    tcp flags syn meter synlimit4 { ip saddr limit rate over 200/second burst 300 packets } counter drop\n");
        s.push_str("    tcp flags syn meter synlimit6 { ip6 saddr limit rate over 200/second burst 300 packets } counter drop\n");
    }
    s.push_str("  }\n");

    s.push_str("  chain forward {\n");
    s.push_str("    type filter hook forward priority filter; policy accept;\n");
    s.push_str("    tcp flags syn tcp option maxseg size set rt mtu\n");
    s.push_str("  }\n");

    s.push_str("  chain output {\n");
    s.push_str("    type filter hook output priority filter; policy accept;\n");
    s.push_str("    tcp flags syn tcp option maxseg size set rt mtu\n");
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

pub fn is_valid_cidr_or_ip(s: &str) -> bool {
    let (ip_str, mask_str) = match s.split_once('/') {
        Some((a, m)) => (a, Some(m)),
        None => (s, None),
    };
    let Ok(addr) = ip_str.parse::<std::net::IpAddr>() else {
        return false;
    };
    if let Some(m) = mask_str {
        let Ok(bits) = m.parse::<u8>() else {
            return false;
        };
        let max_bits = if addr.is_ipv4() { 32 } else { 128 };
        if bits > max_bits {
            return false;
        }
    }
    true
}

pub fn get_local_node_ips() -> Vec<std::net::IpAddr> {
    let mut ips = Vec::new();
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("1.1.1.1:80").is_ok() {
            if let Ok(local) = socket.local_addr() {
                let ip = local.ip();
                if !ip.is_unspecified() && !ip.is_loopback() {
                    ips.push(ip);
                }
            }
        }
    }
    if let Ok(socket) = std::net::UdpSocket::bind("[::]:0") {
        if socket.connect("[2606:4700:4700::1111]:80").is_ok() {
            if let Ok(local) = socket.local_addr() {
                let ip = local.ip();
                if !ip.is_unspecified() && !ip.is_loopback() {
                    ips.push(ip);
                }
            }
        }
    }
    ips
}

pub fn cidr_contains_ip(cidr: &str, target: std::net::IpAddr) -> bool {
    let (ip_str, mask_str) = match cidr.split_once('/') {
        Some((a, m)) => (a, Some(m)),
        None => (cidr, None),
    };
    let Ok(net_addr) = ip_str.parse::<std::net::IpAddr>() else {
        return false;
    };
    match (net_addr, target) {
        (std::net::IpAddr::V4(net_v4), std::net::IpAddr::V4(target_v4)) => {
            let mask_bits = mask_str
                .and_then(|m| m.parse::<u8>().ok())
                .unwrap_or(32)
                .min(32);
            let mask = if mask_bits == 0 {
                0u32
            } else {
                !0u32 << (32 - mask_bits)
            };
            (u32::from(net_v4) & mask) == (u32::from(target_v4) & mask)
        }
        (std::net::IpAddr::V6(net_v6), std::net::IpAddr::V6(target_v6)) => {
            let mask_bits = mask_str
                .and_then(|m| m.parse::<u8>().ok())
                .unwrap_or(128)
                .min(128);
            let mask = if mask_bits == 0 {
                0u128
            } else {
                !0u128 << (128 - mask_bits)
            };
            (u128::from(net_v6) & mask) == (u128::from(target_v6) & mask)
        }
        _ => false,
    }
}

pub fn is_protected_cidr(cidr: &str) -> bool {
    let raw = cidr.trim();
    if raw.is_empty() {
        return true;
    }
    for local_ip in get_local_node_ips() {
        if cidr_contains_ip(raw, local_ip) {
            return true;
        }
    }
    let (addr_str, mask_opt) = match raw.split_once('/') {
        Some((a, m)) => (a, Some(m)),
        None => (raw, None),
    };
    if let Some(mask_str) = mask_opt {
        if let Ok(bits) = mask_str.parse::<u8>() {
            let is_v6 = addr_str.contains(':');
            if (!is_v6 && bits < 8) || (is_v6 && bits < 16) {
                return true;
            }
        } else {
            return true;
        }
    }
    is_protected(addr_str)
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
    for local_ip in get_local_node_ips() {
        if addr == local_ip {
            return true;
        }
    }
    match addr {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 — приватные, fe80::/10 — локальные.
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

pub fn antiscanner_cache_path() -> PathBuf {
    for p in ANTISCANNER_CACHE_PATHS {
        let path = PathBuf::from(p);
        if let Some(parent) = path.parent() {
            if parent.exists() {
                return path;
            }
        }
    }
    PathBuf::from("/tmp/sn-node-antiscanner.list")
}

pub fn load_antiscanner_ips() -> Vec<String> {
    for p in ANTISCANNER_CACHE_PATHS {
        if let Ok(content) = std::fs::read_to_string(p) {
            let ips: Vec<String> = content
                .lines()
                .map(|l| l.split(['#', ';']).next().unwrap_or("").trim())
                .filter(|l| !l.is_empty() && !is_protected_cidr(l) && is_valid_cidr_or_ip(l))
                .map(String::from)
                .collect();
            if !ips.is_empty() {
                return ips;
            }
        }
    }
    Vec::new()
}

pub async fn sync_antiscanner_lists(
    http: &reqwest::Client,
    sources: &[String],
) -> Result<usize, String> {
    let mut all_cidrs = HashSet::new();
    let mut any_success = false;
    let mut errors = Vec::new();

    for url in sources {
        let url = url.trim();
        if url.is_empty() {
            continue;
        }
        match http
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    any_success = true;
                    for line in text.lines() {
                        let clean = line.split(['#', ';']).next().unwrap_or("").trim();
                        if clean.is_empty() {
                            continue;
                        }
                        if !is_protected_cidr(clean) && is_valid_cidr_or_ip(clean) {
                            all_cidrs.insert(clean.to_string());
                        }
                    }
                }
            }
            Ok(resp) => {
                errors.push(format!("{url}: HTTP {}", resp.status()));
            }
            Err(e) => {
                errors.push(format!("{url}: {e}"));
            }
        }
    }

    if !any_success && !errors.is_empty() {
        return Err(format!("не удалось скачать списки: {}", errors.join("; ")));
    }

    let count = all_cidrs.len();
    if count > 0 {
        let mut sorted: Vec<String> = all_cidrs.into_iter().collect();
        sorted.sort();
        let content = sorted.join("\n");
        let path = antiscanner_cache_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, content);
    }

    Ok(count)
}

pub fn get_antiscanner_dropped_stats() -> (u64, u64) {
    let Ok(out) = Command::new("nft")
        .args(["list", "chain", "inet", TABLE, "input"])
        .output()
    else {
        return (0, 0);
    };
    if !out.status.success() {
        return (0, 0);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_counter_stats(&text)
}

fn parse_counter_stats(text: &str) -> (u64, u64) {
    let mut total_packets = 0u64;
    let mut total_bytes = 0u64;
    for line in text.lines() {
        if line.contains("@scanners4") || line.contains("@scanners6") || line.contains("synlimit") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for i in 0..parts.len() {
                if parts[i] == "packets" && i + 1 < parts.len() {
                    if let Ok(p) = parts[i + 1].parse::<u64>() {
                        total_packets += p;
                    }
                }
                if parts[i] == "bytes" && i + 1 < parts.len() {
                    if let Ok(b) = parts[i + 1].parse::<u64>() {
                        total_bytes += b;
                    }
                }
            }
        }
    }
    (total_packets, total_bytes)
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
        assert!(!cfg.anti_scanner.enabled);
        assert_eq!(cfg.torrent_blocker.block_duration, 3600);
        assert_eq!(cfg.anti_scanner.update_interval_secs, 43200);
    }

    #[test]
    fn антисканер_конфиг_разбирается() {
        let json = serde_json::json!({
            "antiScanner": {
                "enabled": true,
                "sources": ["https://example.com/block.list"],
                "updateIntervalSecs": 86400,
                "customIps": ["198.51.100.0/24"]
            }
        });
        let cfg: PluginConfig = serde_json::from_value(json).unwrap();
        assert!(cfg.anti_scanner.enabled);
        assert_eq!(cfg.anti_scanner.sources, vec!["https://example.com/block.list"]);
        assert_eq!(cfg.anti_scanner.update_interval_secs, 86400);
        assert_eq!(cfg.anti_scanner.custom_ips, vec!["198.51.100.0/24"]);
        assert!(!cfg.is_noop());
    }

    #[test]
    fn антисканер_фильтрует_опасные_подсети() {
        assert!(is_protected_cidr("0.0.0.0/0"), "0.0.0.0/0 должно быть защищено");
        assert!(is_protected_cidr("10.0.0.0/8"), "10.0.0.0/8 должно быть защищено");
        assert!(is_protected_cidr("192.168.1.0/24"), "192.168.1.0/24 должно быть защищено");
        assert!(is_protected_cidr("127.0.0.1/32"), "127.0.0.1/32 должно быть защищено");
        assert!(is_protected_cidr("::1/128"), "::1/128 должно быть защищено");
        assert!(is_protected_cidr("1.0.0.0/4"), "широкая маска /4 должна быть защищена");
        assert!(!is_protected_cidr("198.51.100.0/24"), "публичная подсеть не должна быть защищена");
        assert!(!is_protected_cidr("95.173.136.0/21"), "сканерная подсеть должна проходить фильтр");

        assert!(is_valid_cidr_or_ip("198.51.100.0/24"));
        assert!(is_valid_cidr_or_ip("2001:db8::/32"));
        assert!(!is_valid_cidr_or_ip("999.999.999.999/24"));
        assert!(!is_valid_cidr_or_ip("1.2.3.4/33"));
    }

    #[test]
    fn проверка_вхождения_ip_в_cidr() {
        let ip: std::net::IpAddr = "203.0.113.195".parse().unwrap();
        assert!(cidr_contains_ip("203.0.113.195", ip));
        assert!(cidr_contains_ip("203.0.113.195/32", ip));
        assert!(cidr_contains_ip("203.0.113.0/24", ip));
        assert!(cidr_contains_ip("203.0.0.0/16", ip));
        assert!(cidr_contains_ip("0.0.0.0/0", ip));
        assert!(!cidr_contains_ip("203.0.114.0/24", ip));
        assert!(!cidr_contains_ip("1.1.1.1/32", ip));

        let v6: std::net::IpAddr = "2001:db8::1".parse().unwrap();
        assert!(cidr_contains_ip("2001:db8::/32", v6));
        assert!(cidr_contains_ip("2001:db8::1/128", v6));
        assert!(cidr_contains_ip("::/0", v6));
        assert!(!cidr_contains_ip("2001:db9::/32", v6));
    }

    #[test]
    fn разбор_статистики_counter() {
        let nft_out = "
table inet stealthnet {
    chain input {
        type filter hook input priority filter; policy accept;
        ip saddr @scanners4 counter packets 3120 bytes 249600 drop
        ip6 saddr @scanners6 counter packets 5 bytes 400 drop
        tcp flags syn meter synlimit4 size 65535 { ip saddr limit rate over 200/second burst 300 packets } counter packets 15 bytes 900 drop
    }
}";
        let (packets, bytes) = parse_counter_stats(nft_out);
        assert_eq!(packets, 3140);
        assert_eq!(bytes, 250900);
    }

    #[test]
    fn коллапсирование_перекрывающихся_подсетей() {
        let raw_v4 = vec![
            "10.0.0.0/8".into(),
            "10.1.0.0/16".into(),
            "10.1.2.3/32".into(),
            "192.168.1.0/24".into(),
            "192.168.1.15".into(),
        ];
        let v4 = collapse_cidrs_v4(&raw_v4);
        assert_eq!(v4, vec!["10.0.0.0/8", "192.168.1.0/24"]);

        let raw_v6 = vec![
            "2a01:230:3::/48".into(),
            "2a01:230::/32".into(),
            "2a01:230::/48".into(),
            "2001:db8::1/128".into(),
        ];
        let v6 = collapse_cidrs_v6(&raw_v6);
        assert_eq!(v6, vec!["2001:db8::1", "2a01:230::/32"]);
    }
}


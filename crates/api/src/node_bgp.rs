//! On-demand network ownership lookup using bgp.tools' documented WHOIS service.
//! Only public IP literals are sent to a fixed destination; no shell or HTML scraping.
use crate::state::{AppState, CurrentAdmin};
use axum::{extract::{Path, State}, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use sn_core::{Error, Result};
use std::{collections::HashMap, net::IpAddr, sync::OnceLock, time::{Duration, Instant}};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const MAX_RESPONSE: usize = 32 * 1024;
const MAX_ADDRESSES: usize = 4;
const CACHE_SECONDS: u64 = 60;
#[derive(Clone, Debug, Serialize, PartialEq)]
struct Route { asn: u32, prefix: String, country_code: String, registry: String, name: String }
#[derive(Clone, Debug, Serialize)]
struct Address { ip: IpAddr, routes: Vec<Route> }
#[derive(Clone, Serialize)]
struct Network { address: String, checked_at: String, addresses: Vec<Address>, truncated: bool }
struct Entry { at: Instant, result: std::result::Result<Network, String> }
struct Cache { entries: HashMap<String, Entry>, window: Instant, requests: usize }
impl Default for Cache {
    fn default() -> Self { Self { entries: HashMap::new(), window: Instant::now(), requests: 0 } }
}
static CACHE: OnceLock<tokio::sync::Mutex<Cache>> = OnceLock::new();

pub async fn check(_admin: CurrentAdmin, State(st): State<AppState>, Path(id): Path<i64>) -> Result<Response> {
    // Address comes from the existing node, never an arbitrary caller-supplied target.
    let address: String = sqlx::query_scalar("SELECT address FROM nodes WHERE id=$1 AND deleted_at IS NULL")
        .bind(id).fetch_optional(&st.pool).await?.ok_or(Error::NotFound)?;
    let address = address.trim().to_ascii_lowercase();
    let mut cache = CACHE.get_or_init(Default::default).try_lock().map_err(|_| Error::TooManyRequests)?;
    let now = Instant::now();
    cache.entries.retain(|_, e| e.at.elapsed() < Duration::from_secs(if e.result.is_ok() { CACHE_SECONDS } else { 15 }));
    if let Some(entry) = cache.entries.get(&address) { return Ok(reply(&entry.result, true)); }
    if cache.window.elapsed() >= Duration::from_secs(60) { cache.window = now; cache.requests = 0; }
    if cache.requests >= 30 { return Err(Error::TooManyRequests); }
    cache.requests += 1;
    let result = tokio::time::timeout(Duration::from_secs(12), lookup(&address)).await
        .unwrap_or_else(|_| Err("bgp.tools не ответил вовремя. Повторите проверку позже.".into()));
    let response = reply(&result, false);
    // Bounded even when a privileged user changes addresses repeatedly.
    if cache.entries.len() >= 256 { cache.entries.clear(); }
    cache.entries.insert(address, Entry { at: Instant::now(), result });
    Ok(response)
}
fn reply(result: &std::result::Result<Network, String>, cached: bool) -> Response {
    match result {
        Ok(network) => {
            let mut value = serde_json::to_value(network).expect("network is serializable");
            value["source"] = "bgp.tools".into(); value["cached"] = cached.into(); value["cache_seconds"] = CACHE_SECONDS.into();
            Json(value).into_response()
        }
        Err(message) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":message}))).into_response(),
    }
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a,b,c,_] = v.octets();
            !(v.is_private() || v.is_loopback() || v.is_link_local() || v.is_documentation()
                || a == 0 || a >= 224 || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && c == 0) || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            // Global unicast only. Exclude documentation and protocol-assignment ranges.
            (s[0] & 0xe000) == 0x2000 && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && !(s[0] == 0x2002 || (s[0] == 0x3fff && s[1] < 0x1000))
        }
    }
}
async fn resolve(address: &str) -> std::result::Result<(Vec<IpAddr>, bool), String> {
    if !crate::node_admin::valid_node_address(address) { return Err("В настройках ноды нужен IP или домен без протокола и порта.".into()); }
    let raw = address.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(address);
    let mut ips = match raw.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => tokio::net::lookup_host((address, 0)).await
            .map_err(|_| "Не удалось определить IP домена ноды. Проверьте DNS и адрес сервера.".to_string())?
            .map(|s| s.ip()).collect(),
    };
    ips.sort(); ips.dedup();
    if ips.is_empty() { return Err("У домена ноды нет IP-адресов.".into()); }
    if ips.iter().any(|ip| !public_ip(*ip)) {
        return Err("Для проверки BGP нужен публичный IP ноды. Локальные и служебные адреса не отправляются в bgp.tools.".into());
    }
    let truncated = ips.len() > MAX_ADDRESSES;
    ips.truncate(MAX_ADDRESSES);
    Ok((ips, truncated))
}
async fn read_response(reader: impl AsyncRead + Unpin) -> std::result::Result<String, String> {
    let mut bytes = Vec::new();
    reader.take(MAX_RESPONSE as u64 + 1).read_to_end(&mut bytes).await.map_err(|_| "Не удалось прочитать ответ bgp.tools.".to_string())?;
    if bytes.len() > MAX_RESPONSE { return Err("Ответ bgp.tools превышает допустимый размер.".into()); }
    String::from_utf8(bytes).map_err(|_| "bgp.tools вернул некорректный ответ.".into())
}
async fn lookup(address: &str) -> std::result::Result<Network, String> {
    let (ips, truncated) = resolve(address).await?;
    let mut socket = tokio::net::TcpStream::connect(("bgp.tools", 43)).await
        .map_err(|_| "Не удалось связаться с bgp.tools. На сервере панели должен быть доступен исходящий TCP 43.".to_string())?;
    let query = format!("begin\nverbose\n{}\nend\n", ips.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"));
    socket.write_all(query.as_bytes()).await.map_err(|_| "Не удалось отправить запрос в bgp.tools.".to_string())?;
    socket.shutdown().await.map_err(|_| "Соединение с bgp.tools прервалось.".to_string())?;
    let text = read_response(socket).await?;
    let addresses = ips.into_iter().map(|ip| parse(&text, ip).map(|routes| Address { ip, routes })).collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Network { address: address.into(), checked_at: chrono::Utc::now().to_rfc3339(), addresses, truncated })
}
fn prefix_contains(prefix: &str, ip: IpAddr) -> bool {
    let Some((net, mask)) = prefix.split_once('/') else { return false; };
    let (Ok(net), Ok(mask)) = (net.parse::<IpAddr>(), mask.parse::<u32>()) else { return false; };
    match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(i)) if mask <= 32 => mask == 0 || u32::from(n) >> (32-mask) == u32::from(i) >> (32-mask),
        (IpAddr::V6(n), IpAddr::V6(i)) if mask <= 128 => mask == 0 || u128::from(n) >> (128-mask) == u128::from(i) >> (128-mask),
        _ => false,
    }
}
fn parse(text: &str, ip: IpAddr) -> std::result::Result<Vec<Route>, String> {
    let mut matched = false;
    let mut routes = Vec::new();
    for line in text.lines() {
        let cells = line.splitn(7, '|').map(str::trim).collect::<Vec<_>>();
        if cells.len() != 7 || cells[1].parse::<IpAddr>().ok() != Some(ip) { continue; }
        matched = true;
        let asn = match cells[0] { "" | "NA" | "0" => continue, v => v.parse::<u32>().map_err(|_| "bgp.tools вернул некорректный ASN.".to_string())? };
        if !prefix_contains(cells[2], ip) || cells[2].len() > 49 || cells[3].len() > 2 || cells[4].len() > 32 || cells[6].len() > 500 {
            return Err("bgp.tools вернул некорректные данные сети.".into());
        }
        let route = Route { asn, prefix: cells[2].into(), country_code: cells[3].into(), registry: cells[4].into(), name: cells[6].into() };
        if !routes.contains(&route) { routes.push(route); }
    }
    if !matched { return Err("bgp.tools не вернул данные для IP ноды. Повторите проверку позже.".into()); }
    Ok(routes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rejects_non_public_addresses() {
        for value in ["127.0.0.1","10.0.0.1","172.16.0.1","192.168.0.1","169.254.169.254","100.64.0.1","192.0.2.1","198.18.0.1","198.51.100.1","203.0.113.1","0.0.0.0","224.0.0.1","240.0.0.1","255.255.255.255","::1","::ffff:1.1.1.1","fc00::1","fe80::1","2001:db8::1","2002::1","3fff::1"] { assert!(!public_ip(value.parse().unwrap()),"{value}"); }
        for value in ["1.1.1.1","8.8.8.8","213.165.57.105","2606:4700:4700::1111","2001:4860:4860::8888"] { assert!(public_ip(value.parse().unwrap()),"{value}"); }
    }
    #[test] fn reads_ipv4_ipv6_and_multiple_origins_without_mixing_targets() {
        let text="AS | IP | BGP Prefix | CC | Registry | Allocated | AS Name\n13335 | 1.1.1.1 | 1.1.1.0/24 | US | ARIN | 0001-01-01 | Cloudflare, Inc.\n13335 | 2606:4700:4700::1111 | 2606:4700::/32 | US | ARIN | | Cloudflare\n64500 | 1.1.1.1 | 1.1.1.0/24 | US | ARIN | | Other origin\n";
        let rows=parse(text,"1.1.1.1".parse().unwrap()).unwrap(); assert_eq!(rows.len(),2); assert_eq!(rows[0].asn,13335);
        assert_eq!(parse(text,"2606:4700:4700::1111".parse().unwrap()).unwrap().len(),1);
        assert!(parse(text,"8.8.8.8".parse().unwrap()).is_err());
    }
    #[test] fn unknown_announcement_is_distinct_from_broken_response() {
        assert!(parse("0 | 1.1.1.1 | | | | |", "1.1.1.1".parse().unwrap()).unwrap().is_empty());
        for text in ["rate limited", "bad | 1.1.1.1 | 1.1.1.0/24 | US | ARIN | | Name", "1 | 1.1.1.1 | 8.8.8.0/24 | US | ARIN | | Name", "1 | 1.1.1.1 | 1.1.1.0/33 | US | ARIN | | Name"] { assert!(parse(text,"1.1.1.1".parse().unwrap()).is_err()); }
    }
    #[tokio::test] async fn bounds_response_and_rejects_invalid_encoding() {
        assert!(read_response(&vec![b'x';MAX_RESPONSE+1][..]).await.is_err());
        assert!(read_response(&[255u8][..]).await.is_err());
    }
    #[tokio::test] async fn rejects_private_and_injected_query_without_contacting_provider() {
        for address in ["127.0.0.1", "[::1]", "1.1.1.1\nend\nbegin", "https://bgp.tools", "1.1.1.1:443", "$(id)"] { assert!(resolve(address).await.is_err()); }
        assert_eq!(resolve("[2606:4700:4700::1111]").await.unwrap().0.len(),1);
    }
}

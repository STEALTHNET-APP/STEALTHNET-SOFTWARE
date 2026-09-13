//! Fixed-origin, bounded GitHub release check. Never executes an update from HTTP.
use crate::state::CurrentAdmin;
use axum::{extract::Query, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::OnceLock, time::{Duration, Instant}};

const MANUAL_CHECK_INTERVAL: u64 = 60;
type ReleaseCache = tokio::sync::Mutex<Option<(Instant, Value)>>;

#[derive(Default, Deserialize)]
pub struct CheckQuery { #[serde(default)] force: bool }

const REPO: &str = "https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE";
static CACHE: OnceLock<ReleaseCache> = OnceLock::new();

fn installed_version() -> &'static str { env!("CARGO_PKG_VERSION") }
fn metadata() -> Value {
    json!({"version": installed_version(), "commit":option_env!("SN_BUILD_COMMIT"),
        "branch":option_env!("SN_BUILD_BRANCH"), "built_at":option_env!("SN_BUILD_TIME"),
        "build_number":option_env!("SN_BUILD_NUMBER"), "target":option_env!("SN_BUILD_TARGET")})
}
fn parse_release(value: &Value, current: &str) -> Result<Value, ()> {
    let tag = value["tag_name"].as_str().ok_or(())?;
    if tag.len()>100 || !tag.bytes().all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c)) { return Err(()); }
    let remote = semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).map_err(|_| ())?;
    let local = semver::Version::parse(current).map_err(|_| ())?;
    if value["draft"] != false || value["prerelease"] != false || !remote.pre.is_empty() { return Err(()); }
    let assets = value["assets"].as_array().ok_or(())?;
    for arch in ["amd64","arm64"] {
        let name = format!("stealthnet-{tag}-linux-{arch}.tar.gz");
        for required in [name.clone(), format!("{name}.sha256")] {
            if !assets.iter().any(|a| a["name"] == required && a["size"].as_u64().is_some_and(|s| s>0)) { return Err(()); }
        }
    }
    Ok(json!({"status":if remote>local {"update_available"} else if remote<local {"ahead"} else {"current"},
        "latest":{"version":remote.to_string(),"tag":tag,"url":format!("{REPO}/releases/tag/{tag}"),
            "title":value["name"].as_str().unwrap_or(tag).chars().take(200).collect::<String>(),
            "notes":value["body"].as_str().unwrap_or("").chars().take(20000).collect::<String>(),
            "published_at":value["published_at"].as_str()}}))
}
async fn fetch_release() -> Result<Value, ()> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none()).user_agent("STEALTHNET-panel-release-check")
        .build().map_err(|_| ())?;
    let mut response = client.get("https://api.github.com/repos/STEALTHNET-APP/STEALTHNET-SOFTWARE/releases/latest")
        .header("Accept","application/vnd.github+json").header("Cache-Control", "no-cache").send().await.map_err(|_| ())?;
    if response.status()==reqwest::StatusCode::NOT_FOUND { return Ok(json!({"status":"no_releases"})); }
    if !response.status().is_success() { return Err(()); }
    let mut body=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_| ())? {
        if body.len()+chunk.len()>512*1024 { return Err(()); }
        body.extend_from_slice(&chunk);
    }
    parse_release(&serde_json::from_slice::<Value>(&body).map_err(|_| ())?, installed_version())
}
fn cache_interval(value: &Value) -> u64 {
    if value["status"] == "unavailable" { 60 } else { 900 }
}

async fn check_cached(
    cache: &ReleaseCache,
    force: bool,
    fetch: impl std::future::Future<Output = Result<Value, ()>>,
) -> Value {
    // Serialize checks so multiple admins cannot trigger duplicate GitHub requests.
    let mut cache = cache.lock().await;
    if let Some((at, value)) = &*cache {
        let age = at.elapsed().as_secs();
        let ttl = cache_interval(value);
        if age < if force { MANUAL_CHECK_INTERVAL } else { ttl } {
            let mut out = value.clone();
            out["cached"] = json!(true);
            out["check_interval_seconds"] = json!(ttl.saturating_sub(age));
            return out;
        }
    }
    let mut out = fetch.await.unwrap_or_else(|_| json!({"status":"unavailable"}));
    out["installed"] = metadata();
    out["repository"] = json!(REPO);
    out["checked_at"] = json!(chrono::Utc::now().to_rfc3339());
    out["cached"] = json!(false);
    out["check_interval_seconds"] = json!(cache_interval(&out));
    *cache = Some((Instant::now(), out.clone()));
    out
}

pub async fn check(_admin: CurrentAdmin, Query(query): Query<CheckQuery>) -> Json<Value> {
    Json(check_cached(CACHE.get_or_init(|| tokio::sync::Mutex::new(None)), query.force, fetch_release()).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag:&str)->Value {
        let mut assets=Vec::new();
        for arch in ["amd64","arm64"] { for suffix in ["",".sha256"] {assets.push(json!({"name":format!("stealthnet-{tag}-linux-{arch}.tar.gz{suffix}"),"size":100}));} }
        json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":assets,"body":"<script>untrusted</script>"})
    }
    #[test] fn compares_semantic_versions() {
        assert_eq!(parse_release(&release("v0.10.0"),"0.9.9").unwrap()["status"],"update_available");
        assert_eq!(parse_release(&release("v0.1.1"),"0.1.1").unwrap()["status"],"current");
        assert_eq!(parse_release(&release("v0.1.1"),"0.2.0").unwrap()["status"],"ahead");
    }
    #[test] fn requires_complete_stable_release() {
        let mut v=release("v0.2.0");v["assets"][0]["size"]=json!(0);assert!(parse_release(&v,"0.1.1").is_err());
        let mut v=release("v0.2.0");v["draft"]=json!(true);assert!(parse_release(&v,"0.1.1").is_err());
        assert!(parse_release(&release("v0.2.0-rc.1"),"0.1.1").is_err());
        assert!(parse_release(&release("v0.2.0/../../bad"),"0.1.1").is_err());
    }
    #[tokio::test]
    async fn manual_check_discovers_a_release_before_automatic_cache_expires() {
        let old = json!({"status":"current", "checked_at":"old"});
        let cache = ReleaseCache::new(Some((Instant::now() - Duration::from_secs(120), old)));
        let automatic = check_cached(&cache, false, async { panic!("automatic cache must avoid network") }).await;
        assert_eq!(automatic["status"], "current");
        assert_eq!(automatic["cached"], true);
        assert!(automatic["check_interval_seconds"].as_u64().unwrap() <= 780);
        let manual = check_cached(&cache, true, async { Ok(json!({"status":"update_available"})) }).await;
        assert_eq!(manual["status"], "update_available");
        assert_eq!(manual["cached"], false);
        assert_ne!(manual["checked_at"], "old");
        let repeated = check_cached(&cache, true, async { panic!("rapid clicks must reuse the fresh result") }).await;
        assert_eq!(repeated["status"], "update_available");
        assert_eq!(repeated["cached"], true);
    }
    #[tokio::test]
    async fn expired_cache_and_failed_request_do_not_report_old_success() {
        let cache = ReleaseCache::new(Some((Instant::now() - Duration::from_secs(901), json!({"status":"current"}))));
        let failed = check_cached(&cache, false, async { Err(()) }).await;
        assert_eq!(failed["status"], "unavailable");
        assert_eq!(failed["check_interval_seconds"], 60);
        *cache.lock().await = Some((Instant::now() - Duration::from_secs(61), failed));
        let recovered = check_cached(&cache, false, async { Ok(json!({"status":"update_available"})) }).await;
        assert_eq!(recovered["status"], "update_available");
        assert_eq!(recovered["cached"], false);
    }

}

//! Fixed-origin, bounded GitHub release check. Never executes an update from HTTP.
use crate::state::CurrentAdmin;
use axum::Json;
use serde_json::{json, Value};
use std::{sync::OnceLock, time::{Duration, Instant}};

const REPO: &str = "https://github.com/STEALTHNET-APP/STEALTHNET-SOFTWARE";
static CACHE: OnceLock<tokio::sync::Mutex<Option<(Instant, Value)>>> = OnceLock::new();

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
        .header("Accept","application/vnd.github+json").send().await.map_err(|_| ())?;
    if response.status()==reqwest::StatusCode::NOT_FOUND { return Ok(json!({"status":"no_releases"})); }
    if !response.status().is_success() { return Err(()); }
    let mut body=Vec::new();
    while let Some(chunk)=response.chunk().await.map_err(|_| ())? {
        if body.len()+chunk.len()>512*1024 { return Err(()); }
        body.extend_from_slice(&chunk);
    }
    parse_release(&serde_json::from_slice::<Value>(&body).map_err(|_| ())?, installed_version())
}
pub async fn check(_admin: CurrentAdmin) -> Json<Value> {
    let mut cache=CACHE.get_or_init(||tokio::sync::Mutex::new(None)).lock().await;
    if let Some((at,value))=&*cache {
        let ttl=if value["status"]=="unavailable" {60} else {900};
        if at.elapsed()<Duration::from_secs(ttl) { let mut out=value.clone();out["cached"]=json!(true);return Json(out); }
    }
    let mut out=fetch_release().await.unwrap_or_else(|_|json!({"status":"unavailable"}));
    out["installed"]=metadata(); out["repository"]=json!(REPO);
    out["checked_at"]=json!(chrono::Utc::now().to_rfc3339()); out["cached"]=json!(false);
    out["check_interval_seconds"]=json!(if out["status"]=="unavailable" {60} else {900});
    *cache=Some((Instant::now(),out.clone()));Json(out)
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
}

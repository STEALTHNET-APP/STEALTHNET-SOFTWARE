//! Official Xray release choices, cached on the panel instead of fetched by each browser.
use crate::state::CurrentAdmin;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sn_core::{Error, Result};
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};
pub const INSTALL_DEFAULT: &str = "v26.9.9";
#[derive(Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub prerelease: bool,
    pub published_at: String,
    pub url: String,
}
static CACHE: OnceLock<tokio::sync::Mutex<Option<(Instant, Vec<Release>)>>> = OnceLock::new();
pub fn valid_tag(s: &str) -> bool {
    let s = s.strip_prefix('v').unwrap_or(s);
    let parts = s.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()))
}
fn parse(rows: &Value) -> Vec<Release> {
    let mut out = rows
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            if r["draft"] == true || !valid_tag(tag) {
                return None;
            }
            let assets = r["assets"].as_array()?;
            if !["Xray-linux-64.zip", "Xray-linux-arm64-v8a.zip"]
                .iter()
                .all(|name| assets.iter().any(|a| a["name"] == *name))
            {
                return None;
            }
            Some(Release {
                tag: tag.into(),
                prerelease: r["prerelease"].as_bool().unwrap_or(true),
                published_at: r["published_at"].as_str().unwrap_or("").into(),
                url: format!("https://github.com/XTLS/Xray-core/releases/tag/{tag}"),
            })
        })
        .collect::<Vec<_>>();
    out.sort_by_key(|r| {
        std::cmp::Reverse(
            r.tag
                .trim_start_matches('v')
                .split('.')
                .filter_map(|p| p.parse::<u32>().ok())
                .collect::<Vec<_>>(),
        )
    });
    out.dedup_by(|a, b| a.tag == b.tag);
    out
}
pub async fn list(_a: CurrentAdmin) -> Result<Json<Value>> {
    let mut cache = CACHE
        .get_or_init(|| tokio::sync::Mutex::new(None))
        .lock()
        .await;
    if let Some((at, items)) = &*cache {
        if at.elapsed() < Duration::from_secs(300) {
            return Ok(Json(json!({"items":items,"stale":false})));
        }
    }
    let fetched = async {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("STEALTHNET-release-selector")
            .build()
            .map_err(|_| ())?;
        let rows = http
            .get("https://api.github.com/repos/XTLS/Xray-core/releases?per_page=100")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|_| ())?
            .error_for_status()
            .map_err(|_| ())?
            .json::<Value>()
            .await
            .map_err(|_| ())?;
        let items = parse(&rows);
        if items.is_empty() {
            Err(())
        } else {
            Ok(items)
        }
    }
    .await;
    match fetched {
        Ok(items) => {
            *cache = Some((Instant::now(), items.clone()));
            Ok(Json(json!({"items":items,"stale":false})))
        }
        Err(_) => {
            if let Some((_, items)) = &*cache {
                return Ok(Json(
                    json!({"items":items,"stale":true,"warning":"GitHub недоступен. Показан последний загруженный список."}),
                ));
            }
            Err(Error::bad("Не удалось загрузить релизы Xray с GitHub. Повторите запрос или укажите тег вручную."))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_order_channels_and_assets() {
        let row = |tag, pre| json!({"tag_name":tag,"prerelease":pre,"draft":false,"assets":[{"name":"Xray-linux-64.zip"},{"name":"Xray-linux-arm64-v8a.zip"}]});
        let mut draft = row("v99.1.1", false);
        draft["draft"] = json!(true);
        let mut missing = row("v98.1.1", false);
        missing["assets"] = json!([]);
        let a = parse(&json!([
            row("v26.7.11", true),
            row("v26.9.9", true),
            row("v26.3.27", false),
            draft,
            missing
        ]));
        assert_eq!(a.len(), 3);
        assert_eq!(a[0].tag, "v26.9.9");
        assert!(a[0].prerelease);
        assert_eq!(a.iter().find(|r| !r.prerelease).unwrap().tag, "v26.3.27");
    }
    #[test]
    fn tags_reject_paths_and_shell_input() {
        for s in ["v26.9.9", "26.9.9"] {
            assert!(valid_tag(s));
        }
        for s in ["latest", "../foo", "26.9.9;id", "26..9", "999999.1.1"] {
            assert!(!valid_tag(s));
        }
    }
}

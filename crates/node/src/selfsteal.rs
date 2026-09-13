//! Non-blocking preparation; the running engine is only switched after HTTPS works.
use serde_json::{json, Value};
use sn_core::selfsteal::Site;
use std::{path::Path, time::{Duration, Instant}, process::Stdio};
use tokio::{io::AsyncWriteExt, process::Command, task::JoinHandle};

const HELPER: &str = "/usr/local/lib/sn-node/selfsteal.py";
const STATUS: &str = "/etc/sn-selfsteal/status.json";

#[derive(Default)]
pub struct Controller {
    job: Option<(Site, JoinHandle<Result<Value, String>>)>,
    ready: Option<Site>,
    retry: Option<Instant>,
    pub status: Option<Value>,
    committed: Option<Option<Site>>,
}

impl Controller {
    pub async fn poll(&mut self, desired: Option<&Site>) -> bool {
        if self.job.as_ref().is_some_and(|(_, task)| task.is_finished()) {
            let (site, task) = self.job.take().unwrap();
            match task.await {
                Ok(Ok(status)) => { self.ready = Some(site); self.status = Some(status); }
                result => {
                    let error = match result { Ok(Err(e)) => e, _ => "Selfsteal preparation interrupted / Подготовка Selfsteal прервана".into() };
                    self.ready = None;
                    self.status = Some(json!({"phase":"error","domain":site.domain,"error":error}));
                }
            }
            self.retry = Some(Instant::now() + Duration::from_secs(60));
        }
        if let Some((site, _)) = &self.job {
            if let Ok(bytes) = std::fs::read(STATUS) {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    if value["domain"] == site.domain { self.status = Some(value); }
                }
            }
            return false;
        }
        let Some(site) = desired else { return true; };
        let changed = self.ready.as_ref() != Some(site);
        let may_retry = self.retry.is_none_or(|t| Instant::now() >= t);
        // A different desired domain is not held behind an old domain's backoff.
        let same_domain = self.status.as_ref().is_some_and(|s| s["domain"] == site.domain);
        if may_retry || changed && !same_domain {
            let site = site.clone();
            let job_site = site.clone();
            self.status = Some(json!({"phase":"preparing","domain":site.domain}));
            self.committed = None;
            self.job = Some((site, tokio::spawn(async move { helper("prepare", Some(&job_site)).await })));
            return false;
        }
        !changed
    }

    pub fn pending_message(&self) -> String {
        self.status.as_ref().and_then(|s| s["error"].as_str()).unwrap_or(
            "Preparing website and trusted certificate; previous configuration kept / Подготавливаем сайт и доверенный сертификат; прежняя конфигурация сохранена"
        ).to_string()
    }

    pub async fn commit(&mut self, site: Option<&Site>) -> Result<(), String> {
        if self.committed.as_ref() == Some(&site.cloned()) { return Ok(()); }
        // Do not create directories or invoke Python on ordinary nodes.
        if site.is_none() && !Path::new("/etc/sn-selfsteal/active.json").exists()
            && !Path::new("/etc/systemd/system/sn-selfsteal.service").exists() {
            self.committed = Some(None);
            return Ok(());
        }
        let status = helper("commit", site).await?;
        self.committed = Some(site.cloned());
        self.status = Some(status);
        if site.is_none() { self.ready = None; self.retry = None; }
        Ok(())
    }
}

async fn helper(operation: &str, site: Option<&Site>) -> Result<Value, String> {
    // Older native installations predate this dependency. Install it only when
    // enabling the managed website; ordinary nodes never invoke apt.
    if !Path::new("/usr/bin/python3").exists() {
        if !Path::new("/run/systemd/system").is_dir() || !Path::new("/usr/bin/apt-get").exists() {
            return Err("Selfsteal requires a native Debian/Ubuntu node with systemd / Для Selfsteal нужна обычная нода Debian/Ubuntu с systemd".into());
        }
        for args in [vec!["update", "-qq"], vec!["install", "-y", "--no-install-recommends", "python3", "ca-certificates"]] {
            let result = tokio::time::timeout(Duration::from_secs(240), Command::new("/usr/bin/apt-get")
                .args(args).env("DEBIAN_FRONTEND", "noninteractive").env("NEEDRESTART_MODE", "l")
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                .kill_on_drop(true).status()).await;
            if !matches!(result, Ok(Ok(status)) if status.success()) {
                return Err("Cannot install Python 3; check apt on the node / Не удалось установить Python 3; проверьте apt на ноде".into());
            }
        }
    }
    #[cfg(unix)] {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let parent = Path::new(HELPER).parent().unwrap();
        std::fs::create_dir_all(parent).map_err(|_| "Cannot create Selfsteal helper directory / Не удалось создать каталог Selfsteal")?;
        let meta = std::fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.uid() != 0 { return Err("Unsafe Selfsteal helper directory / Небезопасный каталог Selfsteal".into()); }
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        let source = include_bytes!("selfsteal.py");
        if std::fs::read(HELPER).ok().as_deref() != Some(source) {
            // A same-directory exclusive temporary file avoids symlink races.
            let temp = parent.join(format!("selfsteal-{}.tmp", uuid::Uuid::new_v4()));
            let mut file = std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&temp).map_err(|e| e.to_string())?;
            use std::io::Write;
            file.write_all(source).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            std::fs::rename(temp, HELPER).map_err(|e| e.to_string())?;
        }
    }
    let mut data = site.map(|site| {
        let mut value = json!(site);
        value["html"] = json!(sn_core::selfsteal_site::render(site));
        value
    }).unwrap_or(Value::Null).to_string();
    data.push('\n');
    let mut child = Command::new("/usr/bin/python3").arg(HELPER).arg(operation)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn().map_err(|_| "Python 3 is required for Selfsteal / Для Selfsteal нужен Python 3".to_string())?;
    child.stdin.take().unwrap().write_all(data.as_bytes()).await.map_err(|e| e.to_string())?;
    let result = tokio::time::timeout(Duration::from_secs(300), child.wait_with_output()).await
        .map_err(|_| "Selfsteal timed out / Истекло время подготовки Selfsteal")?
        .map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(&result.stdout).map_err(|_| "Selfsteal helper failed; check node logs / Ошибка Selfsteal; проверьте журнал ноды".to_string())?;
    if !result.status.success() {
        return Err(value["error"].as_str().unwrap_or("Selfsteal failed").to_string());
    }
    Ok(value)
}

//! Обновление движка на ноде по команде из панели.
//!
//! Версии Xray выходят часто, и ставить их руками на каждой ноде — работа,
//! которая не масштабируется: парк растёт, а обновление протокола вроде
//! Hysteria 2 ломает клиентов, пока ноды сидят на старом ядре.
//!
//! Панель хранит желаемую версию, агент видит её при опросе и, если у него
//! другая, скачивает релиз с GitHub и подменяет бинарь. Сеть агент трогает
//! только когда версии разошлись: на каждый опрос ходить наружу незачем.
//!
//! Безопасность подмены важнее скорости: качаем во временный файл,
//! проверяем сигнатуру ELF и то, что бинарь вообще запускается и
//! называет свою версию. Только потом заменяем рабочий и перезапускаем
//! движок. Оборванная закачка на месте боевого файла означала бы ноду,
//! которая больше не поднимется.

use std::path::Path;
use tokio::process::Command;

/// Имя архитектуры в релизах Xray — своё, не совпадает с нашим.
fn xray_arch() -> Option<&'static str> {
    Some(match std::env::consts::ARCH {
        "x86_64" => "64",
        "aarch64" => "arm64-v8a",
        _ => return None,
    })
}

/// Версии в сравнимый вид: `Xray 26.7.11` и `v26.7.11` — одно и то же.
pub fn normalize(v: &str) -> String {
    v.trim()
        .trim_start_matches("Xray ")
        .trim_start_matches('v')
        .trim()
        .to_string()
}

/// Нужно ли обновляться.
pub fn needs_update(current: Option<&str>, target: &str) -> bool {
    let target = normalize(target);
    if target.is_empty() {
        return false;
    }
    match current {
        Some(c) => normalize(c) != target,
        // Версию не определили — лучше не трогать рабочий движок вслепую.
        None => false,
    }
}

/// Download and verify before an atomic replacement. Keep the old executable for rollback.
pub async fn install(bin: &str, config: &Path, target: &str) -> Result<String, String> {
    let normalized = normalize(target);
    if normalized.split('.').count() != 3
        || normalized
            .split('.')
            .any(|p| p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("недопустимый тег Xray".into());
    }
    let arch = xray_arch().ok_or("неподдерживаемая архитектура Xray")?;
    let tag = format!("v{normalized}");
    let url =
        format!("https://github.com/XTLS/Xray-core/releases/download/{tag}/Xray-linux-{arch}.zip");
    let tmp = std::env::temp_dir().join(format!("sn-xray-{}", sn_core::auth::generate_token()));
    tokio::fs::DirBuilder::new().mode(0o700).create(&tmp)
        .await
        .map_err(|e| e.to_string())?;
    let result = async {
        let zip = tmp.join("x.zip");
        for (dest, source) in [(zip.clone(),url.clone()),(tmp.join("x.dgst"),format!("{url}.dgst"))] {
            let out=Command::new("curl").args(["--proto","=https","--proto-redir","=https","-fsSL","--connect-timeout","15","--max-time","300","-o"]).arg(dest).arg(source).output().await.map_err(|e|e.to_string())?;
            if !out.status.success(){return Err("не удалось загрузить архив Xray или контрольную сумму".into());}
        }
        let bytes=tokio::fs::read(&zip).await.map_err(|e|e.to_string())?;
        let digest=tokio::fs::read_to_string(tmp.join("x.dgst")).await.map_err(|e|e.to_string())?;
        verify_digest(&bytes,&digest)?;
        let out=Command::new("unzip").arg("-oq").arg(&zip).arg("xray").arg("-d").arg(&tmp).output().await.map_err(|e|e.to_string())?;
        if !out.status.success(){return Err("архив Xray не распаковался".into());}
        let fresh=tmp.join("xray");
        if !is_elf(&fresh).await {return Err("в архиве отсутствует бинарник Xray".into());}
        tokio::fs::set_permissions(&fresh,std::os::unix::fs::PermissionsExt::from_mode(0o755)).await.map_err(|e|e.to_string())?;
        let ver=Command::new(&fresh).arg("version").output().await.map_err(|e|e.to_string())?;
        let installed=String::from_utf8_lossy(&ver.stdout).lines().next().and_then(|l|l.split_whitespace().nth(1)).unwrap_or("").to_string();
        if !ver.status.success() || normalize(&installed)!=normalized {return Err("скачанный Xray не запускается или сообщил другую версию".into());}
        if config.exists() {
            let test=tokio::time::timeout(std::time::Duration::from_secs(30),Command::new(&fresh).args(["run","-test","-c"]).arg(config).output()).await.map_err(|_|"проверка конфига Xray превысила 30 секунд")?.map_err(|e|e.to_string())?;
            if !test.status.success(){return Err("новая версия отвергла текущий конфиг; рабочая версия сохранена. Проверьте профиль и совместимость релиза".into());}
        }
        atomic_replace(&fresh,Path::new(bin)).await?;
        Ok(installed)
    }.await;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    result
}
fn verify_digest(bytes: &[u8], digest: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let expected = digest
        .lines()
        .find_map(|line| {
            line.strip_prefix("SHA2-256=")
                .or_else(|| line.strip_prefix("SHA256="))
        })
        .map(str::trim)
        .ok_or("в релизе нет SHA256")?;
    if expected.len() != 64
        || !expected.bytes().all(|b| b.is_ascii_hexdigit())
        || !hex::encode(Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
    {
        return Err("SHA256 архива Xray не совпадает; рабочая версия сохранена".into());
    }
    Ok(())
}
fn verify_agent_digest(bytes: &[u8], digest: &str) -> Result<(), String> {
    let expected = digest.split_whitespace().next().ok_or("нет SHA256 агента")?;
    verify_digest(bytes, &format!("SHA2-256= {expected}"))
}

async fn atomic_replace(fresh: &Path, bin: &Path) -> Result<(), String> {
    let staged = bin.with_extension("new");
    tokio::fs::copy(fresh, &staged)
        .await
        .map_err(|e| e.to_string())?;
    // Backup is copied to a new inode too: an earlier executable may still be running.
    let backup = bin.with_extension("previous.new");
    tokio::fs::copy(bin, &backup)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(backup, bin.with_extension("previous"))
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(staged, bin)
        .await
        .map_err(|e| format!("не заменил Xray: {e}"))
}
pub async fn rollback(bin: &str) -> Result<(), String> {
    tokio::fs::rename(Path::new(bin).with_extension("previous"), bin)
        .await
        .map_err(|e| format!("не восстановил Xray: {e}"))
}

async fn is_elf(path: &Path) -> bool {
    match tokio::fs::read(path).await {
        Ok(b) => b.starts_with(&[0x7f, b'E', b'L', b'F']),
        Err(_) => false,
    }
}

/// Обновление самого агента.
///
/// Движок агент обновить умеет, а себя — нет: на ноде, поставленной до
/// появления этой возможности, команда из панели просто не понимается, и
/// такая нода остаётся со старым Xray навсегда. Заметно это становится
/// как «версии разные», а починить можно только руками на сервере.
///
/// Порядок тот же, что у движка: скачали, убедились, что это работающий
/// бинарь, и только потом подменили. Замену делаем переименованием:
/// писать в файл запущенной программы ядро не даёт (`ETXTBSY`), а
/// переименование меняет запись в каталоге — запущенный процесс
/// продолжает жить на прежнем inode.
///
/// После подмены агент завершается: systemd поднимет его заново уже из
/// нового файла. Перезапускать движок не нужно — он отдельный процесс.
pub async fn replace_self(sources: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("не знаю своего пути: {e}"))?;
    let tmp = exe.with_extension("new");

    let mut last = String::from("источники не заданы");
    for url in sources {
        if !url.starts_with("https://") { last = "обновление агента требует HTTPS".into(); continue; }
        let out = Command::new("curl")
            .args(["--proto", "=https", "--proto-redir", "=https", "-fsSL", "--connect-timeout", "15", "--max-time", "300", "-o"])
            .arg(&tmp)
            .arg(url)
            .output()
            .await
            .map_err(|e| format!("не запустил curl: {e}"))?;
        if !out.status.success() {
            last = format!("не скачал {url}");
            continue;
        }
        let digest = Command::new("curl")
            .args(["--proto", "=https", "--proto-redir", "=https", "-fsSL", "--connect-timeout", "15", "--max-time", "30"])
            .arg(format!("{url}.sha256")).output().await.map_err(|e| e.to_string())?;
        let bytes = tokio::fs::read(&tmp).await.map_err(|e| e.to_string())?;
        if !digest.status.success() || verify_agent_digest(&bytes, &String::from_utf8_lossy(&digest.stdout)).is_err() {
            last = "SHA256 агента отсутствует или не совпадает; рабочая версия сохранена".into();
            let _ = tokio::fs::remove_file(&tmp).await;
            continue;
        }
        if !is_elf(&tmp).await {
            last = format!("по адресу {url} не бинарь");
            let _ = tokio::fs::remove_file(&tmp).await;
            continue;
        }

        tokio::fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .await
            .map_err(|e| e.to_string())?;

        // Пробный запуск: битая сборка не должна становиться рабочей —
        // после подмены systemd будет поднимать её по кругу.
        match Command::new(&tmp).arg("--version").output().await {
            Ok(out)
                if out.status.success()
                    && String::from_utf8_lossy(&out.stdout).starts_with("sn-node ") => {}
            Ok(_) => {
                last = "скачанный агент не прошёл проверку --version".into();
                continue;
            }
            Err(e) => {
                last = format!("скачанный агент не запускается: {e}");
                let _ = tokio::fs::remove_file(&tmp).await;
                continue;
            }
        }

        tokio::fs::rename(&tmp, &exe)
            .await
            .map_err(|e| format!("не подменил {}: {e}", exe.display()))?;
        return Ok(());
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_checksum_requires_matching_sha256() {
        use sha2::{Digest, Sha256};
        let hash = hex::encode(Sha256::digest(b"agent"));
        assert!(verify_agent_digest(b"agent", &hash).is_ok());
        assert!(verify_agent_digest(b"agent", &format!("{hash}  sn-node")).is_ok());
        for bad in ["", "<html>not found</html>", "abcd"] {
            assert!(verify_agent_digest(b"agent", bad).is_err());
        }
        assert!(verify_agent_digest(b"tampered", &hash).is_err());
    }

    #[test]
    fn checksum_rejects_corruption_and_missing_hash() {
        use sha2::{Digest, Sha256};
        let hash = format!("SHA2-256= {}", hex::encode(Sha256::digest(b"archive")));
        assert!(verify_digest(b"archive", &hash).is_ok());
        assert!(verify_digest(b"corrupted", &hash).is_err());
        assert!(verify_digest(b"archive", "MD5= abc").is_err());
    }
    #[tokio::test]
    async fn replacement_keeps_old_inode_and_rolls_back() {
        use tokio::io::AsyncReadExt;
        let dir = std::env::temp_dir().join(format!("sn-replace-test-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let bin = dir.join("xray");
        let fresh = dir.join("fresh");
        tokio::fs::write(&bin, b"old").await.unwrap();
        tokio::fs::write(&fresh, b"new").await.unwrap();
        let mut running = tokio::fs::File::open(&bin).await.unwrap();
        atomic_replace(&fresh, &bin).await.unwrap();
        let mut old = Vec::new();
        running.read_to_end(&mut old).await.unwrap();
        assert_eq!(old, b"old");
        assert_eq!(tokio::fs::read(&bin).await.unwrap(), b"new");
        rollback(bin.to_str().unwrap()).await.unwrap();
        assert_eq!(tokio::fs::read(&bin).await.unwrap(), b"old");
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[test]
    fn versions_compare_regardless_of_prefix() {
        assert!(!needs_update(Some("26.7.11"), "v26.7.11"));
        assert!(!needs_update(Some("v26.7.11"), "26.7.11"));
        assert!(needs_update(Some("26.3.27"), "v26.7.11"));
    }

    #[test]
    fn missing_pieces_mean_no_update() {
        // Пустая цель — обновление не задано, движок не трогаем.
        assert!(!needs_update(Some("26.3.27"), ""));
        // Своя версия неизвестна — обновлять вслепую опаснее, чем не трогать.
        assert!(!needs_update(None, "v26.7.11"));
    }
}

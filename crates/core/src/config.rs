/// Конфигурация из окружения. Ничего не хардкодим — проект ставят чужие люди.
#[derive(Debug, Clone)]
pub struct Config {
    /// Пусто в режиме `api`: у машины сабки базы нет и быть не должно.
    pub database_url: String,
    pub api_bind: String,
    pub sub_bind: String,
    /// Публичный адрес выдачи подписок, попадает в ссылки клиентам.
    pub sub_public_url: String,
    pub brand_name: String,
    pub web_root: String,
    /// Откуда сервис подписок берёт данные: `db` (одна машина) или `api`
    /// (сабка вынесена на отдельный сервер и ходит в панель по HTTP).
    pub sub_mode: String,
    /// Адрес панели — нужен только в режиме `api`.
    pub panel_url: String,
    /// Сервисный токен для доступа сабки к API. Права только на чтение подписок.
    pub sub_service_token: Option<String>,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    pub fn from_env() -> crate::Result<Self> {
        let sub_mode = env_or("SUB_MODE", "db").to_lowercase();

        // Сабку выносят на отдельный сервер именно затем, чтобы базы там не
        // было: она ходит в панель по HTTP. Требовать DATABASE_URL в этом
        // режиме значит заставить придумывать фиктивную строку подключения.
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) => url,
            Err(_) if sub_mode == "api" => String::new(),
            Err(_) => return Err(crate::Error::Internal("не задан DATABASE_URL".into())),
        };

        Ok(Config {
            database_url,
            api_bind: env_or("API_BIND", "127.0.0.1:8080"),
            sub_bind: env_or("SUB_BIND", "127.0.0.1:8081"),
            sub_public_url: env_or("SUB_PUBLIC_URL", "http://localhost:8081"),
            brand_name: env_or("BRAND_NAME", "VPN Panel"),
            web_root: env_or("WEB_ROOT", "./web"),
            sub_mode,
            panel_url: env_or("PANEL_URL", "http://127.0.0.1:8080")
                .trim_end_matches('/')
                .to_string(),
            sub_service_token: std::env::var("SUB_SERVICE_TOKEN").ok().filter(|t| !t.is_empty()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Переменные окружения — общие на весь процесс, поэтому тесты, которые
    /// их правят, должны идти по одному.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_env(vars: &[(&str, Option<&str>)], f: impl FnOnce()) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<_> = vars
            .iter()
            .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        f();
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(&k, v),
                None => std::env::remove_var(&k),
            }
        }
    }

    #[test]
    fn режим_api_не_требует_базы() {
        // Сабку выносят на отдельный сервер ровно затем, чтобы базы там не
        // было. Требование DATABASE_URL ломало бы этот сценарий целиком.
        with_env(
            &[("SUB_MODE", Some("api")), ("DATABASE_URL", None)],
            || {
                let cfg = Config::from_env().expect("режим api должен подниматься без базы");
                assert!(cfg.database_url.is_empty());
                assert_eq!(cfg.sub_mode, "api");
            },
        );
    }

    #[test]
    fn обычный_режим_без_базы_не_стартует() {
        with_env(&[("SUB_MODE", None), ("DATABASE_URL", None)], || {
            assert!(Config::from_env().is_err(), "без базы режим db работать не может");
        });
    }
}

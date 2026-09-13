use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("не найдено")]
    NotFound,
    #[error("требуется вход")]
    Unauthorized,
    #[error("превышен лимит запросов; повторите позже")]
    TooManyRequests,
    #[error("недостаточно прав")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("конфликт: {0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Internal(String),
}

impl Error {
    pub fn bad(msg: impl Into<String>) -> Self {
        Error::BadRequest(msg.into())
    }

    fn parts(&self) -> (StatusCode, String) {
        match self {
            Error::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Error::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            Error::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            Error::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            Error::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Error::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            // Нарушение уникальности отдаём как 409, а не как 500:
            // на этом держится защита от повторного вебхука.
            //
            // В сообщение подставляем поле: «запись с таким значением уже
            // существует» не говорит, что именно менять, и человек ищет
            // совпадение вслепую по всей форме.
            Error::Db(sqlx::Error::Database(e)) if e.is_unique_violation() => (
                StatusCode::CONFLICT,
                match unique_field(e.constraint()) {
                    Some(f) => format!("{f} уже занято — выберите другое"),
                    None => "запись с таким значением уже существует".into(),
                },
            ),
            Error::Db(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => (
                StatusCode::CONFLICT, "связанная запись отсутствует или используется; обновите список и проверьте связи".into(),
            ),
            Error::Db(sqlx::Error::Database(e)) if e.is_check_violation() || e.code().is_some_and(|c| c == "22P02" || c == "22003") => (
                StatusCode::BAD_REQUEST, "недопустимое значение: проверьте формат и диапазон полей".into(),
            ),
            Error::Db(sqlx::Error::RowNotFound) => (StatusCode::NOT_FOUND, "не найдено".into()),
            Error::Db(e) => {
                tracing::error!(error = %e, "ошибка БД");
                (StatusCode::INTERNAL_SERVER_ERROR, "внутренняя ошибка".into())
            }
            Error::Internal(m) => {
                tracing::error!(error = %m, "внутренняя ошибка");
                (StatusCode::INTERNAL_SERVER_ERROR, "внутренняя ошибка".into())
            }
        }
    }
}

/// Человеческое название поля по имени нарушенного ограничения.
///
/// Список ведётся вручную: имена ограничений задаются миграциями, и
/// выводить название поля разбором строки — значит однажды показать
/// человеку кусок служебного идентификатора. Чего нет в списке — то
/// остаётся общим сообщением.
fn unique_field(constraint: Option<&str>) -> Option<&'static str> {
    Some(match constraint? {
        "nodes_name_key" | "nodes_name_live_key" => "Имя ноды",
        "config_profiles_name_key" => "Название профиля",
        "squads_name_key" | "external_squads_name_key" => "Название сквада",
        "infra_providers_name_key" => "Название хостера",
        "tariffs_code_key" => "Код тарифа",
        "promo_codes_code_key" => "Промокод",
        "partners_slug_key" => "Ссылка партнёра",
        "subscription_templates_code_key" => "Код шаблона",
        "admins_username_key" => "Логин",
        "admins_email_key" => "Почта",
        _ => return None,
    })
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (code, message) = self.parts();
        (code, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

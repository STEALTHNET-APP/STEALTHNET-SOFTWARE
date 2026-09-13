use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sn_core::auth::Admin;
use sn_core::{Config, Error, Pool};

#[derive(Clone)]
pub struct AppState {
    /// Незавершённые церемонии passkey: живут секунды, держим в памяти.
    pub ceremonies: crate::passkeys::Ceremonies,
    pub pool: Pool,
    pub config: Config,
    pub payments: std::sync::Arc<sn_payments::Registry>,
}

/// Экстрактор авторизованного администратора.
/// Наличие `CurrentAdmin` в сигнатуре обработчика = маршрут закрыт.
pub struct CurrentAdmin(pub Admin);

impl FromRequestParts<AppState> for CurrentAdmin {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(Error::Unauthorized)?;

        let (admin, credential) = match sn_core::auth::admin_by_token(&state.pool, token).await {
            Ok(admin) => (admin, None),
            Err(Error::Unauthorized) => {
                let (admin, id, scopes) = sn_core::auth::api_admin_by_token(&state.pool, token).await?;
                (admin, Some((id, scopes)))
            }
            Err(error) => return Err(error),
        };
        if !role_allows(&admin.role, parts.method.as_str(), parts.uri.path()) {
            return Err(Error::Forbidden);
        }
        if let Some((id, scopes)) = credential {
            if !token_allows(&scopes, parts.method.as_str(), parts.uri.path()) {
                return Err(Error::Forbidden);
            }
            sqlx::query("UPDATE api_tokens SET last_used_at=now() WHERE id=$1")
                .bind(id).execute(&state.pool).await?;
        }
        Ok(CurrentAdmin(admin))
    }
}

/// PATCH distinguishes an omitted property from an explicit JSON null.
pub fn patch_field<'de, D, T>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where D: serde::Deserializer<'de>, T: serde::Deserialize<'de> {
    <Option<T> as serde::Deserialize>::deserialize(de).map(Some)
}

fn is_read(method: &str) -> bool { matches!(method, "GET" | "HEAD" | "OPTIONS") }
fn is_preview(method: &str, path: &str) -> bool { method == "POST" && matches!(path, "/api/sub/preview" | "/api/sub/template-preview") }
fn under(path: &str, root: &str) -> bool { path == root || path.strip_prefix(root).is_some_and(|p| p.starts_with('/')) }

pub fn role_allows(role: &str, method: &str, path: &str) -> bool {
    if !sn_core::auth::valid_role(role) { return false; }
    if under(path, "/api/team") { return role == "owner"; }
    // These handlers operate only on the authenticated person's own account.
    if under(path, "/api/admin") || path == "/api/auth/me" { return true; }
    match role {
        "owner" | "admin" => true,
        "readonly" => is_read(method) || is_preview(method, path),
        "support" => {
            let readable = ["/api/dashboard", "/api/clients", "/api/tariffs", "/api/payments", "/api/tickets", "/api/nodes", "/api/hosts", "/api/squads", "/api/sessions", "/api/devices", "/api/srh", "/api/reports", "/api/whoami"];
            if is_read(method) { return readable.iter().any(|root| under(path, root)); }
            if under(path, "/api/tickets") { return true; }
            let parts: Vec<_> = path.trim_matches('/').split('/').collect();
            match parts.as_slice() {
                ["api", "clients", id] if id.parse::<i64>().is_ok() => method == "PATCH",
                ["api", "clients", id, "revoke" | "reset-traffic" | "messages"] if id.parse::<i64>().is_ok() => method == "POST",
                ["api", "clients", id, "devices", _] if id.parse::<i64>().is_ok() => method == "DELETE",
                _ => false,
            }
        }
        _ => false,
    }
}

fn token_allows(scopes: &[String], method: &str, path: &str) -> bool {
    // A service integration must never turn its token into account credentials.
    if path.split('/').any(|p|p=="cabinet-code") || under(path, "/api/admin") || under(path, "/api/team") || under(path, "/api/tokens") || (under(path, "/api/auth") && path != "/api/auth/me") { return false; }
    if scopes.iter().any(|s| !matches!(s.as_str(), "read" | "write")) { return false; }
    // Legacy empty scopes were not usable before token authentication existed.
    // Start them with read access; write access must be explicitly requested.
    is_read(method) || is_preview(method, path) || scopes.iter().any(|s| s == "write")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_and_unknown_roles_cannot_change_panel_data() {
        for role in ["admin", "support", "readonly"] { for method in ["GET", "POST", "PATCH", "DELETE"] { assert!(!role_allows(role, method, "/api/team/1")); } }
        for method in ["POST", "PATCH", "PUT", "DELETE"] {
            assert!(!role_allows("readonly", method, "/api/settings"));
            assert!(!role_allows("viewer-typo", method, "/api/admin/password"));
        }
        assert!(role_allows("readonly", "GET", "/api/nodes"));
        assert!(role_allows("readonly", "POST", "/api/admin/password"));
    }
    #[test]
    fn support_can_resolve_client_issues_but_not_change_infrastructure_or_sales() {
        for (method, path) in [("PATCH", "/api/clients/12"), ("POST", "/api/clients/12/revoke"), ("DELETE", "/api/clients/12/devices/test"), ("POST", "/api/tickets/9/reply")] {
            assert!(role_allows("support", method, path), "{method} {path}");
        }
        for (method, path) in [("POST", "/api/clients/bulk"), ("DELETE", "/api/clients/12"), ("PATCH", "/api/nodes/3"), ("POST", "/api/pay/12/confirm"), ("GET", "/api/settings"), ("GET", "/api/profiles")] {
            assert!(!role_allows("support", method, path), "{method} {path}");
        }
    }
    #[test]
    fn token_scopes_do_not_allow_credential_pivoting_or_implicit_writes() {
        assert!(token_allows(&[], "GET", "/api/clients"));
        assert!(!token_allows(&[], "POST", "/api/clients"));
        assert!(!token_allows(&["read".into()], "DELETE", "/api/clients/1"));
        assert!(token_allows(&["write".into()], "PATCH", "/api/clients/1"));
        for path in ["/api/team", "/api/team/1/password", "/api/admin/password", "/api/admin/totp/setup", "/api/tokens", "/api/tokens/1", "/api/clients/1/cabinet-code", "/api/clients/1/cabinet-code/reset"] {
            assert!(!token_allows(&["read".into(), "write".into()], "POST", path));
        }
        assert!(!token_allows(&["unknown".into()], "GET", "/api/clients"));
    }
}

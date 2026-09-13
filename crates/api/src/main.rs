//! API панели управления.

mod admin_routes;
mod team;
mod catalog_routes;
mod crm_routes;
mod broadcast_media;
mod infra_routes;
mod miniapp;
mod cabinet;
mod node_admin;
mod node_bgp;
mod xray_releases;
mod panel_release;
mod profile_workflow;
mod node_routes;
mod passkeys;
mod pay_routes;
mod sub_routes;
mod sub_admin;
mod sub_service;
mod routes;
mod state;
mod security;

use axum::routing::get;
use axum::Router;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;

use sn_core::{Config, Result};
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        // sn_core здесь обязателен: именно там логируются ошибки БД.
        // Без него «внутренняя ошибка» в ответе не имеет следа в журнале.
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sn_api=info,tower_http=info,sn_core=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = sn_core::db::connect(&config.database_url).await?;
    tracing::info!("подключились к БД");

    // Реестр платёжных модулей собирается один раз при старте
    // и сразу отражается в БД, чтобы панель показывала реальную картину.
    let payments = std::sync::Arc::new(sn_payments::Registry::from_env());
    payments.sync_to_db(&pool).await?;
    for p in payments.all() {
        tracing::info!(
            модуль = p.id(),
            настроен = p.is_configured(),
            валюты = ?p.currencies(),
            "платёжный модуль"
        );
    }

    let state = AppState {
        ceremonies: Default::default(),
        pool,
        config: config.clone(),
        payments,
    };

    let cors_state = state.clone();
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(AllowOrigin::async_predicate(move |origin, _| {
            let st = cors_state.clone();
            async move {
                let expected = sub_service::panel_public_url_pub(&st).await;
                origin.as_bytes() == expected.trim_end_matches('/').as_bytes()
            }
        }));

    let app = Router::new()
        .route("/api/health", get(routes::health))
        .route("/api/system/release", get(panel_release::check))
        .merge(routes::auth_routes())
        .merge(routes::panel_routes())
        .merge(profile_workflow::routes())
        .merge(pay_routes::payment_routes())
        .merge(node_routes::node_routes())
        .merge(node_admin::node_admin_routes())
        .merge(sub_routes::sub_routes())
        .merge(crm_routes::crm_routes())
        .merge(broadcast_media::routes())
        .merge(catalog_routes::catalog_routes())
        .merge(sub_service::sub_service_routes())
        .merge(infra_routes::infra_routes())
        .merge(miniapp::miniapp_routes(state.clone()))
        .merge(cabinet::routes())
        .merge(sub_admin::sub_admin_routes())
        .merge(admin_routes::admin_routes())
        .merge(team::routes())
        .merge(passkeys::passkey_routes())
        // Паника в обработчике не должна рвать соединение — отдаём 500.
        .layer(CatchPanicLayer::new())
        .layer(cors)
        .layer(axum::middleware::from_fn(security::response_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.api_bind)
        .await
        .map_err(|e| sn_core::Error::Internal(format!("не смог занять {}: {e}", config.api_bind)))?;

    tracing::info!("API панели слушает {}", config.api_bind);
    // Адрес клиента нужен белому списку внешних сквадов. Без
    // connect-info экстрактор падал бы в рантайме, а не на сборке.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
        .await
        .map_err(|e| sn_core::Error::Internal(e.to_string()))?;
    Ok(())
}

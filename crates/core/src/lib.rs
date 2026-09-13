//! Общее ядро: конфигурация, ошибки, БД, аутентификация, форматирование.

pub mod auth;
pub mod cabinet;
pub mod customer_brand;
pub mod billing;
pub mod addons;
pub mod config;
pub mod db;
pub mod error;
pub mod keygen;
pub mod money;
pub mod partners;
pub mod service_parts;
pub mod totp;
pub mod xray_check;

pub use config::Config;
pub use error::{Error, Result};

/// Пул соединений с БД, разделяемый сервисами.
pub type Pool = sqlx::PgPool;

pub mod bot_config;
pub mod telegram_client;

pub mod alerts;

pub mod profile_workflow;

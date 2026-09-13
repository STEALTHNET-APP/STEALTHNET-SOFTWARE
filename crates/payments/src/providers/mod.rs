//! Реализации платёжных модулей.
//!
//! Каждый модуль — отдельный файл, реализующий `PaymentProvider`.
//! Добавили свой — не забудьте зарегистрировать его в `Registry::from_env`.

pub mod cryptobot;
pub mod manual;
pub mod stars;
pub mod generic_http;

pub mod hosted;

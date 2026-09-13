//! Утилита создания администратора: `sn-admin <логин> <пароль>`.
//! Отдельный бинарник, чтобы не заводить в API «ручку создания первого админа» —
//! такие ручки регулярно забывают закрыть.

use sn_core::{Config, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!("использование: sn-admin --password-stdin <логин> [роль]");
        std::process::exit(2);
    };
    let (username, password) = if first == "--password-stdin" {
        use std::io::Read;
        let username = args.next().ok_or_else(|| sn_core::Error::BadRequest("укажите логин".into()))?;
        let mut password = String::new();
        std::io::stdin().take(1025).read_to_string(&mut password)
            .map_err(|_| sn_core::Error::BadRequest("не удалось прочитать пароль".into()))?;
        if password.len() > 1024 {
            return Err(sn_core::Error::BadRequest("пароль слишком длинный".into()));
        }
        (username, password.trim_end_matches(['\r', '\n']).to_owned())
    } else {
        let password = args.next().ok_or_else(|| sn_core::Error::BadRequest("укажите пароль".into()))?;
        (first, password)
    };
    let role = args.next().unwrap_or_else(|| "owner".into());
    if !sn_core::auth::valid_role(&role) {
        eprintln!("роль: owner, admin, support или readonly");
        std::process::exit(2);
    }

    if password.len() < 8 {
        eprintln!("пароль короче 8 символов — так нельзя");
        std::process::exit(2);
    }

    let config = Config::from_env()?;
    let pool = sn_core::db::connect(&config.database_url).await?;
    let hash = sn_core::auth::hash_password(&password)?;

    // Повторный запуск меняет пароль, а не падает: так восстанавливают доступ.
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO admins (username, password_hash, role)
         VALUES ($1, $2, $3)
         ON CONFLICT ((lower(username))) DO UPDATE
            SET password_hash = EXCLUDED.password_hash, is_active = true
         RETURNING id",
    )
    .bind(&username)
    .bind(&hash)
    .bind(&role)
    .fetch_one(&pool)
    .await?;

    println!("администратор «{username}» готов (id={id}, роль={role})");
    Ok(())
}

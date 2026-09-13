//! «Позвать друга» — партнёрская программа глазами клиента.
//!
//! Таблицы партнёров и комиссий уже есть, и администратор ведёт их в
//! панели. Здесь та же запись, но заводится самим клиентом из бота: он
//! получает свою ссылку, а начисления идут в тот же `partner_commissions`
//! и попадают в отчёты панели наравне с остальными.
//!
//! Ставка берётся из настроек — общая для всех, кто пришёл через бота.
//! Индивидуальные ставки задаются в панели и здесь не трогаются: правка
//! из бота молча перетирала бы то, о чём договорились отдельно.

use sn_core::{money, Pool, Result};
use sqlx::Row;

use crate::settings::Screen;
use crate::tg::{keyboard, Btn};
use crate::Ctx;

/// Экран приглашения: ссылка, счётчики, условия.
pub async fn show(c: &Ctx<'_>, chat: i64, client_id: i64, bot_name: &str, mid: Option<i64>) -> Result<()> {
    if !c.s.referral_enabled() {
        // Выключено — возвращаем в меню, а не показываем пустой экран.
        return crate::show_menu(c, chat, client_id, mid).await;
    }

    let partner = ensure_partner(c.pool, client_id, c.s.referral_percent()).await?;
    let Some((partner_id, slug, percent)) = partner else {
        c.tg.screen(
            chat, mid,
            "Пригласить друга пока нельзя — напишите в поддержку.",
            c.s.image(Screen::Menu).as_deref(),
            Some(keyboard(vec![vec![Btn::Data("← Назад".into(), "menu".into())]])),
        ).await?;
        return Ok(());
    };

    // Счётчики: сколько пришло и сколько начислено. Показываем как есть —
    // выдумывать «примерный доход» тут нечего.
    let invited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM clients WHERE referred_by = $1 AND deleted_at IS NULL",
    )
    .bind(partner_id)
    .fetch_one(c.pool)
    .await?;

    let wallets = sqlx::query("SELECT currency,earned_minor,balance_minor FROM partner_wallets WHERE partner_id=$1 ORDER BY currency")
        .bind(partner_id).fetch_all(c.pool).await?;
    let earned = wallets.iter().map(|r| money::format_minor(r.get("earned_minor"), &r.get::<String,_>("currency"))).collect::<Vec<_>>().join(" · ");
    let balance = wallets.iter().map(|r| money::format_minor(r.get("balance_minor"), &r.get::<String,_>("currency"))).collect::<Vec<_>>().join(" · ");
    let link = format!("https://t.me/{bot_name}?start=r_{slug}");

    let mut text = format!(
        "<b>Пригласить друга</b>\n\n\
         Вы получаете <b>{percent}%</b> с каждой его оплаты — не только с первой.\n\n\
         Приглашено: <b>{invited}</b>\n\
         Начислено всего: <b>{}</b>\n\
         Доступно к выплате: <b>{}</b>\n\n\
         Ваша ссылка — нажмите, чтобы скопировать:\n<code>{link}</code>",
        earned,
        balance,
    );
    if invited == 0 {
        text.push_str("\n\nОтправьте её тому, кому VPN пригодится.");
    }

    let share = format!(
        "https://t.me/share/url?url={}&text={}",
        urlencode(&link),
        urlencode("Пользуюсь этим VPN — рекомендую"),
    );

    c.tg.screen(
        chat, mid, &text,
        c.s.image(Screen::Menu).as_deref(),
        Some(keyboard(vec![
            vec![Btn::Url("📤 Поделиться".into(), share)],
            vec![Btn::Data("← Назад".into(), "menu".into())],
        ])),
    )
    .await?;
    Ok(())
}

/// Найти партнёрскую запись клиента или завести её.
///
/// Возвращает `None`, если клиента нет: звать друзей от имени
/// несуществующего аккаунта нечему.
async fn ensure_partner(
    pool: &Pool,
    client_id: i64,
    percent: f64,
) -> Result<Option<(i64, String, f64)>> {
    sn_core::partners::ensure(pool, client_id, percent).await
}

/// Привязать пришедшего по ссылке к пригласившему.
///
/// Вызывается на `/start r_<slug>`. Привязка одноразовая: перебивать её
/// второй ссылкой значит отбирать начисления у того, кто привёл первым.
/// Себя пригласить нельзя — иначе это скидка, а не рекомендация.
pub async fn attach(pool: &Pool, client_id: i64, slug: &str) -> Result<bool> {
    let partner: Option<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, client_id FROM partners WHERE slug = $1 AND is_active",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    let Some((partner_id, owner)) = partner else {
        return Ok(false);
    };
    if owner == Some(client_id) {
        return Ok(false);
    }

    let done = sqlx::query(
        "UPDATE clients SET referred_by = $2
          WHERE id = $1 AND referred_by IS NULL AND deleted_at IS NULL",
    )
    .bind(client_id)
    .bind(partner_id)
    .execute(pool)
    .await?;

    Ok(done.rows_affected() > 0)
}

/// Минимальное кодирование для ссылки «поделиться».
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "%20".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn кодирование_ссылки() {
        assert_eq!(urlencode("https://t.me/bot?start=r_c1"),
                   "https%3A%2F%2Ft.me%2Fbot%3Fstart%3Dr_c1");
        assert_eq!(urlencode("два слова"), "%D0%B4%D0%B2%D0%B0%20%D1%81%D0%BB%D0%BE%D0%B2%D0%B0");
        // Незарезервированные символы остаются как есть — иначе ссылка
        // распухает втрое и в сообщении выглядит мусором.
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }
}

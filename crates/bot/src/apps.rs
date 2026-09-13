//! Подключение: какое приложение поставить и что в него вставить.
//!
//! Список приложений уже ведётся в панели для страницы подписки — берём
//! его же. Держать второй список для бота значило бы, что однажды он
//! разойдётся с первым, и клиент получит ссылку на приложение, которое
//! администратор уже убрал.
//!
//! Это самая частая причина обращений в поддержку: человек купил доступ
//! и не понимает, что делать с длинной ссылкой. Экран отвечает ровно на
//! этот вопрос — по одной платформе за раз, без списка из двадцати
//! приложений сразу.

use sn_core::Result;
use sqlx::Row;

use crate::settings::Screen;
use crate::tg::{keyboard, Btn};
use crate::Ctx;

/// Платформы в том порядке, в каком их выбирают чаще.
const PLATFORMS: [(&str, &str); 5] = [
    ("android", "🤖 Android"),
    ("ios", "🍏 iPhone / iPad"),
    ("windows", "🪟 Windows"),
    ("macos", "💻 macOS"),
    ("linux", "🐧 Linux"),
];

fn platform_title(code: &str) -> &str {
    PLATFORMS.iter().find(|(c, _)| *c == code).map(|(_, t)| *t).unwrap_or(code)
}

/// Выбор платформы.
pub async fn show_platforms(c: &Ctx<'_>, chat: i64, mid: Option<i64>) -> Result<()> {
    // Показываем только те платформы, для которых что-то заведено:
    // пустой экран «приложений нет» после выбора выглядит поломкой.
    let have: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT platform FROM subscription_page_apps WHERE is_active",
    )
    .fetch_all(c.pool)
    .await?;

    let rows: Vec<Vec<Btn>> = PLATFORMS
        .iter()
        .filter(|(code, _)| have.iter().any(|h| h == code))
        .map(|(code, title)| vec![Btn::Data((*title).into(), format!("app:{code}"))])
        .collect();

    let text = if rows.is_empty() {
        "<b>Подключение</b>\n\nСписок приложений пока не заполнен — напишите в поддержку, \
         и мы подскажем."
            .to_string()
    } else {
        "<b>Подключение</b>\n\nВыберите устройство — покажем, что поставить и куда вставить \
         вашу ссылку."
            .to_string()
    };

    let mut rows = rows;
    rows.push(vec![Btn::Data("← Назад".into(), "menu".into())]);

    c.tg.screen(chat, mid, &text, c.s.image(Screen::Sub).as_deref(), Some(keyboard(rows)))
        .await?;
    Ok(())
}

/// Приложения для выбранной платформы плюс готовая ссылка подписки.
pub async fn show_apps(
    c: &Ctx<'_>,
    chat: i64,
    platform: &str,
    sub_url: &str,
    mid: Option<i64>,
) -> Result<()> {
    let apps = sqlx::query(
        "SELECT name, deeplink, store_url, guide
           FROM subscription_page_apps
          WHERE is_active AND platform = $1
          ORDER BY sort_order, name",
    )
    .bind(platform)
    .fetch_all(c.pool)
    .await?;

    let mut text = format!("<b>{}</b>\n\n", platform_title(platform));
    let mut rows: Vec<Vec<Btn>> = Vec::new();

    for a in &apps {
        let name: String = a.get("name");
        let guide: Option<String> = a.get("guide");
        text.push_str(&format!("<b>{}</b>\n", esc(&name)));
        if let Some(g) = guide.as_deref().filter(|g| !g.trim().is_empty()) {
            text.push_str(&format!("{}\n", esc(g.trim())));
        }
        text.push('\n');

        // Две кнопки на приложение: поставить и сразу добавить подписку.
        //
        // Глубокая ссылка подставляет подписку в уже установленное
        // приложение. Собирается на месте: заранее её не сохранить,
        // потому что она зависит от конкретного клиента.
        let mut row = Vec::new();
        if let Some(store) = a.get::<Option<String>, _>("store_url").filter(|s| !s.is_empty()) {
            row.push(Btn::Url(format!("⬇️ {name}"), store));
        }
        if let Some(dl) = a.get::<Option<String>, _>("deeplink").filter(|s| !s.is_empty()) {
            row.push(Btn::Url("➕ Добавить".into(), dl.replace("{url}", sub_url)));
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }

    if apps.is_empty() {
        text.push_str("Для этой платформы приложений пока не заведено.");
    } else {
        text.push_str(
            "<b>Ваша ссылка</b> — нажмите, чтобы скопировать:\n",
        );
        // Ссылку даём отдельной строкой моноширинным: в Telegram по
        // нажатию она копируется целиком, и диктовать её не нужно.
        text.push_str(&format!("<code>{}</code>", esc(sub_url)));
    }

    rows.push(vec![Btn::Data("← К устройствам".into(), "apps".into())]);

    c.tg.screen(chat, mid, &text, c.s.image(Screen::Sub).as_deref(), Some(keyboard(rows)))
        .await?;
    Ok(())
}

/// Экранирование для parse_mode=HTML: названия и инструкции пишет человек.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn платформы_подписаны_по_человечески() {
        assert_eq!(platform_title("ios"), "🍏 iPhone / iPad");
        assert_eq!(platform_title("android"), "🤖 Android");
        // Незнакомую показываем как есть, а не прячем.
        assert_eq!(platform_title("freebsd"), "freebsd");
    }

    #[test]
    fn разметка_в_названиях_экранируется() {
        assert_eq!(esc("v2ray<N>"), "v2ray&lt;N&gt;");
    }
}

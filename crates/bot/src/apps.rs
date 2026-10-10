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

        // Telegram buttons use HTTPS; custom imports remain on the subscription page.
        let mut row = Vec::new();
        if let Some(store) = a.get::<Option<String>, _>("store_url").filter(|s| sn_core::bot_config::web_url(s,true)) {
            row.push(Btn::Url(format!("⬇️ {name}"), store));
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

    if !apps.is_empty() && sn_core::bot_config::web_url(sub_url,true) {
        rows.push(vec![Btn::Url("📖 Инструкция".into(),sub_url.to_owned())]);
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

    #[tokio::test]
    #[ignore = "Requires isolated SN_TEST_DATABASE_URL; Telegram is loopback capture only"]
    async fn sn_b06_all_platforms_use_https_instructions_and_keep_catalog() {
        use serde_json::json;
        let url=std::env::var("SN_TEST_DATABASE_URL").expect("isolated database required");
        let pool=sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        let name:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();assert!(name.starts_with("sn_audit_"));
        sqlx::query("CREATE TEMP TABLE subscription_page_apps (LIKE public.subscription_page_apps INCLUDING ALL)").execute(&pool).await.unwrap();
        for (platform,_) in PLATFORMS {
            for (name,store,active) in [("Fixture <Happ>",Some("https://store.example.test/app"),true),("No store",None,true),("Unsafe store",Some("happ://open"),true),("Hidden",Some("https://hidden.example.test"),false)] {
                sqlx::query("INSERT INTO subscription_page_apps(platform,name,store_url,deeplink,guide,is_active) VALUES($1,$2,$3,'happ://add/{{URL}}','Use <code> & connect',$4)").bind(platform).bind(name).bind(store).bind(active).execute(&pool).await.unwrap();
            }
        }
        let before:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT platform,name,deeplink FROM subscription_page_apps ORDER BY platform,name").fetch_all(&pool).await.unwrap();
        let (tg,server)=crate::tg::transport_tests::mock(vec![json!({"ok":true,"result":{"message_id":10}});7]).await;
        let reg=sn_payments::Registry::from_env();
        let cfg=sn_core::config::Config{database_url:url,api_bind:String::new(),sub_bind:String::new(),sub_public_url:"https://sub.example.test".into(),brand_name:"QA".into(),web_root:String::new(),sub_mode:"db".into(),panel_url:"https://panel.example.test".into(),sub_service_token:None};
        let c=Ctx{tg:&tg,pool:&pool,reg:&reg,cfg:&cfg,s:crate::settings::View::from_pairs(&[("bot.menu_layout",json!([{"id":"buy","enabled":false}]))])};
        let sub="https://sub.example.test/s/fixture?a=1&b=2";
        for (platform,_) in PLATFORMS {show_apps(&c,42,platform,sub,None).await.unwrap();}
        show_apps(&c,42,"freebsd",sub,None).await.unwrap();
        show_apps(&c,42,"ios","http://sub.example.test/s/fixture",None).await.unwrap();
        let requests=server.await.unwrap();assert_eq!(requests.len(),7);
        for (i,(_,body)) in requests.iter().enumerate() {
            let buttons:Vec<_>=body["reply_markup"]["inline_keyboard"].as_array().unwrap().iter().flat_map(|x|x.as_array().unwrap()).collect();
            let urls:Vec<_>=buttons.iter().filter_map(|x|x["url"].as_str()).collect();
            assert!(urls.iter().all(|x|sn_core::bot_config::web_url(x,true)),"Unsupported inline URL for request {i}: {urls:?}");
            assert!(buttons.iter().any(|x|x["callback_data"]=="apps"));
            if i<5 {
                assert_eq!(urls.iter().filter(|x|**x==sub).count(),1,"one HTTPS instruction action per screen");
                assert_eq!(urls.iter().filter(|x|**x=="https://store.example.test/app").count(),1);
                let text=body["text"].as_str().unwrap();assert!(text.contains("<code>https://sub.example.test/s/fixture?a=1&amp;b=2</code>"));assert!(text.contains("Fixture &lt;Happ&gt;"));assert!(!text.contains("Hidden"));
            }
            if i==5 {assert!(urls.is_empty());}
        }
        let after:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT platform,name,deeplink FROM subscription_page_apps ORDER BY platform,name").fetch_all(&pool).await.unwrap();assert_eq!(before,after);
        println!("SN-B06: seven local Telegram requests; five platforms, missing/unsafe store, hidden app, no shop, empty platform; catalog deeplinks unchanged");
    }
}

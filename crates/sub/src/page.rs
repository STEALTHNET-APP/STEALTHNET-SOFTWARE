//! Страница подписки для браузера.
//!
//! Её открывает обычный человек с телефона в руках, которому нужно за минуту
//! установить приложение и подключиться. Отсюда все решения: платформа
//! определяется сама, шаги пронумерованы, на каждом шаге ровно одно действие,
//! ссылка всегда продублирована QR-кодом.

use crate::SubData;
use crate::locale::Language;

// Localize static templates before inserting owner or customer content.
macro_rules! localized {
    ($lang:expr, $ru:literal, $en:literal $(, $($args:tt)*)?) => {
        match $lang {
            Language::Ru => format!($ru $(, $($args)*)?),
            Language::En => format!($en $(, $($args)*)?),
        }
    };
}

/// Приложение для подключения.
pub struct App {
    pub name: String,
    pub platform: String,
    /// Ссылка вида `happ://add/...` — открывает приложение и добавляет подписку.
    pub deeplink: Option<String>,
    pub store_url: Option<String>,
    pub guide: Option<String>,
    pub recommended: bool,
    /// Своя иконка от администратора. `icon_svg` инлайнится, `icon_url`
    /// подставляется тегом <img> — но это внешний запрос, а страницу
    /// открывают из-под блокировок, поэтому SVG предпочтительнее.
    pub icon_svg: Option<String>,
    pub icon_url: Option<String>,
}

/// Настройки страницы, задаваемые администратором.
#[derive(Default, Clone)]
pub struct PageSettings {
    pub branding: sn_core::customer_brand::CustomerBrand,
    pub title: Option<String>,
    /// Ссылка на поддержку: чат, бот или сайт — что впишут в настройках.
    pub support_url: Option<String>,
    pub support_text: Option<String>,
    /// Отдельная ссылка на бота, если поддержка и бот — разные адреса.
    pub bot_url: Option<String>,
    pub footer_note: Option<String>,
}

/// Иконки приложений — инлайновые SVG.
///
/// Внешних картинок нет намеренно: страницу открывают из-под блокировок,
/// и любой запрос к чужому домену может не пройти.
fn app_icon(name: &str) -> &'static str {
    let n = name.to_lowercase();
    // Порядок проверок важен: «v2rayng» содержит «v2ray», а «flclash» — «clash».
    if n.contains("happ") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#2563EB"/><path d="M16 33V15h4.4v6.8h7.2V15H32v18h-4.4v-7.1h-7.2V33H16z" fill="#fff"/></svg>"##
    } else if n.contains("streisand") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#7C3AED"/><path d="M24 12l10 4.6v8.9c0 6-4.1 10.4-10 12.5-5.9-2.1-10-6.5-10-12.5v-8.9L24 12z" stroke="#fff" stroke-width="2.6" fill="none"/><path d="M20 24.5l3 3 5.5-5.5" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" fill="none"/></svg>"##
    } else if n.contains("v2rayng") || n.contains("v2rayn") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#1565C0"/><path d="M12 15l12 19 12-19" stroke="#fff" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/><circle cx="24" cy="12" r="2.6" fill="#fff"/></svg>"##
    } else if n.contains("v2box") || n.contains("v2ray") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#0D9488"/><path d="M13 16l11 17 11-17" stroke="#fff" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/></svg>"##
    } else if n.contains("hiddify") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#0891B2"/><circle cx="24" cy="24" r="7.5" stroke="#fff" stroke-width="2.8" fill="none"/><path d="M24 7v6.5M24 34.5V41M7 24h6.5M34.5 24H41" stroke="#fff" stroke-width="2.8" stroke-linecap="round"/></svg>"##
    } else if n.contains("flclash") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#C2410C"/><path d="M27 9L13.5 27h8.2L20 39l14.5-18h-8.2L27 9z" fill="#fff"/></svg>"##
    } else if n.contains("clash") || n.contains("mihomo") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#EA580C"/><path d="M27 9L13.5 27h8.2L20 39l14.5-18h-8.2L27 9z" fill="#fff"/></svg>"##
    } else if n.contains("stash") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#0F766E"/><path d="M11 18h26M11 24h26M11 30h26" stroke="#fff" stroke-width="3" stroke-linecap="round"/></svg>"##
    } else if n.contains("shadowrocket") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#475569"/><path d="M24 9c5.2 4.3 8.2 9.6 8.2 15.6 0 4.2-1.9 7.4-4.2 9.4l-4-4.2-4 4.2c-2.3-2-4.2-5.2-4.2-9.4C15.8 18.6 18.8 13.3 24 9z" fill="#fff"/></svg>"##
    } else if n.contains("nekobox") || n.contains("nekoray") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#DB2777"/><path d="M14 20l-1-7 6 3.5a16 16 0 0 1 10 0L35 13l-1 7a13 13 0 1 1-20 0z" fill="#fff"/><circle cx="20" cy="27" r="1.9" fill="#DB2777"/><circle cx="28" cy="27" r="1.9" fill="#DB2777"/></svg>"##
    } else if n.contains("karing") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#4338CA"/><circle cx="24" cy="24" r="10" stroke="#fff" stroke-width="3" fill="none"/><circle cx="24" cy="24" r="3.4" fill="#fff"/></svg>"##
    } else if n.contains("sing-box") || n.contains("singbox") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#7C2D12"/><path d="M24 11l10.4 6v12L24 35l-10.4-6V17L24 11z" stroke="#fff" stroke-width="2.8" fill="none"/><path d="M24 23l10.4-6M24 23v12M24 23L13.6 17" stroke="#fff" stroke-width="2" opacity=".8"/></svg>"##
    } else if n.contains("throne") || n.contains("husi") {
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#166534"/><path d="M14 34V19l5 5 5-9 5 9 5-5v15H14z" fill="#fff"/></svg>"##
    } else {
        // Незнакомое приложение: нейтральная иконка «загрузить».
        r##"<svg viewBox="0 0 48 48"><rect width="48" height="48" rx="13" fill="#334155"/><path d="M24 13v15M17.5 21.5L24 15l6.5 6.5M15 33h18" stroke="#fff" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round" fill="none"/></svg>"##
    }
}

/// Иконка приложения: своя от администратора либо встроенная.
fn render_icon(a: &App) -> String {
    if let Some(svg) = a
        .icon_svg
        .as_deref()
        .filter(|s| s.trim_start().starts_with("<svg"))
    {
        // SVG вставляем как есть — он приходит от администратора, не от клиента.
        return svg.to_string();
    }
    if let Some(url) = a.icon_url.as_deref().filter(|u| !u.is_empty()) {
        return format!(
            r#"<img src="{}" alt="" loading="lazy" width="46" height="46">"#,
            esc(url)
        );
    }
    app_icon(&a.name).to_string()
}

fn platform_title(p: &str, lang: Language) -> &'static str {
    match p {
        "ios" => "iPhone · iPad",
        "android" => "Android",
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        _ => lang.text("Другое", "Other"),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Срок словами: «через 6 месяцев» понятнее, чем «через 183 дня».
fn humanize_days(days: i64, lang: Language) -> String {
    if lang == Language::En {
        return match days {
            ..=-1 => "expired".into(),
            0 => "expires today".into(),
            1 => "1 day left".into(),
            2..=29 => format!("{days} days left"),
            _ => { let months = days / 30; format!("~{months} {} left", if months == 1 { "month" } else { "months" }) }
        };
    }
    if days < 0 {
        return "истекла".into();
    }
    if days == 0 {
        return "истекает сегодня".into();
    }
    if days == 1 {
        return "остался 1 день".into();
    }
    if days < 30 {
        let last = days % 10;
        let tens = days % 100;
        let word = if (11..=14).contains(&tens) || last == 0 || last >= 5 {
            "дней"
        } else if last == 1 {
            "день"
        } else {
            "дня"
        };
        return format!("осталось {days} {word}");
    }
    let months = days / 30;
    let last = months % 10;
    let tens = months % 100;
    let word = if (11..=14).contains(&tens) || last == 0 || last >= 5 {
        "месяцев"
    } else if last == 1 {
        "месяц"
    } else {
        "месяца"
    };
    format!("осталось ~{months} {word}")
}

fn format_bytes(bytes: i64, lang: Language) -> String {
    let value = sn_core::money::format_bytes(bytes);
    if lang == Language::Ru { return value; }
    let (number, unit) = value.rsplit_once(' ').unwrap_or((&value, ""));
    let unit = match unit { "Б" => "B", "КБ" => "KB", "МБ" => "MB", "ГБ" => "GB", "ТБ" => "TB", _ => unit };
    format!("{number} {unit}")
}

/// QR-код ссылки подписки как inline-SVG.
///
/// Рисуем на сервере: библиотека в браузере — это внешний запрос,
/// который под блокировками может не загрузиться.
/// Тот же QR, но доступный снаружи модуля: его отдаёт отдельный
/// эндпоинт для панели.
pub fn qr_svg_public(data: &str) -> String {
    qr_svg(data)
}

fn qr_svg(data: &str) -> String {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};

    // Средний уровень коррекции: код читается даже при бликах на экране
    // и при этом не разрастается в мелкую сетку.
    match QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .dark_color(svg::Color("#0A0D12"))
            .light_color(svg::Color("#FFFFFF"))
            .build(),
        Err(_) => String::new(),
    }
}

// Brand hues remain configurable; the button's white label needs contrast even
// when the owner chooses a very pale color. Other brand surfaces retain the hue.
fn readable_button_color(hex:&str)->String {
    let channels=[1,3,5].map(|i|u8::from_str_radix(&hex[i..i+2],16).unwrap_or(0) as f64);
    for step in 0..=100 {
        let rgb=channels.map(|c|(c*(1.-step as f64/100.)).floor() as u8);
        if white_contrast(rgb)>=4.6 {return format!("#{:02x}{:02x}{:02x}",rgb[0],rgb[1],rgb[2]);}
    }
    "#000000".into()
}
fn white_contrast(rgb:[u8;3])->f64 {
    let linear=rgb.map(|c|{let c=c as f64/255.;if c<=0.04045 {c/12.92}else{((c+0.055)/1.055).powf(2.4)}});
    1.05/(0.2126*linear[0]+0.7152*linear[1]+0.0722*linear[2]+0.05)
}

/// Compact connection surface. Details live in focused sheets; the primary task stays in one viewport.
#[cfg(test)]
pub fn render(d: &SubData, brand: &str, sub_url: &str, apps: &[App], settings: &PageSettings, notice: &str) -> String {
    render_localized(d, brand, sub_url, apps, settings, notice, Language::Ru)
}

pub fn render_localized(
    d: &SubData,
    brand: &str,
    sub_url: &str,
    apps: &[App],
    settings: &PageSettings,
    notice: &str,
    lang: Language,
) -> String {
    let identity=&settings.branding;
    let brand=identity.brand.as_deref().unwrap_or(brand);
    let brand_html=if let Some(logo)=identity.logo.as_deref(){format!(r#"<img class="brand-light" src="{}" alt="{}">{}"#,esc(logo),esc(brand),identity.logo_dark.as_deref().map(|u|format!(r#"<img class="brand-dark" src="{}" alt="{}">"#,esc(u),esc(brand))).unwrap_or_default())}else{format!("<b>{}</b>",esc(brand))};
    let mut brand_style=format!("{}{}",identity.accent.as_deref().map(|c|format!("--a1:{c};")).unwrap_or_default(),identity.accent_end.as_deref().map(|c|format!("--a2:{c};")).unwrap_or_default());
    if let Some(start)=identity.accent.as_deref().or(identity.accent_end.as_deref()) {
        let end=identity.accent_end.as_deref().unwrap_or(start);
        brand_style.push_str(&format!("--primary-start:{};--primary-end:{};--on-primary:#fff;",readable_button_color(start),readable_button_color(end)));
    }
    let favicon=identity.favicon.as_deref().map(|url|format!(r#"<link rel="icon" href="{}">"#,esc(url))).unwrap_or_default();
    let active = d.status == "active";
    let ready = active && !d.hosts.is_empty();
    let used = format_bytes(d.used, lang);
    let limit = d
        .limit
        .map(|bytes| format_bytes(bytes, lang))
        .unwrap_or_else(|| lang.text("Безлимит", "Unlimited").into());
    let percent = d
        .limit
        .filter(|l| *l > 0)
        .map(|l| (d.used as f64 / l as f64 * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    let expires = d
        .expires_at
        .map(|e| e.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|| lang.text("Без срока", "No expiry").into());
    let days = d
        .expires_at
        .map(|e| humanize_days((e - chrono::Utc::now()).num_days(), lang))
        .unwrap_or_else(|| lang.text("Без ограничения срока", "No expiry date").into());
    let devices = if d.device_limit > 0 {
        d.device_limit.to_string()
    } else {
        lang.text("Безлимит", "Unlimited").into()
    };
    let devices_summary = if d.device_limit > 0 {
        devices.clone()
    } else {
        lang.text(r#"<span role="img" aria-label="Безлимит" title="Без ограничения устройств">∞</span>"#, r#"<span role="img" aria-label="Unlimited" title="Unlimited devices">∞</span>"#).into()
    };
    let (state, heading, explanation) = match d.status.as_str() {
        "active" if ready => (
            lang.text("Доступ активен", "Access active"),
            lang.text("Подключим ваше устройство", "Connect your device"),
            lang.text("Установите приложение и добавьте подписку — всё готово для подключения.", "Install an app and add your subscription to connect."),
        ),
        "active" => (
            lang.text("Доступ активен", "Access active"),
            lang.text("Локации готовятся", "Locations are being prepared"),
            lang.text("В подписке пока нет доступных локаций.", "Your subscription has no available locations yet."),
        ),
        "limited" => (
            lang.text("Лимит трафика", "Data limit"),
            lang.text("Трафик закончился", "Data allowance used up"),
            lang.text("Подключение появится после сброса лимита или продления.", "You can connect again when your data allowance resets or you renew."),
        ),
        "disabled" => (
            lang.text("Доступ отключён", "Access disabled"),
            lang.text("Подписка отключена", "Subscription disabled"),
            lang.text("Свяжитесь с поддержкой, чтобы уточнить причину.", "Contact support to find out why."),
        ),
        _ => (
            lang.text("Срок истёк", "Expired"),
            lang.text("Подписка завершилась", "Subscription expired"),
            lang.text("После продления здесь снова появится подключение.", "Renew your subscription to connect again."),
        ),
    };
    let support = settings
        .support_url
        .as_deref()
        .filter(|v| !v.is_empty())
        .or(settings.bot_url.as_deref().filter(|v| !v.is_empty()));
    let support_action = support.map(|u| localized!(lang, r#"<a class="button primary" href="{}" target="_blank" rel="noopener">{}Написать в поддержку</a>"#, r#"<a class="button primary" href="{}" target="_blank" rel="noopener">{}Contact support</a>"#,esc(u),icon_send())).unwrap_or_else(|| lang.text("<p class=\"muted\">Обратитесь к администратору вашего VPN-сервиса.</p>", "<p class=\"muted\">Contact your VPN service administrator.</p>").into());
    let support_footer = support
        .map(|u| {
            format!(
                r#"<a href="{}" target="_blank" rel="noopener">{}</a>"#,
                esc(u),
                esc(lang.default_text(settings
                    .support_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .unwrap_or("Написать в поддержку")))
            )
        })
        .unwrap_or_else(|| lang.text("<span>Настройка безопасного подключения</span>", "<span>Set up a secure connection</span>").into());
    let initial = apps.iter().position(|a| a.recommended).unwrap_or(0);
    let platform_options = ["ios", "android", "windows", "macos", "linux"]
        .iter()
        .filter(|p| apps.iter().any(|a| a.platform == **p))
        .map(|p| {
            format!(
                r#"<option value="{}"{}>{}</option>"#,
                p,
                if apps.get(initial).is_some_and(|a| a.platform == *p) {
                    " selected"
                } else {
                    ""
                },
                platform_title(p, lang)
            )
        })
        .collect::<String>();
    let choices = apps.iter().enumerate().map(|(i,a)|format!(r#"<button type="button" class="app-choice" data-choice="{i}" data-platform="{platform}" aria-pressed="false"><span class="app-icon">{icon}</span><span class="app-label"><b>{name}</b><span>{hint}</span></span><span class="choice-check" aria-hidden="true">{check}</span></button>"#,platform=esc(&a.platform),icon=render_icon(a),name=esc(&a.name),hint=if a.recommended {lang.text("Рекомендуем", "Recommended")} else {lang.text("Выбрать приложение", "Choose an app")},check=icon_check())).collect::<String>();
    let application_views = apps.iter().enumerate().map(|(i,a)|format!(r#"<div class="application" data-app="{i}" data-platform="{platform}" data-recommended="{recommended}" {hidden}>{content}</div>"#,platform=esc(&a.platform),recommended=a.recommended,hidden=if i==initial {""}else{"hidden"},content=render_app(a,sub_url,lang))).collect::<String>();
    let app_picker = apps.get(initial).map(|a| localized!(lang, r#"<button class="app-picker" id="appPicker" type="button" data-sheet="appsSheet" aria-haspopup="dialog" aria-label="Выбрать приложение"><span class="app-icon">{}</span><span class="app-label"><b>{}</b><span>Выбрать другое приложение</span></span>{}</button>"#, r#"<button class="app-picker" id="appPicker" type="button" data-sheet="appsSheet" aria-haspopup="dialog" aria-label="Choose an app"><span class="app-icon">{}</span><span class="app-label"><b>{}</b><span>Choose another app</span></span>{}</button>"#,render_icon(a),esc(&a.name),icon_chevron())).unwrap_or_default();
    let connect = if ready && !apps.is_empty() {
        localized!(lang, r#"<div class="device-row"><label for="deviceSelect">Ваше устройство</label><select id="deviceSelect">{platform_options}</select></div>{app_picker}<div id="applicationViews">{application_views}</div>"#, r#"<div class="device-row"><label for="deviceSelect">Your device</label><select id="deviceSelect">{platform_options}</select></div>{app_picker}<div id="applicationViews">{application_views}</div>"#
        )
    } else if ready {
        localized!(lang, r#"<div class="manual-state">{}<h2>Добавьте подписку вручную</h2><p>Скопируйте ссылку и импортируйте её в своё VPN-приложение.</p><button class="button primary" type="button" data-copy>Скопировать ссылку</button><button class="text-button" type="button" data-sheet="helpSheet">Как это сделать</button></div>"#, r#"<div class="manual-state">{}<h2>Add your subscription manually</h2><p>Copy the link and import it into your VPN app.</p><button class="button primary" type="button" data-copy>Copy link</button><button class="text-button" type="button" data-sheet="helpSheet">How to do this</button></div>"#,
            icon_device()
        )
    } else {
        localized!(lang, r#"<div class="access-state">{}<h2>{}</h2><p>{}</p>{support_action}<button class="text-button" type="button" data-sheet="accountSheet">Подробнее о подписке</button></div>"#, r#"<div class="access-state">{}<h2>{}</h2><p>{}</p>{support_action}<button class="text-button" type="button" data-sheet="accountSheet">Subscription details</button></div>"#,
            icon_lock(),
            if active {
                lang.text("Нужна помощь с локациями", "Need help with locations?")
            } else {
                lang.text("Восстановим доступ", "Restore access")
            },
            if active {
                lang.text("Поддержка проверит назначенные вам серверы.", "Support can check the servers assigned to you.")
            } else {
                lang.text("Управление подпиской доступно через ваш сервис.", "Manage your subscription through your VPN service.")
            }
        )
    };
    let link_sheet = if active {
        sheet(lang,
            "linkSheet",
            lang.text("Ссылка и QR-код", "Link and QR code"),
            &localized!(lang, r#"<p class="sheet-lead">Откройте эту страницу на другом устройстве или импортируйте ссылку вручную.</p><div class="qr-box" role="img" aria-label="QR-код вашей подписки">{}</div><label class="copy-label" for="subscriptionLink">Личная ссылка подписки</label><textarea id="subscriptionLink" readonly rows="2" spellcheck="false">{}</textarea><button type="button" class="button primary" data-copy>Скопировать ссылку</button><p class="privacy-note">Не публикуйте ссылку и QR-код: они дают доступ к вашей подписке.</p>"#, r#"<p class="sheet-lead">Open this page on another device or import the link manually.</p><div class="qr-box" role="img" aria-label="Your subscription QR code">{}</div><label class="copy-label" for="subscriptionLink">Your private subscription link</label><textarea id="subscriptionLink" readonly rows="2" spellcheck="false">{}</textarea><button type="button" class="button primary" data-copy>Copy link</button><p class="privacy-note">Keep your link and QR code private: they give access to your subscription.</p>"#,
                qr_svg(sub_url),
                esc(sub_url)
            ),
        )
    } else {
        String::new()
    };
    let server_sheet = if active {
        sheet(lang, "serversSheet", lang.text("Ваши локации", "Your locations"), &render_servers(d, lang))
    } else {
        String::new()
    };
    let extra_tools = if active {
        localized!(lang, r#"<button class="quick-action" type="button" data-sheet="serversSheet">{}<span>Локации <b>{}</b></span></button><button class="quick-action" type="button" data-sheet="linkSheet">{}<span>Ссылка и QR</span></button>"#, r#"<button class="quick-action" type="button" data-sheet="serversSheet">{}<span>Locations <b>{}</b></span></button><button class="quick-action" type="button" data-sheet="linkSheet">{}<span>Link and QR</span></button>"#,
            icon_servers(),
            d.hosts.len(),
            icon_qr()
        )
    } else {
        localized!(lang, r#"<button class="quick-action" type="button" data-sheet="helpSheet">{}<span>Помощь</span></button>"#, r#"<button class="quick-action" type="button" data-sheet="helpSheet">{}<span>Help</span></button>"#,
            icon_info()
        )
    };
    let notice_button = if notice.is_empty() {
        String::new()
    } else {
        localized!(lang, r#"<button class="service-note" type="button" data-sheet="noticeSheet">{}<span>Сообщение сервиса</span>{}</button>"#, r#"<button class="service-note" type="button" data-sheet="noticeSheet">{}<span>Service message</span>{}</button>"#,
            icon_alert(),
            icon_arrow()
        )
    };
    let notice_sheet = if notice.is_empty() {
        String::new()
    } else {
        sheet(lang,
            "noticeSheet",
            lang.text("Сообщение сервиса", "Service message"),
            &format!("<p class=\"prose\">{}</p>", esc(notice)),
        )
    };
    let account = sheet(lang,
        "accountSheet",
        lang.text("Ваша подписка", "Your subscription"),
        &localized!(lang, r#"<p class="account-user">{user}</p><span class="status {tone}">{state}</span><dl class="account-facts"><dt>Тариф</dt><dd>{tariff}</dd><dt>Действует до</dt><dd>{expires}</dd><dt>Остаток срока</dt><dd>{days}</dd><dt>Трафик использован</dt><dd>{used}</dd><dt>Лимит трафика</dt><dd>{limit}</dd><dt>Лимит устройств</dt><dd>{devices}</dd></dl>{traffic}<p class="prose">{footer}</p>"#, r#"<p class="account-user">{user}</p><span class="status {tone}">{state}</span><dl class="account-facts"><dt>Plan</dt><dd>{tariff}</dd><dt>Valid until</dt><dd>{expires}</dd><dt>Time remaining</dt><dd>{days}</dd><dt>Data used</dt><dd>{used}</dd><dt>Data limit</dt><dd>{limit}</dd><dt>Device limit</dt><dd>{devices}</dd></dl>{traffic}<p class="prose">{footer}</p>"#,
            user = esc(&d.username),
            tone = if active { "ok" } else { "attention" },
            tariff = esc(d.tariff.as_deref().unwrap_or(lang.text("Не указан", "Not specified"))),
            days = esc(&days),
            footer = esc(settings.footer_note.as_deref().unwrap_or("")),
            traffic = if d.limit.is_some() {
                localized!(lang, r#"<div class="traffic-label">Использовано {percent:.0}% трафика</div><div class="traffic-bar"><i style="width:{percent:.1}%"></i></div>"#, r#"<div class="traffic-label">{percent:.0}% of data used</div><div class="traffic-bar"><i style="width:{percent:.1}%"></i></div>"#
                )
            } else {
                String::new()
            }
        ),
    );
    let manual_install=lang.text("Установите совместимое VPN-приложение из официального источника. Если не знаете, какое выбрать, обратитесь в поддержку.", "Install a compatible VPN app from its official source. Ask support if you need help choosing one.");
    let manual_import=lang.text("Нажмите «Скопировать ссылку» на главном экране, откройте импорт из буфера обмена в VPN-приложении и подтвердите добавление профиля.", "Tap “Copy link” on the main screen, import from the clipboard in your VPN app, and confirm adding the profile.");
    let steps=if ready {localized!(lang, r#"<ol class="guide-steps"><li><b>Установите приложение</b><p data-help-install>{manual_install}</p></li><li><b>Добавьте подписку</b><p data-help-import>{manual_import}</p></li><li><b>Включите VPN в приложении</b><p>Выберите локацию, нажмите кнопку подключения и разрешите создание VPN-подключения, если система попросит.</p></li></ol>"#, r#"<ol class="guide-steps"><li><b>Install the app</b><p data-help-install>{manual_install}</p></li><li><b>Add your subscription</b><p data-help-import>{manual_import}</p></li><li><b>Turn on VPN in the app</b><p>Choose a location, tap connect, and allow the VPN connection if your device asks.</p></li></ol>"#)}else{format!(r#"<p class="sheet-lead">{}</p>"#,esc(explanation))};
    let help = sheet(lang,
        "helpSheet",
        lang.text("Помощь с подключением", "Connection help"),
        &localized!(lang, r#"{steps}<div class="faq"><details><summary>Открыли страницу внутри Telegram?</summary><p>Если кнопка не открывает приложение, выберите «Открыть в браузере» в меню Telegram и повторите.</p></details><details><summary>Локации не подключаются</summary><p>Обновите приложение и подписку, проверьте срок и трафик. Попробуйте другую локацию или сеть. Передайте поддержке название приложения, локацию и текст ошибки.</p></details><details><summary>Можно поделиться ссылкой?</summary><p>Ссылка даёт доступ к вашей подписке. Чужие устройства могут занять лимит и расходовать трафик. Для другого человека нужна отдельная подписка.</p></details></div>{support_action}"#, r#"{steps}<div class="faq"><details><summary>Opened this page inside Telegram?</summary><p>If the button does not open the app, choose “Open in browser” from the Telegram menu and try again.</p></details><details><summary>Locations will not connect</summary><p>Update the app and subscription, and check your expiry date and data allowance. Try another location or network. Tell support the app name, location, and error message.</p></details><details><summary>Can I share my link?</summary><p>The link gives access to your subscription. Other devices can use your device slots and data allowance. Each person needs their own subscription.</p></details></div>{support_action}"#
        ),
    );
    let no_script = if active {
        localized!(lang, r#"<noscript><p>Для выбора приложений включите JavaScript. <a href="{}">Открыть конфигурацию подписки</a>. Ссылку можно скопировать из адресной строки.</p></noscript>"#, r#"<noscript><p>Enable JavaScript to choose an app. <a href="{}">Open subscription configuration</a>. You can copy the link from your address bar.</p></noscript>"#,
            esc(sub_url)
        )
    } else {
        String::new()
    };
    localized!(lang, r##"<!doctype html>
<html lang="{language}" style="{brand_style}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="color-scheme" content="light dark"><meta name="robots" content="noindex,nofollow"><title>{brand} — {title}</title>{favicon}<link rel="preload" href="/fonts/roboto.ttf" as="font" type="font/ttf" crossorigin><script>(function(){{var mode='auto';try{{mode=localStorage.getItem('sn.app.theme')||'auto';}}catch(e){{}}document.documentElement.dataset.theme=(mode==='light'||mode==='dark')?mode:matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light';}})();</script><style>{css}</style></head><body>
<!-- THESIS: One customer identity through the connection journey.
OWN-WORLD: Configured logo, Roboto, softly raised rounded white or dark cards and brand-gradient actions.
STORY: Confirm access, choose device and app, install, import, then turn VPN on in the app.
FIRST VIEWPORT: Paired desktop summary and actions; mobile reading order; details in focused sheets.
FORM: Extend the customer cabinet style approved by the user.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance. -->
<div class="viewport-shell">
<header class="page-header"><a class="brand" href="#" aria-label="{brand} — страница подключения">{brand_html}</a><div class="header-actions"><button class="language-button" type="button" id="languageSwitch" aria-label="{language_label}">{language_switch}</button><button class="theme-button" type="button" data-sheet="themeSheet" aria-label="Изменить тему">{theme_icon}</button><button class="help-button" type="button" data-sheet="helpSheet" aria-label="Помощь с подключением">{help_icon}<span>Помощь</span></button></div></header>
<main class="connection-shell">
<section class="account-panel" aria-labelledby="connectionTitle"><div class="account-top"><span class="status {tone}"><i aria-hidden="true"></i>{state}</span><span class="username" title="{user}">{user}</span></div><h1 id="connectionTitle">{heading}</h1><p class="intro-copy">{explanation}</p><div class="summary-facts"><div><span>Доступ до</span><b>{expires}</b></div><div><span>Трафик</span><b>{used}<small> / {limit}</small></b></div><div><span>Лимит устройств</span><b>{devices_summary}</b></div></div>{notice_button}</section>
<section class="connection-panel" aria-label="Подключение устройства">{connect}</section>
<nav class="quick-actions" aria-label="Детали подключения"><button class="quick-action" type="button" data-sheet="accountSheet">{account_icon}<span>Подписка</span></button>{extra_tools}</nav>
</main>
<footer class="page-footer">{support_footer}</footer>{no_script}
</div>
{account}{link_sheet}{server_sheet}{help}{notice_sheet}{apps_sheet}{theme_sheet}
<div class="feedback" id="feedback" role="status" aria-live="polite" hidden></div>
<script>{js}</script></body></html>"##, r##"<!doctype html>
<html lang="{language}" style="{brand_style}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="color-scheme" content="light dark"><meta name="robots" content="noindex,nofollow"><title>{brand} — {title}</title>{favicon}<link rel="preload" href="/fonts/roboto.ttf" as="font" type="font/ttf" crossorigin><script>(function(){{var mode='auto';try{{mode=localStorage.getItem('sn.app.theme')||'auto';}}catch(e){{}}document.documentElement.dataset.theme=(mode==='light'||mode==='dark')?mode:matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light';}})();</script><style>{css}</style></head><body>
<!-- THESIS: One customer identity through the connection journey.
OWN-WORLD: Configured logo, Roboto, softly raised rounded white or dark cards and brand-gradient actions.
STORY: Confirm access, choose device and app, install, import, then turn VPN on in the app.
FIRST VIEWPORT: Paired desktop summary and actions; mobile reading order; details in focused sheets.
FORM: Extend the customer cabinet style approved by the user.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance. -->
<div class="viewport-shell">
<header class="page-header"><a class="brand" href="#" aria-label="{brand} — connection page">{brand_html}</a><div class="header-actions"><button class="language-button" type="button" id="languageSwitch" aria-label="{language_label}">{language_switch}</button><button class="theme-button" type="button" data-sheet="themeSheet" aria-label="Change theme">{theme_icon}</button><button class="help-button" type="button" data-sheet="helpSheet" aria-label="Connection help">{help_icon}<span>Help</span></button></div></header>
<main class="connection-shell">
<section class="account-panel" aria-labelledby="connectionTitle"><div class="account-top"><span class="status {tone}"><i aria-hidden="true"></i>{state}</span><span class="username" title="{user}">{user}</span></div><h1 id="connectionTitle">{heading}</h1><p class="intro-copy">{explanation}</p><div class="summary-facts"><div><span>Valid until</span><b>{expires}</b></div><div><span>Data</span><b>{used}<small> / {limit}</small></b></div><div><span>Device limit</span><b>{devices_summary}</b></div></div>{notice_button}</section>
<section class="connection-panel" aria-label="Connect a device">{connect}</section>
<nav class="quick-actions" aria-label="Connection details"><button class="quick-action" type="button" data-sheet="accountSheet">{account_icon}<span>Subscription</span></button>{extra_tools}</nav>
</main>
<footer class="page-footer">{support_footer}</footer>{no_script}
</div>
{account}{link_sheet}{server_sheet}{help}{notice_sheet}{apps_sheet}{theme_sheet}
<div class="feedback" id="feedback" role="status" aria-live="polite" hidden></div>
<script>{js}</script></body></html>"##,
        language = lang.code(),
        language_label = if lang == Language::Ru { "Switch to English" } else { "Переключить на русский" },
        language_switch = if lang == Language::Ru { "EN" } else { "RU" },
        brand = esc(brand),
        title = esc(lang.default_text(settings.title.as_deref().unwrap_or("Подключение"))),
        css = include_str!("connect.css"),
        js = include_str!("connect.js"),
        theme_icon = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M20.7 13.2A8.8 8.8 0 0 1 10.8 3.3a9 9 0 1 0 9.9 9.9Z"/></svg>"#,
        theme_sheet = sheet(lang, "themeSheet",lang.text("Оформление", "Appearance"),&localized!(lang, r#"<p class="sheet-lead">Выберите тему или используйте оформление устройства.</p><fieldset class="theme-options"><legend class="visually-hidden">Тема страницы</legend>{}</fieldset>"#, r#"<p class="sheet-lead">Choose a theme or follow your device settings.</p><fieldset class="theme-options"><legend class="visually-hidden">Page theme</legend>{}</fieldset>"#,[ ("auto",lang.text("Автоматически", "Automatic"),lang.text("Как на вашем устройстве", "Follow your device settings")),("light",lang.text("Светлая", "Light"),lang.text("Светлые карточки и мягкие тени", "Light cards and soft shadows")),("dark",lang.text("Тёмная", "Dark"),lang.text("Мягкий контраст на тёмном фоне", "Soft contrast on a dark background")) ].iter().map(|(id,name,hint)|format!(r#"<label class="theme-option"><input type="radio" name="theme" value="{id}"><span><b>{name}</b><small>{hint}</small></span>{}</label>"#,icon_check())).collect::<String>())),
        help_icon = icon_info(),
        account_icon = icon_device(),
        tone = if active { "ok" } else { "attention" },
        user = esc(&d.username),
        apps_sheet = if ready && !apps.is_empty() {
            sheet(lang,
                "appsSheet",
                lang.text("Выберите приложение", "Choose an app"),
                &localized!(lang, r#"<p class="sheet-lead">Для <span id="appsPlatform">вашего устройства</span>. Выберите одно приложение, которое будете использовать.</p><div class="app-choices">{choices}</div>"#, r#"<p class="sheet-lead">Choose one app for <span id="appsPlatform">your device</span>.</p><div class="app-choices">{choices}</div>"#
                ),
            )
        } else {
            String::new()
        },
    )
}

fn sheet(lang: Language, id: &str, title: &str, content: &str) -> String {
    localized!(lang, r#"<dialog class="sheet" id="{id}" aria-labelledby="{id}Title"><header class="sheet-header"><h2 id="{id}Title">{title}</h2><button class="close-button" type="button" data-close aria-label="Закрыть окно" autofocus><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m6 6 12 12M18 6 6 18"/></svg></button></header><div class="sheet-body">{content}</div></dialog>"#, r#"<dialog class="sheet" id="{id}" aria-labelledby="{id}Title"><header class="sheet-header"><h2 id="{id}Title">{title}</h2><button class="close-button" type="button" data-close aria-label="Close dialog" autofocus><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m6 6 12 12M18 6 6 18"/></svg></button></header><div class="sheet-body">{content}</div></dialog>"#
    )
}

fn render_servers(d: &SubData, lang: Language) -> String {
    if d.status != "active" {
        return String::new();
    }
    let rows = d.hosts.iter().map(|h|localized!(lang, r#"<details class="server"><summary><span><b>{}</b><small>{} · {}</small></span>{}</summary><dl><dt>Адрес</dt><dd>{}:{}</dd><dt>Протокол</dt><dd>{}</dd><dt>Транспорт</dt><dd>{}</dd><dt>Защита</dt><dd>{}</dd></dl></details>"#, r#"<details class="server"><summary><span><b>{}</b><small>{} · {}</small></span>{}</summary><dl><dt>Address</dt><dd>{}:{}</dd><dt>Protocol</dt><dd>{}</dd><dt>Transport</dt><dd>{}</dd><dt>Security</dt><dd>{}</dd></dl></details>"#,esc(&h.remark),esc(&h.protocol.to_uppercase()),esc(&h.security.to_uppercase()),icon_chevron(),esc(&h.address),h.port,esc(&h.protocol),esc(&h.network),esc(&h.security))).collect::<String>();
    localized!(lang, r#"<p class="sheet-lead">Выбирайте локацию внутри VPN-приложения. Задержку можно измерить там же — она зависит от вашей сети.</p><div class="server-list">{}</div>"#, r#"<p class="sheet-lead">Choose a location in your VPN app. You can also check latency there; it depends on your network.</p><div class="server-list">{}</div>"#,
        if rows.is_empty() {
            lang.text("<p>Доступных локаций пока нет. Обратитесь в поддержку.</p>", "<p>No locations are available yet. Contact support.</p>").into()
        } else {
            rows
        }
    )
}

fn render_app(a: &App, sub_url: &str, lang: Language) -> String {
    let deeplink = a.deeplink.as_ref().filter(|v| !v.is_empty()).map(|dl| {
        dl.replace("{{URL}}", &urlencode(sub_url))
            .replace("{URL}", &urlencode(sub_url))
            .replace("{url}", &urlencode(sub_url))
    });
    let hint = if deeplink.is_some() {
        lang.text("Затем включите VPN в приложении.", "Then turn on VPN in the app.")
    } else {
        lang.text("Импортируйте ссылку в приложение и включите VPN.", "Import the link into the app and turn on VPN.")
    };
    let install = a.store_url.as_deref().filter(|v|!v.is_empty()).map(|url|localized!(lang, r#"<a class="button install" href="{}" target="_blank" rel="noopener"><span class="step-number">1</span><span>Установить {}</span>{}</a>"#, r#"<a class="button install" href="{}" target="_blank" rel="noopener"><span class="step-number">1</span><span>Install {}</span>{}</a>"#,esc(url),esc(&a.name),icon_external())).unwrap_or_else(||localized!(lang, r#"<p class="manual-install">Установите {} из официального источника.</p>"#, r#"<p class="manual-install">Install {} from its official source.</p>"#,esc(&a.name)));
    let add = deeplink.map(|url|localized!(lang, r#"<a class="button primary import" href="{}" data-import><span class="step-number">2</span><span>Добавить подписку</span>{}</a>"#, r#"<a class="button primary import" href="{}" data-import><span class="step-number">2</span><span>Add subscription</span>{}</a>"#,esc(&url),icon_arrow())).unwrap_or_else(||localized!(lang, r#"<button class="button primary" type="button" data-copy><span class="step-number">2</span><span>Скопировать ссылку</span>{}</button>"#, r#"<button class="button primary" type="button" data-copy><span class="step-number">2</span><span>Copy link</span>{}</button>"#,icon_link()));
    let guide = a.guide.as_deref().filter(|v|!v.is_empty()).map(|text|localized!(lang, r#"<details class="app-guide"><summary>Инструкция для {}</summary><p class="prose">{}</p></details>"#, r#"<details class="app-guide"><summary>Instructions for {}</summary><p class="prose">{}</p></details>"#,esc(&a.name),esc(text))).unwrap_or_default();
    // Guides are inside the help sheet when selected, never expanded in the compact connection panel.
    format!(
        r#"<div class="connect-actions">{install}{add}</div><p class="connect-hint">{hint}</p><template class="app-help">{guide}</template>"#
    )
}

fn icon_check() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m5 12 4 4L19 6"/></svg>"#
}
fn icon_arrow() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h16m-6-6 6 6-6 6"/></svg>"#
}
fn icon_info() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 .3c0 1.7-2.5 2-2.5 3.7m0 3h.01"/></svg>"#
}
fn icon_servers() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18M5 6.5h14M5 17.5h14"/></svg>"#
}
fn icon_qr() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M3 3h6v6H3zm12 0h6v6h-6zM3 15h6v6H3zm12 0h3v3h3v3h-6zM12 3v3m0 6v3M3 12h3m3 0h3m6 0h3m-9 6v3"/></svg>"#
}
// ── иконки интерфейса ──
fn icon_link() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"><path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/></svg>"#
}
fn icon_send() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linejoin="round"><path d="M22 2 11 13"/><path d="M22 2 15 22l-4-9-9-4Z"/></svg>"#
}
fn icon_device() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9"><rect x="6" y="2" width="12" height="20" rx="3"/><path d="M11 18h2"/></svg>"#
}
fn icon_alert() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>"#
}
fn icon_lock() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><rect x="4" y="11" width="16" height="10" rx="3"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>"#
}
fn icon_external() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><path d="M15 3h6v6"/><path d="M10 14 21 3"/></svg>"#
}
fn icon_chevron() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#
}

/// Percent-кодирование для вставки ссылки внутрь deeplink.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_units_and_durations() {
        for (days, expected) in [(-1, "expired"), (0, "expires today"), (1, "1 day left"), (24, "24 days left"), (31, "~1 month left"), (90, "~3 months left")] { assert_eq!(humanize_days(days, Language::En), expected); }
        assert_eq!(format_bytes(1024, Language::En), "1 KB");
        assert_eq!(format_bytes(1073741824, Language::En), "1 GB");
        assert_eq!(Language::En.default_text("Мой собственный заголовок"), "Мой собственный заголовок");
    }
    #[test]
    fn primary_label_remains_readable_for_owner_colors() {
        for color in ["#533bff","#12bfd5","#ffffff","#000000","#ffff00","#ff00ff","#00ff00"] {
            let adjusted=readable_button_color(color);
            let rgb=[1,3,5].map(|i|u8::from_str_radix(&adjusted[i..i+2],16).unwrap());
            assert!(white_contrast(rgb)>=4.5);
        }
        assert_eq!(readable_button_color("#533bff"),"#533bff");
    }
    #[test]
    fn connection_page_states_and_preview() {
        let mut d = SubData {
            external_squad:serde_json::json!({}),
            username: "Александр <test>".into(),
            status: "active".into(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(24)),
            used: 12_000_000_000,
            limit: Some(100_000_000_000),
            device_limit: 5,
            tariff: Some("Личный".into()),
            hosts: vec![crate::formats::HostEntry {
                remark: "Германия · Франкфурт".into(),
                address: "de.example.com".into(),
                port: 443,
                protocol: "vless".into(),
                network: "tcp".into(),
                security: "reality".into(),
                sni: None,
                fingerprint: None,
                alpn: None,
                path: None,
                public_key: None,
                short_id: None,
                method: None,
                service_name: None,
                server_key: Some("secret-server-key".into()),
                host_header: None,
        options: serde_json::json!({}),
                uuid: "secret-client-uuid".into(),
            }],
        };
        let apps: Vec<App> = ["ios", "android", "windows", "macos", "linux"]
            .iter()
            .map(|platform| App {
                name: "Happ".into(),
                platform: (*platform).into(),
                deeplink: Some("happ://add/{url}".into()),
                store_url: Some("https://example.com/download".into()),
                guide: None,
                recommended: true,
                icon_svg: None,
                icon_url: None,
            })
            .collect();
        let settings = PageSettings {
            branding: sn_core::customer_brand::CustomerBrand::from_config(&serde_json::json!({
                "brand":"STEALTHNET", "logo":"https://panel.example.com/customer-brand/stealthnet.svg",
                "logo_dark":"https://panel.example.com/customer-brand/stealthnet-dark.svg",
                "favicon":"https://panel.example.com/customer-brand/favicon.svg",
                "accent":"#533bff", "accent_end":"#12bfd5"
            })),
            support_url: Some("https://example.com/support".into()),
            ..Default::default()
        };
        let url = "https://sub.example.com/s/demo";
        let html = render(&d, "STEALTHNET", url, &apps, &settings, "");
        assert!(html.contains("Германия · Франкфурт"));
        assert!(html.contains("--a1:#533bff;--a2:#12bfd5"));
        assert!(html.contains("/customer-brand/stealthnet-dark.svg"));
        assert!(html.contains("rel=\"icon\""));
        assert!(html.contains("/fonts/roboto.ttf"));
        assert!(html.contains("de.example.com"));
        assert!(html.contains("Александр &lt;test&gt;"));
        assert!(html.contains("id=\"deviceSelect\""));
        assert!(!html.contains("secret-client-uuid"));
        assert!(!html.contains("secret-server-key"));
        let preview_path = std::env::var("SN_CONNECTION_PREVIEW").ok();
        for status in ["active", "expired", "disabled", "limited"] {
            d.status = status.into();
            let english = render_localized(&d, "STEALTHNET", url, &apps, &settings, "", Language::En);
            assert!(english.contains("<html lang=\"en\""));
            assert!(english.contains("Connection help"));
            assert!(english.contains("Александр &lt;test&gt;"));
            assert!(english.contains("Личный"));
            assert!(english.contains("GB"));
            assert!(!english.contains("<span>Подписка</span>"));
            assert!(!english.contains("Установить Happ"));
            if status != "active" { assert!(!english.contains("happ://add/")); }
            if let Some(path) = &preview_path { std::fs::write(std::path::Path::new(path).parent().unwrap().join(format!("{status}-en.html")), &english).unwrap(); }
        }
        d.status = "active".into();
        if let Some(path) = &preview_path {
            std::fs::write(path, &html).unwrap();
            let parent = std::path::Path::new(path).parent().unwrap();
            std::fs::write(parent.join("no-apps-en.html"), render_localized(&d, "STEALTHNET", url, &[], &settings, "", Language::En)).unwrap();
            let saved = std::mem::take(&mut d.hosts);
            let empty = render_localized(&d, "STEALTHNET", url, &[], &settings, "", Language::En);
            assert!(empty.contains("Locations are being prepared"));
            std::fs::write(parent.join("no-hosts-en.html"), empty).unwrap();
            d.hosts = saved;

            let no_accent=PageSettings{branding:sn_core::customer_brand::CustomerBrand{accent:None,accent_end:None,..settings.branding.clone()},..settings.clone()};
            std::fs::write(std::path::Path::new(path).parent().unwrap().join("no-accent.html"),render(&d,"STEALTHNET",url,&apps,&no_accent,"")).unwrap();
            let parent = std::path::Path::new(path).parent().unwrap();
            let saved_limits = (d.limit, d.expires_at, d.device_limit);
            d.limit = None;
            d.expires_at = None;
            d.device_limit = 0;
            std::fs::write(parent.join("unlimited.html"), render(&d, "STEALTHNET", url, &apps, &settings, "")).unwrap();
            std::fs::write(parent.join("unlimited-en.html"), render_localized(&d, "STEALTHNET", url, &apps, &settings, "", Language::En)).unwrap();
            (d.limit, d.expires_at, d.device_limit) = saved_limits;
            std::fs::write(
                parent.join("no-apps.html"),
                render(&d, "STEALTHNET", url, &[], &settings, ""),
            )
            .unwrap();
            let saved_hosts = std::mem::take(&mut d.hosts);
            let empty = render(&d, "STEALTHNET", url, &apps, &settings, "");
            assert!(empty.contains("Локации готовятся"));
            assert!(!empty.contains("happ://add/"));
            std::fs::write(parent.join("no-hosts.html"), empty).unwrap();
            d.hosts = saved_hosts;
            let more_apps: Vec<App> = apps.iter().flat_map(|a| [App {name:a.name.clone(),platform:a.platform.clone(),deeplink:a.deeplink.clone(),store_url:a.store_url.clone(),guide:Some("Откройте приложение, подтвердите добавление профиля и включите VPN.\nЕсли импорт не сработал, скопируйте ссылку вручную.".into()),recommended:true,icon_svg:None,icon_url:None}, App{name:"Hiddify".into(),platform:a.platform.clone(),deeplink:None,store_url:None,guide:None,recommended:false,icon_svg:None,icon_url:None}]).collect();
            std::fs::write(
                parent.join("choices.html"),
                render(&d, "STEALTHNET", url, &more_apps, &settings, ""),
            )
            .unwrap();
            std::fs::write(parent.join("choices-en.html"), render_localized(&d, "STEALTHNET", url, &more_apps, &settings, "", Language::En)).unwrap();
            d.username = "Очень-длинное-имя-клиента-с-необычным-профилем".into();
            d.hosts = (0..30)
                .map(|i| {
                    let mut h = d.hosts[0].clone();
                    h.remark = format!("Локация {} · Длинное название региона и города", i + 1);
                    h
                })
                .collect();
            std::fs::write(parent.join("long-content.html"),render(&d,"Длинное название вашего VPN-сервиса",url,&more_apps,&PageSettings{footer_note:Some("Длинное примечание администратора. ".repeat(20)),..settings.clone()},"Работы на серверах: часть локаций может быть временно недоступна. Проверьте другие локации в приложении.")).unwrap();
        }
        for status in ["expired", "disabled", "limited"] {
            d.status = status.into();
            d.expires_at = Some(
                chrono::Utc::now()
                    + chrono::Duration::days(if status == "expired" { -5 } else { 24 }),
            );
            d.used = if status == "limited" {
                100_000_000_000
            } else {
                12_000_000_000
            };
            let html = render(&d, "STEALTHNET", url, &apps, &settings, "");
            assert!(!html.contains(url));
            assert!(!html.contains("de.example.com"));
            assert!(!html.contains("happ://"));
            assert!(!html.contains("id=\"linkSheet\""));
            if let Some(path) = &preview_path {
                std::fs::write(
                    std::path::Path::new(path)
                        .parent()
                        .unwrap()
                        .join(format!("{status}.html")),
                    html,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn ссылка_кодируется_для_deeplink() {
        assert_eq!(urlencode("https://a.b/s/x"), "https%3A%2F%2Fa.b%2Fs%2Fx");
        assert_eq!(urlencode("abc-_.~"), "abc-_.~");
    }

    #[test]
    fn плейсхолдер_url_подставляется() {
        let mut app = app("Happ");
        app.deeplink = Some("happ://add/{{URL}}".into());
        app.store_url = Some("https://store".into());
        app.recommended = true;
        let html = render_app(&app, "https://sub.example/s/abc", Language::Ru);
        assert!(html.contains("happ://add/https%3A%2F%2Fsub.example%2Fs%2Fabc"));
        assert!(html.contains("Установить Happ"));
        assert_eq!(html.matches("step-number").count(), 2);
        assert!(html.contains("Затем включите VPN в приложении"));
    }

    #[test]
    fn без_deeplink_показываем_ручной_путь() {
        let mut app = app("v2rayN");
        app.platform = "windows".into();
        app.store_url = Some("https://store".into());
        let html = render_app(&app, "https://sub.example/s/abc", Language::Ru);
        assert!(html.contains("Импортируйте ссылку"), "нужен ручной путь");
        assert!(!html.contains("Добавить подписку"));
        assert!(
            html.contains("data-copy"),
            "ручной импорт должен быть доступен"
        );
        assert!(html.contains("https://store"));
    }

    fn app(name: &str) -> App {
        App {
            name: name.into(),
            platform: "ios".into(),
            deeplink: None,
            store_url: None,
            guide: None,
            recommended: false,
            icon_svg: None,
            icon_url: None,
        }
    }

    #[test]
    fn иконки_различают_похожие_имена() {
        // v2rayNG содержит «v2ray», FlClash содержит «clash» —
        // порядок проверок должен давать разные иконки.
        assert_ne!(app_icon("v2rayNG"), app_icon("V2Box"));
        assert_ne!(app_icon("FlClash"), app_icon("mihomo"));
        assert_ne!(app_icon("Happ"), app_icon("Hiddify"));
        // Регистр не важен
        assert_eq!(app_icon("HAPP"), app_icon("happ"));
        // Незнакомое приложение получает запасную иконку, а не пустоту
        assert!(app_icon("Неизвестное").contains("<svg"));
    }

    #[test]
    fn своя_иконка_переопределяет_встроенную() {
        let mut a = app("Happ");
        a.icon_svg = Some("<svg data-custom></svg>".into());
        assert!(render_icon(&a).contains("data-custom"));

        let mut b = app("Happ");
        b.icon_url = Some("https://cdn.example/happ.png".into());
        let html = render_icon(&b);
        assert!(html.contains("<img"));
        assert!(html.contains("https://cdn.example/happ.png"));

        // Мусор вместо SVG не подставляем — падаем на встроенную
        let mut c = app("Happ");
        c.icon_svg = Some("<script>alert(1)</script>".into());
        let html = render_icon(&c);
        assert!(!html.contains("<script>"));
        assert!(html.contains("<svg"));
    }

    #[test]
    fn qr_код_генерируется() {
        let svg = qr_svg("https://sub.example/s/abcdefgh");
        assert!(svg.contains("<svg"));
        assert!(svg.len() > 200);
    }

    #[test]
    fn html_экранируется() {
        // Имя приходит из базы и может содержать угловые скобки —
        // без экранирования это XSS на странице клиента.
        let e = esc("<script>alert(1)</script>");
        assert!(!e.contains("<script>"));
        assert!(e.contains("&lt;script&gt;"));
    }

    #[test]
    fn срок_склоняется_по_русски() {
        assert_eq!(humanize_days(1, Language::Ru), "остался 1 день");
        assert_eq!(humanize_days(2, Language::Ru), "осталось 2 дня");
        assert_eq!(humanize_days(5, Language::Ru), "осталось 5 дней");
        assert_eq!(humanize_days(11, Language::Ru), "осталось 11 дней");
        assert_eq!(humanize_days(21, Language::Ru), "осталось 21 день");
        assert_eq!(humanize_days(0, Language::Ru), "истекает сегодня");
        assert_eq!(humanize_days(-1, Language::Ru), "истекла");
        assert_eq!(humanize_days(60, Language::Ru), "осталось ~2 месяца");
        assert_eq!(humanize_days(180, Language::Ru), "осталось ~6 месяцев");
        assert_eq!(humanize_days(31, Language::Ru), "осталось ~1 месяц");
    }
}

//! Nine original, offline mini-sites. No third-party requests, forms or tracking.
use crate::selfsteal::{Site, TEMPLATES};
use serde_json::{json, Value};

const NAMES: [[&str; 2]; 9] = [
    ["Дизайн-студия", "Design studio"], ["Журнал", "Journal"], ["Путешествия", "Travel notes"],
    ["Рецепты", "Recipe book"], ["Текстовые инструменты", "Text tools"], ["Книжная полка", "Bookshelf"],
    ["Галерея", "Geometry gallery"], ["Домашний сад", "Home garden"], ["Мировое время", "World clock"],
];
const DESCRIPTIONS: [[&str; 2]; 9] = [
    ["Типографика, проекты и заметки о дизайне.", "Typography, projects and notes on design."],
    ["Небольшие истории о повседневных вещах.", "Small stories about everyday things."],
    ["Идеи маршрутов и список вещей в дорогу.", "Itinerary ideas and a packing checklist."],
    ["Простые блюда и пересчёт ингредиентов.", "Simple dishes with a serving-size calculator."],
    ["Счётчик слов и форматирование текста в браузере.", "Word counts and text formatting in your browser."],
    ["Личная подборка классических книг.", "A personal selection of classic books."],
    ["Коллекция абстрактных геометрических работ.", "A collection of abstract geometric compositions."],
    ["Заметки по уходу за комнатными растениями.", "Notes on looking after indoor plants."],
    ["Точное время в разных часовых поясах.", "The current time in different time zones."],
];

pub fn catalog(language: &str) -> Vec<Value> {
    let l = usize::from(language == "en");
    TEMPLATES.iter().enumerate().map(|(i, id)| {
        let site = Site { domain: "example.com".into(), inbound_tag: "vpn".into(), template: (*id).into(), language: if l == 1 { "en" } else { "ru" }.into(), title: String::new(), description: String::new() };
        json!({"id":id,"name":NAMES[i][l],"description":DESCRIPTIONS[i][l],"html":render(&site)})
    }).collect()
}

fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;") }

pub fn render(site: &Site) -> String {
    let l = usize::from(site.language == "en");
    let t = |ru: &str, en: &str| if l == 1 { en.to_owned() } else { ru.to_owned() };
    let index = TEMPLATES.iter().position(|id| *id == site.template).unwrap_or(0);
    let title = escape(if site.title.trim().is_empty() { NAMES[index][l] } else { &site.title });
    let description = escape(if site.description.trim().is_empty() { DESCRIPTIONS[index][l] } else { &site.description });
    let details = |heading: &str, body: &str| format!("<details><summary>{}</summary><p>{}</p></details>", escape(heading), escape(body));
    let art = |n: usize| format!("<svg viewBox='0 0 480 340' role='img' aria-label='{}'><rect width='480' height='340' fill='var(--paper)'/>{}</svg>", t("Геометрическая композиция", "Geometric composition"), match n % 3 {
        0 => "<circle cx='240' cy='170' r='126' fill='var(--accent)'/><path d='M100 250 240 40 380 250Z' fill='var(--ink)'/><circle cx='240' cy='182' r='58' fill='var(--paper)'/>",
        1 => "<path d='M65 275V65h175v210ZM240 65h175v210H240Z' fill='var(--accent)'/><circle cx='240' cy='170' r='105' fill='var(--ink)'/><path d='M240 65a105 105 0 0 1 0 210Z' fill='var(--paper)'/>",
        _ => "<path d='M55 265 170 75l110 190ZM215 265 330 75l100 190Z' fill='var(--ink)'/><circle cx='330' cy='110' r='48' fill='var(--accent)'/><path d='M55 280h375' stroke='var(--accent)' stroke-width='12'/>",
    });
    let content = match site.template.as_str() {
        "studio" => format!("<section class='portfolio'><h2>{}</h2><article><div class='print'>{}</div><div><h3>{}</h3><p>{}</p></div></article><article><div class='print'>{}</div><div><h3>{}</h3><p>{}</p></div></article></section><section class='endnote'><h2>{}</h2><p>{}</p></section>",
            t("Избранные работы", "Selected studies"), art(0), t("Форма и пространство", "Form & space"), t("Упражнение в равновесии: круг, треугольник и свободное поле. Ограниченная палитра помогает увидеть отношения между формами.", "An exercise in balance: a circle, a triangle and an open field. A limited palette brings the relationship between forms into focus."), art(1), t("Ритм на плоскости", "Rhythm on a plane"), t("Повторение одного элемента создаёт порядок. Небольшое изменение в середине композиции задаёт точку внимания.", "Repeating a single element creates order. A small change in the middle of the composition gives the eye a place to rest."), t("Начать с простого", "Start with something simple"), t("Выбрать один цвет. Убрать лишнее. Дать идее место. Так рождаются наши визуальные исследования.", "Choose one colour. Remove what is unnecessary. Give an idea some room. That is how these visual studies begin.")),
        "journal" => format!("<section class='reading'><h2>{}</h2>{}{}{}<aside>{}</aside></section>", t("Свежие записи", "Recent entries"),
            details(&t("Прогулка без маршрута", "A walk without a route"), &t("Иногда лучший план — выйти на знакомую улицу и повернуть в другую сторону. Обратите внимание на вывеску, дерево у остановки или отражение в окне. Обычный район становится новым, когда идёшь чуть медленнее.", "Sometimes the best plan is to step onto a familiar street and take a different turn. Notice a sign, a tree by the bus stop, or a reflection in a window. An ordinary neighbourhood feels new when you walk a little more slowly.")),
            details(&t("Бумажный блокнот", "A paper notebook"), &t("В блокноте удобно хранить незаконченные мысли. Не обязательно писать каждый день или начинать с первой страницы. Несколько слов и быстрый набросок часто сохраняют больше, чем аккуратный длинный отчёт.", "A notebook is a good place for unfinished thoughts. You do not have to write every day or start on the first page. A few words and a quick sketch can preserve more than a carefully polished account.")),
            details(&t("Время на чтение", "Making room for reading"), &t("Оставьте книгу там, где обычно ждёте: рядом с чайником или на краю стола. Десять спокойных минут — уже часть истории. Можно остановиться посреди главы и вернуться завтра.", "Leave a book somewhere you often wait: beside the kettle or at the edge of a table. Ten quiet minutes are already part of a story. You can stop halfway through a chapter and return tomorrow.")),
            t("Наблюдения не обязаны быть большими, чтобы заслуживать записи.", "Observations do not have to be remarkable to be worth writing down.")),
        "travel" => format!("<section class='travel-plan'><div><h2>{}</h2>{}{}<p>{}</p></div><fieldset><legend>{}</legend>{}</fieldset></section>", t("Выходные без спешки", "An unhurried weekend"),
            details(&t("День в городе", "A day in the city"), &t("Утром — прогулка по старому кварталу. Днём — небольшой музей и обед в стороне от главной площади. Вечером — набережная или парк. Оставьте между пунктами свободное время.", "Morning: a walk through the old quarter. Afternoon: a small museum and lunch away from the main square. Evening: a waterfront or park. Leave some free time between stops.")),
            details(&t("День на природе", "A day outdoors"), &t("Выберите короткую маркированную тропу, проверьте прогноз и время заката. Возьмите воду и слой одежды на случай перемены погоды. Сообщите близким маршрут и время возвращения.", "Choose a short marked trail and check the forecast and sunset time. Take water and an extra layer for changing weather. Let someone know your route and when you expect to return.")),
            t("Проверьте часы работы и транспорт перед поездкой.", "Check opening hours and transport before travelling."), t("Собрать рюкзак", "Pack your bag"),
            [["Документы", "Documents"], ["Зарядка и кабель", "Charger and cable"], ["Бутылка воды", "Water bottle"], ["Удобная обувь", "Comfortable shoes"], ["Офлайн-карта", "Offline map"], ["Лёгкая куртка", "Light jacket"]].iter().map(|v| format!("<label class='check'><input type='checkbox'><span>{}</span></label>", v[l])).collect::<String>()),
        "recipes" => format!("<section class='recipe'><div><h2>{}</h2><p>{}</p><label>{} <input id='servings' type='number' min='1' max='20' value='2'></label><ul class='ingredients'><li><b data-amount='100'>200</b> {} {}</li><li><b data-amount='150'>300</b> {} {}</li><li><b data-amount='1'>2</b> {} {}</li><li><b data-amount='1'>2</b> {} {}</li></ul></div><div><h3>{}</h3><ol><li>{}</li><li>{}</li><li>{}</li></ol><p>{}</p></div></section>",
            t("Паста с томатами", "Tomato pasta"), t("Простой ужин из нескольких ингредиентов.", "A simple supper made with a few ingredients."), t("Порций", "Servings"), t("г", "g"), t("пасты", "pasta"), t("г", "g"), t("томатов", "tomatoes"), t("ст. л.", "tbsp"), t("оливкового масла", "olive oil"), t("зубчика", "cloves"), t("чеснока", "garlic"),
            t("Приготовление", "Method"), t("Отварите пасту в подсоленной воде по времени на упаковке. Сохраните немного воды от варки.", "Cook the pasta in salted water according to its packet. Save a little cooking water."), t("Слегка прогрейте нарезанный чеснок в масле. Добавьте томаты и тушите до мягкости.", "Gently warm the sliced garlic in the oil. Add the tomatoes and simmer until soft."), t("Смешайте пасту с соусом. Добавьте немного воды от варки, если нужно, и приправьте по вкусу.", "Toss the pasta with the sauce. Add some cooking water if needed and season to taste."), t("По желанию добавьте базилик. Учитывайте аллергии при выборе ингредиентов.", "Add basil if you like. Choose ingredients that suit your dietary needs.")),
        "tools" => format!("<section class='editor'><h2>{}</h2><label for='text-input'>{}</label><textarea id='text-input' rows='8' placeholder='{}'></textarea><div class='toolbar'><button data-text-action='upper'>{}</button><button data-text-action='lower'>{}</button><button data-text-action='spaces'>{}</button><button data-text-action='clear'>{}</button></div><p class='count' aria-live='polite'><span id='words'>0</span> {} · <span id='characters'>0</span> {}</p><p>{}</p></section>",
            t("Рабочий лист", "Scratchpad"), t("Ваш текст", "Your text"), t("Введите или вставьте текст…", "Type or paste text…"), t("ПРОПИСНЫЕ", "UPPERCASE"), t("строчные", "lowercase"), t("Убрать лишние пробелы", "Trim extra spaces"), t("Очистить", "Clear"), t("слов", "words"), t("символов", "characters"), t("Текст обрабатывается только в этой вкладке и никуда не отправляется.", "Your text stays in this tab and is never sent anywhere.")),
        "library" => format!("<section class='books'><h2>{}</h2><label for='book-search'>{}</label><input id='book-search' type='search' placeholder='{}'><div id='book-list'>{}{}{}{}</div><p id='no-books' hidden>{}</p></section>",
            t("На полке", "On the shelf"), t("Найти книгу", "Find a book"), t("Название или автор", "Title or author"),
            book(&t("Гордость и предубеждение", "Pride and Prejudice"), &t("Джейн Остин", "Jane Austen"), &t("Разговоры, первые впечатления и умение пересмотреть своё мнение.", "Conversations, first impressions, and the ability to reconsider an opinion."), "1813"),
            book(&t("Вокруг света за 80 дней", "Around the World in Eighty Days"), &t("Жюль Верн", "Jules Verne"), &t("Путешествие, в котором расписание постоянно встречается с неожиданностями.", "A journey in which a carefully made schedule keeps meeting the unexpected."), "1872"),
            book(&t("Маленькие женщины", "Little Women"), &t("Луиза Мэй Олкотт", "Louisa May Alcott"), &t("Четыре сестры выбирают свой путь и учатся поддерживать друг друга.", "Four sisters find their own paths and learn to support one another."), "1868"),
            book(&t("Алиса в Стране чудес", "Alice’s Adventures in Wonderland"), &t("Льюис Кэрролл", "Lewis Carroll"), &t("Игра с языком, логикой и правилами знакомого мира.", "A playful journey through language, logic and the rules of a familiar world."), "1865"), t("Ничего не найдено", "No books found")),
        "gallery" => format!("<section class='gallery'><h2>{}</h2><div class='art-grid'>{}</div><p>{}</p></section>", t("Исследования формы", "Studies in form"), (0..3).map(|n| format!("<figure>{}<figcaption>{}</figcaption></figure>", art(n), [["Равновесие", "Balance"], ["Пересечение", "Intersection"], ["Движение", "Movement"]][n][l])).collect::<String>(), t("Круги, линии и плоскости. Три способа взглянуть на одну палитру.", "Circles, lines and planes. Three ways of looking at a single palette.")),
        "garden" => format!("<section class='plant-notes'><h2>{}</h2>{}{}{}<aside>{}</aside></section>", t("Зелёные заметки", "Green notes"),
            details(&t("Свет", "Light"), &t("Понаблюдайте за местом в течение дня. Прямое полуденное солнце и рассеянный свет — разные условия. Учитывайте потребности конкретного вида, а зимой при необходимости переставьте растение ближе к окну.", "Watch the space over the course of a day. Direct midday sun and indirect light are different conditions. Consider the needs of each species, and move a plant closer to the window in winter if needed.")),
            details(&t("Полив", "Watering"), &t("Перед поливом проверьте влажность грунта. Частота зависит от вида растения, размера горшка, сезона и температуры. Дренажное отверстие помогает лишней воде выйти; не оставляйте воду в поддоне надолго.", "Check the soil before watering. Frequency depends on the plant, pot size, season and temperature. A drainage hole lets excess water escape; do not leave water sitting in the saucer.")),
            details(&t("Пересадка", "Repotting"), &t("Осмотрите корни и выберите подходящий для растения грунт. Новый горшок обычно нужен лишь немного больше предыдущего. После пересадки дайте растению время привыкнуть к новым условиям.", "Inspect the roots and choose a potting mix suited to the plant. The new pot usually only needs to be a little larger. Give the plant time to adjust after repotting.")), t("У растений разные потребности — уточняйте рекомендации для своего вида.", "Every plant has different needs. Check the guidance for your particular species.")),
        _ => format!("<section class='timepiece'><label for='zone'>{}</label><select id='zone'><option value='local'>{}</option><option value='UTC'>UTC</option><option value='Europe/London'>London</option><option value='Europe/Berlin'>Berlin</option><option value='Asia/Tokyo'>Tokyo</option><option value='America/New_York'>New York</option></select><time id='clock'>--:--:--</time><p id='clock-date'></p><p>{}</p></section>", t("Часовой пояс", "Time zone"), t("Местное время", "Local time"), t("Используется время вашего устройства. Часовой пояс можно изменить выше.", "Uses your device’s clock. Choose a different time zone above.")),
    };
    let css = format!("@font-face{{font-family:'PT Serif';src:url(data:font/woff2;base64,{}) format('woff2');font-weight:400;font-style:normal;font-display:swap}}{}", include_str!("selfsteal_font.b64"), include_str!("selfsteal_site.css"));
    format!("<!doctype html><html lang='{}'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><meta name='color-scheme' content='light'><meta name='description' content='{description}'><meta http-equiv='Content-Security-Policy' content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'\"><link rel='icon' href='data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 32 32%22%3E%3Cpath d=%22M5 5h22v22H5z%22 fill=%22%2335473c%22/%3E%3Ccircle cx=%2216%22 cy=%2216%22 r=%226%22 fill=%22%23fff%22/%3E%3C/svg%3E'><title>{title}</title><style>{}</style></head><body class='{}'><header><a href='#' class='wordmark'>{title}</a><a href='#content'>{}</a></header><main><section class='intro'><h1>{title}</h1><p>{description}</p></section><div id='content'>{content}</div></main><footer><span>{}</span><a href='#'>{}</a></footer><script>{}</script></body></html>", if l == 1 { "en" } else { "ru" }, css, TEMPLATES[index], t("Посмотреть", "Explore"), escape(&site.domain), t("Наверх", "Back to top"), include_str!("selfsteal_site.js"))
}
fn book(title: &str, author: &str, description: &str, year: &str) -> String {
    format!("<article class='book'><div class='spine' aria-hidden='true'>{year}</div><div><h3>{}</h3><p>{}</p><p>{}</p></div></article>", escape(title), escape(author), escape(description))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn nine_bilingual_offline_sites_and_escaped_custom_text() {
        for lang in ["ru", "en"] {
            let items = catalog(lang); assert_eq!(items.len(), 9);
            for (i, item) in items.iter().enumerate() { let html = item["html"].as_str().unwrap(); assert!(html.contains(&format!("lang='{lang}'"))); assert!(html.contains(&format!("class='{}'", TEMPLATES[i]))); assert!(!html.contains("src='https:")); assert!(!html.contains("<form")); assert!(html.contains("connect-src 'none'")); }
        }
        let s = Site {domain:"test.example.com".into(), inbound_tag:"vpn".into(),template:"studio".into(), language:"en".into(),title:"</title><script>alert(1)</script>".into(),description:"\" onload='evil' <img>".into()};
        let html=render(&s); assert!(!html.contains("<script>alert(1)")); assert!(!html.contains("<img>")); assert!(html.contains("&lt;/title&gt;"));
    }
}

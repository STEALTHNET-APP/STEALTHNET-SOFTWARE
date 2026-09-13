//! Обращения в поддержку из бота.
//!
//! Пишем в те же таблицы, которые читает панель: у администратора это
//! обычный тикет со всей перепиской, а не отдельный канал, о котором
//! нужно помнить. Ответ администратора приходит человеку в бот — это уже
//! умеет панель.
//!
//! Диалог двухшаговый: нажали «написать» — прислали текст. Между шагами
//! ожидание лежит в `bot_await` и протухает через полчаса, чтобы
//! случайное сообщение через сутки не превратилось в обращение.

use serde_json::Value;
use sn_core::{Pool, Result};
use sqlx::Row;

use crate::settings::Screen;
use crate::tg::{keyboard, Btn};
use crate::Ctx;

/// Сколько обращений показываем списком. Больше — уже история, за ней
/// человек идёт к поддержке, а не листает бота.
const LIST_LIMIT: i64 = 10;

/// Список обращений клиента.
pub async fn show_list(c: &Ctx<'_>, chat: i64, client_id: i64, mid: Option<i64>) -> Result<()> {
    let rows = sqlx::query(
        "SELECT t.id, t.subject, t.status::text AS status, t.updated_at,
                (SELECT count(*) FROM ticket_messages m WHERE m.ticket_id = t.id) AS msgs
           FROM tickets t
          WHERE t.client_id = $1
          ORDER BY t.updated_at DESC
          LIMIT $2",
    )
    .bind(client_id)
    .bind(LIST_LIMIT)
    .fetch_all(c.pool)
    .await?;

    let mut text = format!("<b>Ваши обращения</b>\n{}\n\n",html_escape(&c.s.custom("bot.support_text","Опишите, что случилось — поможем с подключением.")));
    let mut kb: Vec<Vec<Btn>> = Vec::new();

    if rows.is_empty() {
        text.push_str("Пока ни одного. Напишите, если что-то не работает — ответим здесь же.");
    } else {
        for r in &rows {
            let id: i64 = r.get("id");
            let subject: String = r.get("subject");
            let status: String = r.get("status");
            let msgs: i64 = r.get("msgs");
            text.push_str(&format!(
                "{} <b>#{id}</b> · {}\n{}\n\n",
                status_icon(&status),
                status_label(&status),
                html_escape(&subject),
            ));
            kb.push(vec![Btn::Data(
                format!("#{id} · {}", short(&subject, 24)),
                format!("tk:{id}"),
            )]);
            let _ = msgs;
        }
    }

    if c.s.tickets_enabled() {kb.push(vec![Btn::Data("✍️ Новое обращение".into(), "tk:new".into())]);}
    kb.push(vec![Btn::Data("← Назад".into(), "menu".into())]);

    c.tg.screen(chat, mid, &text, c.s.image(Screen::Tickets).as_deref(), Some(keyboard(kb)))
        .await?;
    Ok(())
}

/// Переписка по одному обращению.
pub async fn show_one(c: &Ctx<'_>, chat: i64, client_id: i64, ticket_id: i64, mid: Option<i64>) -> Result<()> {
    // Проверяем принадлежность: идентификатор приходит из кнопки, а её
    // содержимое клиент может подменить.
    let head = sqlx::query(
        "SELECT subject, status::text AS status FROM tickets
          WHERE id = $1 AND client_id = $2",
    )
    .bind(ticket_id)
    .bind(client_id)
    .fetch_optional(c.pool)
    .await?;

    let Some(head) = head else {
        return show_list(c, chat, client_id, mid).await;
    };
    let subject: String = head.get("subject");
    let status: String = head.get("status");

    let msgs = sqlx::query(
        "SELECT author_kind, body, created_at FROM ticket_messages
          WHERE ticket_id = $1 ORDER BY id DESC LIMIT 8",
    )
    .bind(ticket_id)
    .fetch_all(c.pool)
    .await?;

    let mut text = format!(
        "<b>Обращение #{ticket_id}</b> · {}\n{}\n\n",
        status_label(&status),
        html_escape(&subject)
    );
    for m in msgs.iter().rev() {
        let who = if m.get::<String, _>("author_kind") == "admin" { "Поддержка" } else { "Вы" };
        let body: String = m.get("body");
        text.push_str(&format!("<b>{who}:</b> {}\n\n", html_escape(&short(&body,300))));
    }

    text.push_str("Последние сообщения. Полная переписка доступна в Mini App.\n");
    let mut kb = Vec::new();
    if status != "closed" {
        kb.push(vec![Btn::Data("✍️ Ответить".into(), format!("tk:r:{ticket_id}"))]);
    }
    kb.push(vec![Btn::Data("← К обращениям".into(), "tickets".into())]);

    c.tg.screen(chat, mid, &text, c.s.image(Screen::Tickets).as_deref(), Some(keyboard(kb)))
        .await?;
    Ok(())
}

/// Запомнить, что ждём текст: нового обращения или ответа.
pub async fn expect(pool: &Pool, client_id: i64, kind: &str, ticket_id: Option<i64>) -> Result<()> {
    sqlx::query(
        "INSERT INTO bot_await (client_id, kind, ticket_id, expires_at)
         VALUES ($1, $2, $3, now() + interval '30 minutes')
         ON CONFLICT (client_id) DO UPDATE
            SET kind = EXCLUDED.kind, ticket_id = EXCLUDED.ticket_id,
                expires_at = EXCLUDED.expires_at",
    )
    .bind(client_id)
    .bind(kind)
    .bind(ticket_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Ждём ли мы от этого клиента текст. Протухшее ожидание убираем.
pub async fn pending(pool: &Pool, client_id: i64) -> Result<Option<(String, Option<i64>)>> {
    let row = sqlx::query(
        "DELETE FROM bot_await WHERE client_id = $1 RETURNING kind, ticket_id, expires_at > now() AS alive",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|r| {
        if r.get::<bool, _>("alive") {
            Some((r.get("kind"), r.get("ticket_id")))
        } else {
            None
        }
    }))
}

/// Принять текст от клиента: создать обращение или дописать ответ.
pub async fn accept_text(
    c: &Ctx<'_>,
    chat: i64,
    client_id: i64,
    kind: &str,
    ticket_id: Option<i64>,
    body: &str,
) -> Result<()> {
    if !c.s.tickets_enabled() {return c.tg.send(chat,"Обращения отключены. Откройте /support для связи с поддержкой.",None).await.map(|_| ());}
    let body = body.trim();
    if body.chars().count()>4000 {
        c.tg.send(chat,"Сообщение слишком длинное: максимум 4000 символов.",None).await?;
        return expect(c.pool,client_id,kind,ticket_id).await;
    }
    if body.is_empty() {
        c.tg.send(chat, "Пустое сообщение отправлять некуда — напишите текст.", None).await?;
        return expect(c.pool, client_id, kind, ticket_id).await;
    }

    match kind {
        "ticket_new" => {
            // Тема — первая строка: отдельно её спрашивать значит добавить
            // ещё один шаг ради того, что и так видно в первом сообщении.
            let subject = short(body.lines().next().unwrap_or(body), 120);
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO tickets (client_id, subject) VALUES ($1, $2) RETURNING id",
            )
            .bind(client_id)
            .bind(&subject)
            .fetch_one(c.pool)
            .await?;
            sqlx::query(
                "INSERT INTO ticket_messages (ticket_id, author_kind, body)
                 VALUES ($1, 'client', $2)",
            )
            .bind(id)
            .bind(body)
            .execute(c.pool)
            .await?;
            c.tg.send(
                chat,
                &format!("Обращение <b>#{id}</b> создано. Ответим здесь же."),
                Some(keyboard(vec![
                    vec![Btn::Data("Открыть".into(), format!("tk:{id}"))],
                    vec![Btn::Data("← В меню".into(), "menu".into())],
                ])),
            )
            .await?;
        }
        "ticket_reply" => {
            let Some(tid) = ticket_id else {
                return show_list(c, chat, client_id, None).await;
            };
            // Ещё раз проверяем принадлежность: между нажатием и текстом
            // обращение могли закрыть или оно вообще не его.
            let ok: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM tickets WHERE id = $1 AND client_id = $2 AND status <> 'closed'",
            )
            .bind(tid)
            .bind(client_id)
            .fetch_optional(c.pool)
            .await?;
            if ok.is_none() {
                c.tg.send(chat, "Это обращение уже закрыто.", None).await?;
                return show_list(c, chat, client_id, None).await;
            }
            sqlx::query(
                "INSERT INTO ticket_messages (ticket_id, author_kind, body)
                 VALUES ($1, 'client', $2)",
            )
            .bind(tid)
            .bind(body)
            .execute(c.pool)
            .await?;
            // Мяч на стороне поддержки.
            sqlx::query("UPDATE tickets SET status = 'open', updated_at = now() WHERE id = $1")
                .bind(tid)
                .execute(c.pool)
                .await?;
            c.tg.send(chat, "Ответ отправлен.", None).await?;
            return show_one(c, chat, client_id, tid, None).await;
        }
        _ => {}
    }
    Ok(())
}

/// Разбор нажатий, относящихся к обращениям.
///
/// Возвращает `false`, если кнопка не наша, — тогда её обработает общий
/// разбор callback'ов.
pub async fn on_callback(
    c: &Ctx<'_>,
    chat: i64,
    mid: i64,
    client_id: i64,
    parts: &[&str],
) -> Result<bool> {
    match parts {
        ["tickets"] => {
            show_list(c, chat, client_id, Some(mid)).await?;
            Ok(true)
        }
        ["tk", "new"] => {
            expect(c.pool, client_id, "ticket_new", None).await?;
            c.tg.screen(
                chat,
                Some(mid),
                "Опишите, что случилось — одним сообщением.\n\n\
                 Первая строка станет темой обращения.",
                c.s.image(Screen::Tickets).as_deref(),
                Some(keyboard(vec![vec![Btn::Data("← Отмена".into(), "tickets".into())]])),
            )
            .await?;
            Ok(true)
        }
        ["tk", "r", id] => {
            let tid: i64 = id.parse().unwrap_or(0);
            expect(c.pool, client_id, "ticket_reply", Some(tid)).await?;
            c.tg.screen(
                chat,
                Some(mid),
                &format!("Напишите ответ по обращению #{tid} одним сообщением."),
                c.s.image(Screen::Tickets).as_deref(),
                Some(keyboard(vec![vec![Btn::Data("← Отмена".into(), format!("tk:{tid}"))]])),
            )
            .await?;
            Ok(true)
        }
        ["tk", id] => {
            let tid: i64 = id.parse().unwrap_or(0);
            show_one(c, chat, client_id, tid, Some(mid)).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn status_icon(s: &str) -> &'static str {
    match s {
        "open" => "🟡",
        "pending" => "🔵",
        "closed" => "⚪️",
        _ => "•",
    }
}

fn status_label(s: &str) -> &'static str {
    match s {
        "open" => "ждёт ответа поддержки",
        "pending" => "поддержка ответила",
        "closed" => "закрыто",
        _ => "в работе",
    }
}

/// Обрезка по границе символов, а не байтов: русский текст в UTF-8
/// многобайтовый, и срез по индексу байта уронил бы бота на панике.
fn short(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// Экранирование для parse_mode=HTML.
///
/// Текст пишет человек, и одинокий «<» ломает разметку: Telegram
/// отказывается отправить сообщение целиком, а не показывает его как есть.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Заглушка, чтобы `Value` не считался неиспользованным при правках выше.
#[allow(dead_code)]
fn _unused(_: &Value) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn обрезка_не_рвёт_многобайтовые_символы() {
        // Срез по байтам здесь уронил бы процесс.
        assert_eq!(short("привет", 10), "привет");
        assert_eq!(short("абвгдежзик", 5), "абвг…");
        assert_eq!(short("  пробелы по краям  ", 50), "пробелы по краям");
    }

    #[test]
    fn разметка_экранируется() {
        assert_eq!(html_escape("<b>не тег</b>"), "&lt;b&gt;не тег&lt;/b&gt;");
        assert_eq!(html_escape("1 & 2"), "1 &amp; 2");
    }

    #[test]
    fn статусы_переведены() {
        for s in ["open", "pending", "closed"] {
            assert!(!status_label(s).is_empty());
            assert_ne!(status_icon(s), "•", "у известного статуса свой значок");
        }
        assert_eq!(status_icon("невиданный"), "•");
    }
}

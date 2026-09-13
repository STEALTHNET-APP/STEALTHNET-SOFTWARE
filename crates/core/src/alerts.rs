//! Durable team alerts. Telegram failures never roll back a sale or a support ticket.
use crate::{bot_config as bc,Error,Pool,Result};
use serde_json::{json,Value,Map};
use sqlx::Row;

pub fn destination(s:&Map<String,Value>)->Result<(String,Option<i64>)> {
    let chat=bc::text(s,"bot.alert_chat_id","");
    if chat.parse::<i64>().ok().is_none_or(|id|id>=0||id.to_string()!=chat) {return Err(Error::bad("Укажите отрицательный ID Telegram-группы, например -1001234567890"));}
    let raw=bc::text(s,"bot.alert_thread_id","");
    let topic=if raw.is_empty(){None}else{Some(raw.parse::<i64>().ok().filter(|id|*id>0).ok_or_else(||Error::bad("ID темы должен быть положительным целым числом"))?)};
    Ok((chat.to_string(),topic))
}
fn enabled(s:&Map<String,Value>,kind:&str)->bool {bc::flag(s,"bot.admin_alerts_enabled",false)&&bc::flag(s,&format!("bot.alert_{kind}_enabled"),true)}
fn short(s:&str,n:usize)->String {s.chars().take(n).collect()}
fn jump(panel:&str,route:&str)->String {if bc::web_url(panel,true){format!("\n\n{}/#/{route}",panel.trim_end_matches('/'))}else{String::new()}}
async fn message(pool:&Pool,kind:&str,payload:&Value,brand:&str,panel:&str)->Result<String> {
    let (title,body,route)=match kind {
        "clients"=>{
            let id=payload["client_id"].as_i64().unwrap_or(0);
            let name:Option<String>=sqlx::query_scalar("SELECT username FROM clients WHERE id=$1").bind(id).fetch_optional(pool).await?;
            ("Новый клиент",format!("Клиент #{} · {}",id,short(name.as_deref().unwrap_or("без имени"),100)),"users")
        },
        "payments"=>{
            let id=payload["payment_id"].as_i64().unwrap_or(0);
            let row=sqlx::query("SELECT p.client_id,c.username,p.amount_minor,p.currency,p.provider,p.kind::text AS kind,COALESCE(p.addon_snapshot->>'title',t.title,'Подписка') AS item FROM payments p LEFT JOIN clients c ON c.id=p.client_id LEFT JOIN tariffs t ON t.id=p.tariff_id WHERE p.id=$1").bind(id).fetch_optional(pool).await?.ok_or(Error::NotFound)?;
            (if payload["status"]=="refunded"{"Возврат платежа"}else{"Оплата получена"},format!("Счёт #{id} · {}\nКлиент #{} · {}\n{}\nСпособ: {}",crate::money::format_minor(row.get("amount_minor"),row.get("currency")),row.get::<Option<i64>,_>("client_id").unwrap_or(0),short(row.get::<Option<String>,_>("username").as_deref().unwrap_or("без имени"),100),short(row.get("item"),200),row.get::<String,_>("provider")),"payments")
        },
        "tickets"=>{
            let id=payload["ticket_id"].as_i64().unwrap_or(0);
            let row=sqlx::query("SELECT t.subject,t.client_id,c.username FROM tickets t JOIN clients c ON c.id=t.client_id WHERE t.id=$1").bind(id).fetch_optional(pool).await?.ok_or(Error::NotFound)?;
            (if payload["reply"]==true{"Ответ клиента в тикете"}else{"Новое обращение"},format!("Тикет #{id} · {}\nКлиент #{} · {}",short(row.get("subject"),250),row.get::<i64,_>("client_id"),short(row.get::<Option<String>,_>("username").as_deref().unwrap_or("без имени"),100)),"support")
        },
        "service"=>(if payload["recovered"]==true{"Сервис восстановлен"}else{"Проблема сервиса"},format!("{}\n{}",short(payload["title"].as_str().unwrap_or("Сервис"),150),short(payload["detail"].as_str().unwrap_or(""),600)),"home"),
        _=>("Проверка уведомлений","Группа подключена. Здесь будут выбранные владельцем события сервиса.".into(),"bot"),
    };
    Ok(format!("{} · {title}\n\n{body}{}",short(brand,100),jump(panel,route)))
}
struct DeliveryError { reason:String,retry_after:i64,permanent:bool }
async fn deliver(http:&reqwest::Client,endpoint:&str,chat:&str,topic:Option<i64>,text:&str)->std::result::Result<(),DeliveryError>{
    let mut body=json!({"chat_id":chat,"text":text,"link_preview_options":{"is_disabled":true}});
    if let Some(topic)=topic {body["message_thread_id"]=json!(topic);}
    let result=match http.post(endpoint).json(&body).send().await {
        Ok(r)=>r.json::<Value>().await.ok(),Err(_)=>None,
    };
    if result.as_ref().is_some_and(|v|v["ok"]==true){return Ok(());}
    let value=result.unwrap_or(Value::Null);
    let code=value["error_code"].as_i64().unwrap_or(0);
    let reason=match code {400=>"Telegram не принял группу или тему. Проверьте ID и наличие бота в группе.",401=>"Telegram отклонил токен бота.",403=>"Бот не может писать в группу. Добавьте его и разрешите отправку сообщений.",429=>"Telegram ограничил частоту. Отправка будет повторена.",_=>"Telegram временно недоступен. Отправка будет повторена."};
    Err(DeliveryError{reason:reason.into(),retry_after:value["parameters"]["retry_after"].as_i64().unwrap_or(30).clamp(1,3600),permanent:matches!(code,400|401|403)})
}
fn http()->reqwest::Client {reqwest::Client::builder().timeout(std::time::Duration::from_secs(8)).redirect(reqwest::redirect::Policy::none()).build().expect("HTTP client")}

pub async fn send_test(pool:&Pool,token:&str,brand:&str,panel:&str)->Result<()> {
    let s=bc::load(pool).await?;let (chat,topic)=destination(&s)?;
    deliver(&http(),&format!("https://api.telegram.org/bot{token}/sendMessage"),&chat,topic,&message(pool,"test",&Value::Null,brand,panel).await?).await.map_err(|e|Error::bad(e.reason))
}
/// State transitions produce one alert and one recovery; identical samples do not flood the group.
pub async fn incident(pool:&Pool,key:&str,title:&str,problem:Option<&str>)->Result<()> {
    let s=bc::load(pool).await?;if !enabled(&s,"service"){return Ok(());}
    let mut tx=pool.begin().await?;
    sqlx::query("INSERT INTO service_incidents(key) VALUES($1) ON CONFLICT DO NOTHING").bind(key).execute(&mut *tx).await?;
    let old:Option<String>=sqlx::query_scalar("SELECT problem FROM service_incidents WHERE key=$1 FOR UPDATE").bind(key).fetch_one(&mut *tx).await?;
    if old.as_deref()!=problem {
        let revision:i64=sqlx::query_scalar("UPDATE service_incidents SET problem=$2,revision=revision+1,changed_at=now() WHERE key=$1 RETURNING revision").bind(key).bind(problem).fetch_one(&mut *tx).await?;
        sqlx::query("SELECT enqueue_team_notification('service',$1,$2)").bind(format!("incident:{key}:{revision}")).bind(json!({"title":title,"detail":problem.unwrap_or("Проблема устранена"),"recovered":problem.is_none()})).execute(&mut *tx).await?;
    }
    tx.commit().await?;Ok(())
}
pub async fn scan_nodes(pool:&Pool)->Result<()> {
    let s=bc::load(pool).await?;if !enabled(&s,"service"){return Ok(());}
    let rows=sqlx::query("SELECT n.id,n.name,n.engine_ok,n.status::text AS status,COALESCE(n.last_seen_at<nOW()-interval '3 minutes',n.created_at<now()-interval '10 minutes') AS stale,m.cpu_percent,m.ram_percent FROM nodes n LEFT JOIN LATERAL(SELECT cpu_percent,ram_percent FROM node_metrics WHERE node_id=n.id AND at>now()-interval '5 minutes' ORDER BY at DESC LIMIT 1)m ON true WHERE n.deleted_at IS NULL AND n.notify AND n.status::text<>'disabled'").fetch_all(pool).await?;
    for row in rows {
        let stale:bool=row.get("stale");let engine:bool=row.get("engine_ok");
        let cpu=row.try_get::<Option<f32>,_>("cpu_percent").ok().flatten().unwrap_or(0.0);let ram=row.try_get::<Option<f32>,_>("ram_percent").ok().flatten().unwrap_or(0.0);
        let problem=if stale {Some("Нет отчёта агента более трёх минут")}else if !engine{Some("Агент сообщает об ошибке VPN-движка")}else{None};
        let id:i64=row.get("id");let title=format!("Нода {}",row.get::<String,_>("name"));
        incident(pool,&format!("node:{id}"),&title,problem).await?;
        let previous:Option<String>=sqlx::query_scalar("SELECT problem FROM service_incidents WHERE key=$1").bind(format!("load:{id}")).fetch_optional(pool).await?.flatten();
        let overloaded=cpu>=95.0||ram>=95.0||(previous.is_some()&&(cpu>=85.0||ram>=85.0));
        incident(pool,&format!("load:{id}"),&title,if !stale&&overloaded{Some("Нагрузка CPU или памяти выше 95%. Восстановление — ниже 85%.")}else{None}).await?;
    }
    Ok(())
}
pub async fn tick(pool:&Pool,token:&str,brand:&str,panel:&str)->Result<usize> {
    dispatch(pool,&format!("https://api.telegram.org/bot{token}/sendMessage"),brand,panel).await
}
async fn dispatch(pool:&Pool,endpoint:&str,brand:&str,panel:&str)->Result<usize> {
    let s=bc::load(pool).await?;let target=destination(&s).ok();let http=http();let mut sent=0;
    // Transaction + SKIP LOCKED prevents two workers from sending the same queued event concurrently.
    for _ in 0..20 {
        let mut tx=pool.begin().await?;
        let row=sqlx::query("SELECT * FROM team_notifications WHERE status='pending' AND available_at<=now() ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED").fetch_optional(&mut *tx).await?;
        let Some(row)=row else {break};let id:i64=row.get("id");let kind:String=row.get("kind");
        let chat:String=row.get("chat_id");let topic:Option<i64>=row.get("thread_id");
        if !enabled(&s,&kind)||target.as_ref()!=Some(&(chat.clone(),topic)) {
            sqlx::query("UPDATE team_notifications SET status='canceled',error='Настройки уведомлений изменены' WHERE id=$1").bind(id).execute(&mut *tx).await?;tx.commit().await?;continue;
        }
        let content=message(pool,&kind,&row.get::<Value,_>("payload"),brand,panel).await;
        let outcome=match content{Ok(body)=>deliver(&http,endpoint,&chat,topic,&body).await,Err(e)=>Err(DeliveryError{reason:"Объект уведомления удалён или временно недоступен".into(),retry_after:30,permanent:matches!(e,Error::NotFound)})};
        match outcome {
            Ok(())=>{sqlx::query("UPDATE team_notifications SET status='sent',sent_at=now(),attempts=attempts+1,error=NULL WHERE id=$1").bind(id).execute(&mut *tx).await?;sent+=1;},
            Err(e)=>{
                let attempts:i32=row.get("attempts");let delay=e.retry_after.max(30*(1i64<<attempts.min(6)));
                sqlx::query("UPDATE team_notifications SET status=CASE WHEN $2 OR attempts>=7 THEN 'failed' ELSE 'pending' END,attempts=attempts+1,error=$3,available_at=now()+$4*interval '1 second' WHERE id=$1").bind(id).bind(e.permanent).bind(e.reason).bind(delay as f64).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
    }
    Ok(sent)
}

#[cfg(test)]
#[path="alerts_tests.rs"]
mod tests;

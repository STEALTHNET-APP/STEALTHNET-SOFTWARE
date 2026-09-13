use std::future::IntoFuture;
use super::*;
use std::{sync::{Arc,Mutex},collections::VecDeque};
use axum::{routing::post,Json,extract::State};

#[test]
fn alert_destination_validation() {
    for chat in ["123","@group","-0","-001","-9223372036854775809"] {assert!(destination(json!({"bot.alert_chat_id":chat}).as_object().unwrap()).is_err());}
    assert_eq!(destination(json!({"bot.alert_chat_id":"-1001234567890","bot.alert_thread_id":"42"}).as_object().unwrap()).unwrap(),("-1001234567890".into(),Some(42)));
    for value in [json!(2),json!("-1"),json!("+1")] {assert!(bc::validate(json!({"bot.alert_thread_id":value}).as_object().unwrap()).is_err());}
}

#[tokio::test]
#[ignore="requires isolated SN_TEST_DATABASE_URL"]
async fn team_alert_delivery_lifecycle()->Result<()> {
    let pool=crate::db::connect(&std::env::var("SN_TEST_DATABASE_URL").expect("isolated DB")).await?;
    let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await?;assert!(db.starts_with("sn_audit_"));
    let pending:i64=sqlx::query_scalar("SELECT count(*) FROM team_notifications WHERE status='pending'").fetch_one(&pool).await?;assert_eq!(pending,0);
    async fn setting(pool:&Pool,key:&str,value:Value)->Result<()> {sqlx::query("INSERT INTO settings(key,value) VALUES($1,$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value").bind(key).bind(value).execute(pool).await?;Ok(())}
    let original:Vec<(String,Value)>=sqlx::query_as("SELECT key,value FROM settings WHERE key='bot.admin_alerts_enabled' OR key LIKE 'bot.alert_%'").fetch_all(&pool).await?;
    for (key,value) in [("bot.admin_alerts_enabled",json!(false)),("bot.alert_chat_id",json!("-1001234567890")),("bot.alert_thread_id",json!("42"))] {setting(&pool,key,value).await?;}
    for category in ["payments","clients","tickets","service"] {setting(&pool,&format!("bot.alert_{category}_enabled"),json!(true)).await?;}
    let cid:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES('<b>Alert QA</b>',$1) RETURNING id").bind(uuid::Uuid::new_v4().to_string()).fetch_one(&pool).await?;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM team_notifications WHERE event_key=$1").bind(format!("client:{cid}")).fetch_one(&pool).await?;assert_eq!(count,0,"disabled notifications do not queue");
    setting(&pool,"bot.admin_alerts_enabled",json!(true)).await?;
    let cid2:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES('<b>Alert QA</b>',$1) RETURNING id").bind(uuid::Uuid::new_v4().to_string()).fetch_one(&pool).await?;
    let pid:i64=sqlx::query_scalar("INSERT INTO payments(client_id,amount_minor,currency,provider,status) VALUES($1,499,'USD','manual','pending') RETURNING id").bind(cid2).fetch_one(&pool).await?;
    for status in ["success","success","refunded","refunded"] {sqlx::query("UPDATE payments SET status=$2::payment_status WHERE id=$1").bind(pid).bind(status).execute(&pool).await?;}
    let tid:i64=sqlx::query_scalar("INSERT INTO tickets(client_id,subject) VALUES($1,'Help <script> & test') RETURNING id").bind(cid2).fetch_one(&pool).await?;
    for author in ["client","admin","client"] {sqlx::query("INSERT INTO ticket_messages(ticket_id,author_kind,body) VALUES($1,$2,'private body must not be forwarded')").bind(tid).bind(author).execute(&pool).await?;}
    let key=format!("qa:{}",uuid::Uuid::new_v4());
    for problem in [Some("Worker failed"),Some("Worker failed"),None,None] {incident(&pool,&key,"Worker",problem).await?;}
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM team_notifications WHERE status='pending'").fetch_one(&pool).await?;assert_eq!(count,7,"client + paid/refund + ticket/reply + incident/recovery");
    type Mock=Arc<Mutex<(Vec<Value>,VecDeque<Value>)>>;
    async fn receive(State(state):State<Mock>,Json(body):Json<Value>)->Json<Value>{let mut s=state.lock().unwrap();s.0.push(body);Json(s.1.pop_front().unwrap_or(json!({"ok":true,"result":{"message_id":1}})))}
    let state:Mock=Arc::new(Mutex::new((vec![],VecDeque::new())));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=format!("http://{}/sendMessage",listener.local_addr().unwrap());
    let server=tokio::spawn(axum::serve(listener,axum::Router::new().route("/sendMessage",post(receive)).with_state(state.clone())).into_future());
    let (a,b)=tokio::join!(dispatch(&pool,&endpoint,"Audit VPN","https://panel.example.test"),dispatch(&pool,&endpoint,"Audit VPN","https://panel.example.test"));assert_eq!(a?+b?,7,"concurrent workers deliver each event once");
    {let s=state.lock().unwrap();assert_eq!(s.0.len(),7);for body in &s.0 {assert_eq!(body["chat_id"],"-1001234567890");assert_eq!(body["message_thread_id"],42);assert!(body.get("parse_mode").is_none());assert!(!body["text"].as_str().unwrap().contains("private body"));}assert!(s.0.iter().any(|v|v["text"].as_str().unwrap().contains("4.99")));}
    async fn event(pool:&Pool,name:&str)->Result<i64>{let key=format!("qa:{name}:{}",uuid::Uuid::new_v4());sqlx::query("SELECT enqueue_team_notification('service',$1,'{}')").bind(&key).execute(pool).await?;Ok(sqlx::query_scalar("SELECT id FROM team_notifications WHERE event_key=$1").bind(key).fetch_one(pool).await?)}
    let retry=event(&pool,"retry").await?;
    state.lock().unwrap().1.push_back(json!({"ok":false,"error_code":429,"parameters":{"retry_after":1}}));
    assert_eq!(dispatch(&pool,&endpoint,"QA","").await?,0);
    let row:(String,i32,bool)=sqlx::query_as("SELECT status,attempts,available_at>now() FROM team_notifications WHERE id=$1").bind(retry).fetch_one(&pool).await?;assert_eq!(row,("pending".into(),1,true));
    sqlx::query("UPDATE team_notifications SET available_at=now() WHERE id=$1").bind(retry).execute(&pool).await?;
    assert_eq!(dispatch(&pool,&endpoint,"QA","").await?,1);
    let denied=event(&pool,"denied").await?;state.lock().unwrap().1.push_back(json!({"ok":false,"error_code":403,"description":"fake secret never stored"}));assert_eq!(dispatch(&pool,&endpoint,"QA","").await?,0);
    let row:(String,String)=sqlx::query_as("SELECT status,error FROM team_notifications WHERE id=$1").bind(denied).fetch_one(&pool).await?;assert_eq!(row.0,"failed");assert!(!row.1.contains("secret"));
    let changed=event(&pool,"changed").await?;setting(&pool,"bot.alert_chat_id",json!("-1009876543210")).await?;assert_eq!(dispatch(&pool,&endpoint,"QA","").await?,0);
    let status:String=sqlx::query_scalar("SELECT status FROM team_notifications WHERE id=$1").bind(changed).fetch_one(&pool).await?;assert_eq!(status,"canceled");
    let disabled=event(&pool,"disabled").await?;setting(&pool,"bot.admin_alerts_enabled",json!(false)).await?;assert_eq!(dispatch(&pool,&endpoint,"QA","").await?,0);
    let status:String=sqlx::query_scalar("SELECT status FROM team_notifications WHERE id=$1").bind(disabled).fetch_one(&pool).await?;assert_eq!(status,"canceled");
    setting(&pool,"bot.admin_alerts_enabled",json!(true)).await?;
    let node:i64=sqlx::query_scalar("INSERT INTO nodes(name,country_code,address,agent_secret_hash,last_seen_at,status) VALUES($1,'DE','127.0.0.1',decode('00','hex'),now()-interval '4 minutes','online') RETURNING id").bind(format!("alert-{}",uuid::Uuid::new_v4())).fetch_one(&pool).await?;
    scan_nodes(&pool).await?;scan_nodes(&pool).await?;
    let revision:i64=sqlx::query_scalar("SELECT revision FROM service_incidents WHERE key=$1").bind(format!("node:{node}")).fetch_one(&pool).await?;assert_eq!(revision,2);
    sqlx::query("UPDATE nodes SET last_seen_at=now() WHERE id=$1").bind(node).execute(&pool).await?;
    sqlx::query("INSERT INTO node_metrics(node_id,cpu_percent,ram_percent) VALUES($1,96,20)").bind(node).execute(&pool).await?;
    scan_nodes(&pool).await?;
    let problem:Option<String>=sqlx::query_scalar("SELECT problem FROM service_incidents WHERE key=$1").bind(format!("node:{node}")).fetch_one(&pool).await?;assert!(problem.is_none());
    for cpu in [90.0f32,80.0] {sqlx::query("UPDATE node_metrics SET cpu_percent=$2 WHERE node_id=$1").bind(node).bind(cpu).execute(&pool).await?;scan_nodes(&pool).await?;
        let problem:Option<String>=sqlx::query_scalar("SELECT problem FROM service_incidents WHERE key=$1").bind(format!("load:{node}")).fetch_one(&pool).await?;assert_eq!(problem.is_some(),cpu>=85.0);}
    sqlx::query("UPDATE nodes SET deleted_at=now() WHERE id=$1").bind(node).execute(&pool).await?;
    setting(&pool,"bot.admin_alerts_enabled",json!(false)).await?;dispatch(&pool,&endpoint,"QA","").await?;
    server.abort();
    sqlx::query("DELETE FROM settings WHERE key='bot.admin_alerts_enabled' OR key LIKE 'bot.alert_%'").execute(&pool).await?;for (key,value) in original {setting(&pool,&key,value).await?;}
    sqlx::query("UPDATE clients SET deleted_at=now() WHERE id=ANY($1)").bind(vec![cid,cid2]).execute(&pool).await?;
    Ok(())
}

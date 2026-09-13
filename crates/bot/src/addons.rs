use crate::*;

pub async fn show(c:&Ctx<'_>,chat:i64,client:i64,mid:Option<i64>,selected:Option<i64>)->Result<()> {
    let catalog=sn_core::addons::catalog(c.pool,client).await?;
    let currency=catalog["currency"].as_str().unwrap_or("USD");
    let items=catalog["items"].as_array().cloned().unwrap_or_default();
    let mut rows=Vec::new();
    let text=if let Some(id)=selected {
        let pack=items.iter().find(|p|p["id"].as_i64()==Some(id)).ok_or_else(||Error::bad("пакет недоступен для текущей подписки"))?;
        for cur in [currency,"XTR"] {
            if cur=="XTR" && currency=="XTR" && !rows.is_empty(){continue;}
            let amount=if cur==currency {pack["amount_minor"].as_i64()} else {pack["stars_minor"].as_i64()};
            if let Some(amount)=amount {
                for method in c.reg.enabled_for_currency(c.pool,cur).await {
                    rows.push(vec![Btn::Data(format!("{} · {}",method.title(),money::format_minor(amount,cur)),format!("am:{id}:{}:{cur}",method.id()))]);
                }
            }
        }
        let until=pack["expires_at"].as_str().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d|d.format("%d.%m.%Y %H:%M UTC").to_string()).unwrap_or_else(||"конца подписки".into());
        let mut body=format!("<b>{}</b>\n\nК оплате: <b>{}</b>\nДействует до: {until}\n\n{}\nТариф и срок подписки не меняются. Пакет добавится после подтверждения оплаты.",html_escape(pack["title"].as_str().unwrap_or("Докупка")),money::format_minor(pack["amount_minor"].as_i64().unwrap_or(0),currency),if pack["kind"]=="traffic" {"Трафик действует до ближайшего сброса или окончания подписки."} else {"Места для устройств действуют до конца текущего оплаченного срока и не продлеваются автоматически."});
        if rows.is_empty(){body.push_str("\n\nОплата пока не настроена. Обратитесь в поддержку.");}
        rows.push(vec![Btn::Data("Другие пакеты".into(),"addons".into())]);body
    } else {
        for pack in &items {
            rows.push(vec![Btn::Data(format!("{} · {}",pack["title"].as_str().unwrap_or("Докупка"),money::format_minor(pack["amount_minor"].as_i64().unwrap_or(0),currency)),format!("addon:{}",pack["id"]))]);
        }
        let mut body=if items.is_empty(){"<b>Докупки</b>\n\nДля текущей подписки нет доступных пакетов. Сначала активируйте тариф или обратитесь в поддержку.".into()}else{"<b>Расширить подписку</b>\n\nВыберите трафик или дополнительные места для устройств. Срок подписки остаётся прежним.\n\nУстройства — до конца оплаченного срока. Трафик — до ближайшего сброса или окончания подписки.".to_string()};
        for grant in catalog["grants"].as_array().into_iter().flatten() {
            let until=grant["expires_at"].as_str().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d|d.format("%d.%m.%Y").to_string()).unwrap_or_else(||"конца подписки".into());
            body.push_str(&format!("\n\nОплачено: +{} {} · {}",grant["quantity"],if grant["kind"]=="traffic"{"ГБ"}else{"устр."},if grant["pending"]==true{"ожидает действующую подписку".into()}else{format!("до {until}")}));
        }
        body
    };
    rows.push(vec![Btn::Data("В меню".into(),"menu".into())]);
    c.tg.screen(chat,mid,&text,None,Some(keyboard(rows))).await?;Ok(())
}

pub async fn pay(c:&Ctx<'_>,chat:i64,client:i64,mid:i64,package:i64,provider:&str,currency:&str)->Result<()> {
    let (id,invoice)=c.reg.create_addon_payment(c.pool,client,package,provider,currency).await?;
    let (amount,title):(i64,String)=sqlx::query_as("SELECT amount_minor,addon_snapshot->>'title' FROM payments WHERE id=$1").bind(id).fetch_one(c.pool).await?;
    let mut rows=Vec::new();
    if !invoice.pay_url.is_empty(){rows.push(vec![Btn::Url("Перейти к оплате".into(),invoice.pay_url)]);}
    rows.push(vec![Btn::Data("Проверить оплату".into(),format!("c:{id}"))]);
    rows.push(vec![Btn::Data("К пакетам".into(),"addons".into())]);
    c.tg.edit(chat,mid,&format!("<b>{}</b>\nСчёт №{id} · <b>{}</b>\n\n{}\n\nЕсли оплата поступит после окончания подписки, пакет сохранится и включится после её активации.",html_escape(&title),money::format_minor(amount,currency),html_escape(invoice.payload["instructions"].as_str().unwrap_or("После оплаты нажмите «Проверить оплату»."))),Some(keyboard(rows))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    #[ignore = "requires SN_TEST_DATABASE_URL pointing to an isolated sn_audit database"]
    async fn bot_addon_menu_respects_tariff_and_explicit_visibility() -> Result<()> {
        let url=std::env::var("SN_TEST_DATABASE_URL").expect("isolated DB required");
        let pool=sn_core::db::connect(&url).await?;
        let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await?;assert!(db.starts_with("sn_audit_"));
        let reg=Registry::from_env();
        let cfg=Config{database_url:url,api_bind:String::new(),sub_bind:String::new(),sub_public_url:"https://sub.example.test".into(),brand_name:"QA".into(),web_root:String::new(),sub_mode:"db".into(),panel_url:"https://panel.example.test".into(),sub_service_token:None};
        let (tg,server)=tg::transport_tests::mock(vec![json!({"ok":true,"result":{"message_id":10}});5]).await;
        let uid=9300000000+chrono::Utc::now().timestamp();let client=ensure_client(&pool,&json!({"id":uid,"username":"qa_addon_visibility"})).await?;
        let tariff:i64=sqlx::query_scalar("INSERT INTO tariffs(code,title,device_limit) VALUES($1,'Addon visibility QA',2) RETURNING id").bind(uuid::Uuid::new_v4().to_string()).fetch_one(&pool).await?;
        sqlx::query("UPDATE clients SET status='active' WHERE id=$1").bind(client).execute(&pool).await?;
        sqlx::query("UPDATE subscriptions SET tariff_id=NULL,device_limit=2,traffic_limit_bytes=107374182400,expires_at=now()+interval '30 days',canceled_at=NULL WHERE client_id=$1 AND is_current").bind(client).execute(&pool).await?;
        sqlx::query("INSERT INTO tariff_addons(tariff_id,kind,quantity,currency,amount_minor) VALUES($1,'devices',1,'USD',100),($1,'traffic',50,'USD',200)").bind(tariff).execute(&pool).await?;
        let c=Ctx{tg:&tg,pool:&pool,reg:&reg,cfg:&cfg,s:settings::View::from_pairs(&[])};
        show_menu(&c,uid,client,Some(10)).await?;
        sqlx::query("UPDATE subscriptions SET tariff_id=$2 WHERE client_id=$1 AND is_current").bind(client).bind(tariff).execute(&pool).await?;
        show_menu(&c,uid,client,Some(10)).await?;
        show(&c,uid,client,Some(10),None).await?;
        let hidden=Ctx{tg:&tg,pool:&pool,reg:&reg,cfg:&cfg,s:settings::View::from_pairs(&[("bot.menu_layout",json!([{"id":"addons","enabled":false}]))])};
        show_menu(&hidden,uid,client,Some(10)).await?;
        sqlx::query("UPDATE subscriptions SET traffic_limit_bytes=NULL WHERE client_id=$1 AND is_current").bind(client).execute(&pool).await?;
        show(&c,uid,client,Some(10),None).await?;
        let requests=server.await.unwrap();assert_eq!(requests.len(),5);
        let has_entry=|i:usize|requests[i].1["reply_markup"]["inline_keyboard"].as_array().unwrap().iter().flat_map(|r|r.as_array().unwrap()).any(|b|b["callback_data"]=="addons");
        assert!(!has_entry(0),"another tariff must not expose packages");assert!(has_entry(1),"next menu must show saved packages");assert!(!has_entry(3),"owner-hidden menu remains hidden");
        let buttons=requests[2].1["reply_markup"].to_string();assert!(buttons.contains("+1 устройство")&&buttons.contains("+50 ГБ трафика"));
        let unlimited=requests[4].1["reply_markup"].to_string();assert!(unlimited.contains("+1 устройство")&&!unlimited.contains("+50 ГБ трафика"));
        sqlx::query("UPDATE clients SET deleted_at=now() WHERE id=$1").bind(client).execute(&pool).await?;
        sqlx::query("UPDATE tariffs SET is_active=false,is_visible=false WHERE id=$1").bind(tariff).execute(&pool).await?;
        Ok(())
    }
    #[tokio::test]
    #[ignore = "requires SN_TEST_DATABASE_URL pointing to an isolated sn_audit database"]
    async fn bot_addon_checkout_and_stars_receipt() -> Result<()> {
        let url=std::env::var("SN_TEST_DATABASE_URL").expect("isolated DB required");
        let pool=sn_core::db::connect(&url).await?;
        let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await?;assert!(db.starts_with("sn_audit_"));
        let reg=Registry::from_env();reg.sync_to_db(&pool).await?;
        sqlx::query("UPDATE payment_providers SET is_enabled=true,enabled_currencies=ARRAY['USD'] WHERE id='manual'").execute(&pool).await?;
        let cfg=Config{database_url:url,api_bind:String::new(),sub_bind:String::new(),sub_public_url:"https://sub.example.test".into(),brand_name:"QA".into(),web_root:String::new(),sub_mode:"db".into(),panel_url:"https://panel.example.test".into(),sub_service_token:None};
        let (tg,server)=tg::transport_tests::mock(vec![json!({"ok":true,"result":{"message_id":10}});7]).await;
        let uid=9100000000+chrono::Utc::now().timestamp();let from=json!({"id":uid,"username":"qa_addon_bot"});let client=ensure_client(&pool,&from).await?;
        let tariff:i64=sqlx::query_scalar("INSERT INTO tariffs(code,title,device_limit) VALUES($1,'Bot addon QA',2) RETURNING id").bind(uuid::Uuid::new_v4().to_string()).fetch_one(&pool).await?;
        sqlx::query("UPDATE subscriptions SET tariff_id=$2,device_limit=2,expires_at=now()+interval '30 days',canceled_at=NULL WHERE client_id=$1 AND is_current").bind(client).bind(tariff).execute(&pool).await?;
        let package:i64=sqlx::query_scalar("INSERT INTO tariff_addons(tariff_id,kind,quantity,currency,amount_minor) VALUES($1,'devices',1,'USD',199) RETURNING id").bind(tariff).fetch_one(&pool).await?;
        let c=Ctx{tg:&tg,pool:&pool,reg:&reg,cfg:&cfg,s:settings::View::from_pairs(&[])};
        show(&c,uid,client,Some(10),None).await?;show(&c,uid,client,Some(10),Some(package)).await?;
        pay(&c,uid,client,10,package,"manual","USD").await?;
        let id:i64=sqlx::query_scalar("SELECT id FROM payments WHERE client_id=$1 AND kind='addon' ORDER BY id DESC LIMIT 1").bind(client).fetch_one(&pool).await?;
        check_payment(&tg,&pool,&reg,&cfg,uid,10,client,id).await?;
        reg.apply_outcome(&pool,"manual",&sn_payments::WebhookOutcome{payment_id:Some(id),provider_txid:None,external_event_id:None,status:sn_payments::PaymentStatus::Success,amount_minor:Some(199),currency:Some("USD".into()),error:None,raw:Value::Null}).await?;
        check_payment(&tg,&pool,&reg,&cfg,uid,10,client,id).await?;
        let star:i64=sqlx::query_scalar("INSERT INTO payments(client_id,tariff_id,kind,status,amount_minor,currency,provider,addon_snapshot) VALUES($1,$2,'addon','pending',20,'XTR','stars',$3) RETURNING id").bind(client).bind(tariff).bind(json!({"kind":"devices","quantity":1,"title":"+1 устройство","units":1})).fetch_one(&pool).await?;
        on_message(&c,&json!({"chat":{"id":uid},"from":from,"successful_payment":{"invoice_payload":star.to_string(),"total_amount":20,"currency":"XTR","telegram_payment_charge_id":format!("qa-stars-{star}")}})).await?;
        let clients_before:i64=sqlx::query_scalar("SELECT count(*) FROM clients").fetch_one(&pool).await?;
        for command in ["/start","/chatid"] {handle(&tg,&pool,&reg,&cfg,&settings::BotSettings,&json!({"message":{"chat":{"id":-1001234567890i64,"type":"supergroup"},"from":from,"text":command,"message_thread_id":42}})).await?;}
        let clients_after:i64=sqlx::query_scalar("SELECT count(*) FROM clients").fetch_one(&pool).await?;assert_eq!(clients_before,clients_after);
        let requests=server.await.unwrap();assert_eq!(requests.len(),7);assert_eq!(requests[6].1["message_thread_id"],42);assert!(requests[6].1["text"].as_str().unwrap().contains("-1001234567890"));
        assert!(requests[0].1["reply_markup"].to_string().contains(&format!("addon:{package}")));
        assert!(requests[1].1["reply_markup"].to_string().contains(&format!("am:{package}:manual:USD")));
        assert!(requests[3].1["text"].as_str().unwrap().contains("пока не поступила"));
        for index in [4,5]{assert!(requests[index].1["text"].as_str().unwrap().contains("Лимиты подписки увеличены"));}
        let limit:i32=sqlx::query_scalar("SELECT device_limit FROM subscriptions WHERE client_id=$1 AND is_current").bind(client).fetch_one(&pool).await?;assert_eq!(limit,4);
        sqlx::query("UPDATE clients SET deleted_at=now() WHERE id=$1").bind(client).execute(&pool).await?;
        sqlx::query("UPDATE tariffs SET is_active=false,is_visible=false WHERE id=$1").bind(tariff).execute(&pool).await?;
        Ok(())
    }
}

//! Реестр платёжных модулей: сборка, выбор по валюте и применение оплаты.

use sqlx::Row;
use std::sync::{Arc, RwLock};

use crate::{Invoice, InvoiceRequest, PaymentProvider, PaymentStatus, WebhookOutcome};
use sn_core::{Error, Pool, Result};

pub struct Registry {
    providers: Vec<Arc<dyn PaymentProvider>>,
    /// Когда последний раз читали настройки из БД.
    loaded_at: Arc<RwLock<Option<std::time::Instant>>>,
}

impl Registry {
    /// Собирает все известные модули.
    ///
    /// ЧТОБЫ ДОБАВИТЬ СВОЙ — реализуйте `PaymentProvider` в
    /// `providers/ваш_модуль.rs` и допишите одну строку сюда.
    /// Ненастроенные модули остаются в списке: администратор должен видеть,
    /// что доступно, но пока не подключено.
    pub fn from_env() -> Self {
        let providers: Vec<Arc<dyn PaymentProvider>> = vec![
            Arc::new(crate::providers::stars::TelegramStars::new()),
            Arc::new(crate::providers::cryptobot::CryptoBot::new()),
            Arc::new(crate::providers::generic_http::GenericHttp::new()),
            Arc::new(crate::providers::hosted::Hosted::new(crate::providers::hosted::Gateway::Platega)),
            Arc::new(crate::providers::hosted::Hosted::new(crate::providers::hosted::Gateway::RollyPay)),
            Arc::new(crate::providers::hosted::Hosted::new(crate::providers::hosted::Gateway::ParityPay)),
            Arc::new(crate::providers::manual::Manual::new()),
            // Arc::new(crate::providers::ваш_модуль::Ваш::new()),
        ];
        Self { providers, loaded_at: Arc::new(RwLock::new(None)) }
    }

    /// Забыть, что настройки уже читали: следующий вызов сходит в БД.
    /// Нужен сразу после сохранения формы, иначе панель ещё несколько
    /// секунд показывала бы прежний признак «настроен».
    pub fn invalidate(&self) {
        if let Ok(mut at) = self.loaded_at.write() {
            *at = None;
        }
    }

    /// Подтягивает настройки модулей из БД.
    ///
    /// Кэш на несколько секунд: ключи читаются на каждом платеже и на
    /// каждом вебхуке, а меняются раз в месяц. Зато после правки в панели
    /// перезапускать сервисы не нужно — включая бота, у которого свой
    /// экземпляр реестра.
    pub async fn refresh(&self, pool: &Pool) {
        const TTL: std::time::Duration = std::time::Duration::from_secs(10);
        if let Ok(at) = self.loaded_at.read() {
            if at.map(|t: std::time::Instant| t.elapsed() < TTL).unwrap_or(false) {
                return;
            }
        }

        let rows = sqlx::query("SELECT id, config FROM payment_providers")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

        for r in &rows {
            let id: String = r.get("id");
            let cfg: serde_json::Value = r.try_get("config").unwrap_or(serde_json::Value::Null);
            let Some(map) = cfg.as_object() else { continue };
            if let Some(p) = self.providers.iter().find(|p| p.id() == id) {
                if let Some(store) = p.settings() {
                    store.replace(map.clone());
                }
            }
        }

        if let Ok(mut at) = self.loaded_at.write() {
            *at = Some(std::time::Instant::now());
        }
    }

    pub fn all(&self) -> &[Arc<dyn PaymentProvider>] {
        &self.providers
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn PaymentProvider>> {
        self.providers.iter().find(|p| p.id() == id).cloned()
    }

    /// Модули, которыми реально можно заплатить в этой валюте.
    /// Ненастроенные отсеиваются: показать клиенту кнопку, ведущую в ошибку, — хуже,
    /// чем не показать её вовсе.
    pub fn for_currency(&self, currency: &str) -> Vec<Arc<dyn PaymentProvider>> {
        let mut list: Vec<_> = self
            .providers
            .iter()
            .filter(|p| p.is_configured() && p.currencies().iter().any(|c| c == currency))
            .cloned()
            .collect();
        list.sort_by_key(|p| p.sort_order());
        list
    }

    /// Модули, которыми клиент реально может заплатить в этой валюте.
    ///
    /// К проверке «ключи на месте» добавляем решения администратора из
    /// панели: выключенный модуль и снятая валюта не должны появляться
    /// кнопкой в боте.
    pub async fn enabled_for_currency(
        &self,
        pool: &Pool,
        currency: &str,
    ) -> Vec<Arc<dyn PaymentProvider>> {
        // Ключи могли поменять в панели минуту назад — читаем свежие,
        // иначе кнопка оплаты либо не появится, либо приведёт к ошибке.
        self.refresh(pool).await;

        let rows = sqlx::query(
            "SELECT id, sort_order FROM payment_providers
              WHERE is_enabled
                AND (enabled_currencies IS NULL OR $1 = ANY(enabled_currencies))
              ORDER BY sort_order, id",
        )
        .bind(currency)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut out = Vec::new();
        for r in &rows {
            let id: String = r.get("id");
            if let Some(p) = self
                .providers
                .iter()
                .find(|p| p.id() == id && p.is_configured() && p.currencies().iter().any(|c| c == currency))
            {
                out.push(p.clone());
            }
        }
        out
    }

    /// Записывает состав модулей в БД, чтобы панель показывала реальную картину.
    ///
    /// Обновляем только то, что является фактом о коде и окружении: список
    /// валют модуля и настроены ли ключи. Название, порядок и включённость
    /// задаёт администратор — перезаписывать их при каждом старте значит
    /// молча откатывать его настройки.
    pub async fn sync_to_db(&self, pool: &Pool) -> Result<()> {
        self.invalidate();
        self.refresh(pool).await;
        for p in &self.providers {
            sqlx::query(
                "INSERT INTO payment_providers (id, title, currencies, is_configured, sort_order, is_enabled, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, now())
                 ON CONFLICT (id) DO UPDATE SET
                    currencies = EXCLUDED.currencies,
                    is_configured = EXCLUDED.is_configured,
                    updated_at = now()",
            )
            .bind(p.id())
            .bind(p.title())
            .bind(p.currencies())
            .bind(p.is_configured())
            .bind(p.sort_order())
            .bind(p.enabled_by_default())
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// Создаёт платёж в БД и просит модуль выставить счёт.
    ///
    /// Порядок важен: сначала строка в `payments`, потом счёт у провайдера.
    /// Иначе вебхук может прийти раньше, чем мы узнаем о платеже.
    pub async fn create_payment(
        &self,
        pool: &Pool,
        provider_id: &str,
        client_id: i64,
        tariff_id: i64,
        period_days: i32,
        amount_minor: i64,
        currency: &str,
        description: &str,
        telegram_id: Option<i64>,
        promo_code_id: Option<i64>,
    ) -> Result<(i64, Invoice)> {
        self.create_payment_returning(pool,provider_id,client_id,tariff_id,period_days,amount_minor,currency,description,telegram_id,promo_code_id,None).await
    }

    pub async fn create_payment_returning(
        &self,
        pool: &Pool,
        provider_id: &str,
        client_id: i64,
        tariff_id: i64,
        period_days: i32,
        amount_minor: i64,
        currency: &str,
        description: &str,
        telegram_id: Option<i64>,
        promo_code_id: Option<i64>,
        return_url: Option<String>,
    ) -> Result<(i64, Invoice)> {
        sn_core::money::check_purchase_currency(pool, currency, provider_id).await?;
        // Ключи берём свежими: администратор мог поменять их только что.
        self.refresh(pool).await;

        let provider = self
            .get(provider_id)
            .ok_or_else(|| Error::bad(format!("нет модуля оплаты «{provider_id}»")))?;

        if !provider.is_configured() {
            return Err(Error::bad(format!(
                "способ оплаты «{}» не настроен",
                provider.title()
            )));
        }
        if !provider.currencies().iter().any(|c| c == currency) {
            return Err(Error::bad(format!(
                "«{}» не принимает {currency}",
                provider.title()
            )));
        }

        // Проверяем также прямые запросы: скрытой кнопки в боте недостаточно.
        let enabled: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM payment_providers WHERE id = $1 AND is_enabled
               AND (enabled_currencies IS NULL OR $2 = ANY(enabled_currencies)))",
        )
        .bind(provider_id)
        .bind(currency)
        .fetch_one(pool)
        .await?;
        if !enabled {
            return Err(Error::bad("этот способ оплаты отключён для выбранной валюты"));
        }
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tariffs WHERE id=$1 AND is_active)")
            .bind(tariff_id).fetch_one(pool).await?;
        if !active { return Err(Error::bad("тариф отключён или удалён")); }
        if period_days <= 0 || amount_minor < 0 {
            return Err(Error::bad("некорректный срок или сумма платежа"));
        }

        let mut tx=pool.begin().await?;
        let (charged,period_days)=if let Some(promo)=promo_code_id {
            crate::promo::reserve(&mut tx,client_id,tariff_id,promo,amount_minor,period_days,currency).await?
        } else { (amount_minor,period_days) };
        let payment_id: i64 = sqlx::query_scalar("INSERT INTO payments
            (client_id,tariff_id,kind,status,amount_minor,discount_minor,currency,period_days,provider,promo_code_id,expires_at)
            VALUES($1,$2,'purchase','pending',$3,$4,$5,$6,$7,$8,now()+interval '1 hour') RETURNING id")
            .bind(client_id).bind(tariff_id).bind(charged).bind(amount_minor-charged).bind(currency)
            .bind(period_days).bind(provider.id()).bind(promo_code_id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        let amount_minor=charged;
        if amount_minor==0 {
            self.apply_outcome(pool,provider_id,&WebhookOutcome{external_event_id:None,payment_id:Some(payment_id),provider_txid:None,status:PaymentStatus::Success,amount_minor:Some(0),currency:Some(currency.into()),error:None,raw:serde_json::Value::Null}).await?;
            return Ok((payment_id,Invoice{pay_url:String::new(),external_id:None,expires_in_minutes:0,payload:serde_json::json!({"free":true})}));
        }

        let req = InvoiceRequest {
            payment_id,
            client_id,
            telegram_id,
            amount_minor,
            currency: currency.to_string(),
            description: description.to_string(),
            return_url,
        };

        match provider.create_invoice(&req).await {
            Ok(invoice) => {
                sqlx::query(
                    "UPDATE payments
                        SET pay_url = $2,
                            provider_txid = COALESCE($3, provider_txid),
                            provider_payload = $4,
                            expires_at = now() + ($5 || ' minutes')::interval
                      WHERE id = $1",
                )
                .bind(payment_id)
                .bind(&invoice.pay_url)
                .bind(&invoice.external_id)
                .bind(&invoice.payload)
                .bind(invoice.expires_in_minutes.to_string())
                .execute(pool)
                .await?;
                Ok((payment_id, invoice))
            }
            Err(e) => {
                // Счёт не выставился — платёж не должен висеть в «ожидании» вечно.
                let _ = sqlx::query(
                    "UPDATE payments SET status = 'failed', error_message = $2 WHERE id = $1 AND status='pending'",
                )
                .bind(payment_id)
                .bind(e.to_string())
                .execute(pool)
                .await;
                Err(e)
            }
        }
    }

    /// Применяет результат вебхука: помечает платёж и продлевает подписку.
    ///
    /// Идемпотентно: повторный вызов с тем же событием ничего не изменит.
    /// На это опираются все провайдеры — они честно шлют дубликаты.
    pub async fn apply_outcome(&self, pool: &Pool, provider_id: &str, outcome: &WebhookOutcome) -> Result<Option<i64>> {
        let payment_id = match outcome.payment_id {
            Some(id)=>id,
            None=>sqlx::query_scalar("SELECT id FROM payments WHERE provider=$1 AND provider_txid=$2").bind(provider_id).bind(&outcome.provider_txid).fetch_optional(pool).await?.ok_or_else(||Error::bad("счёт ещё не зарегистрирован, повторите уведомление"))?,
        };

        let mut tx = pool.begin().await?;

        // Блокируем строку: два одновременных вебхука не должны продлить дважды.
        let row: Option<(String, i64, String, Option<i64>, Option<i32>, String, String)> = sqlx::query_as(
            "SELECT status::text, amount_minor, currency, client_id, period_days, provider, kind::text
               FROM payments WHERE id = $1 FOR UPDATE",
        )
        .bind(payment_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((status, expected_minor, currency, client_id, period_days, expected_provider, kind)) = row else {
            return Err(Error::bad(format!("платёж {payment_id} не найден")));
        };

        if expected_provider != provider_id {
            return Err(Error::bad("платёж принадлежит другому платёжному модулю"));
        }
        if provider_id!="manual" {
            let external:Option<String>=sqlx::query_scalar("SELECT provider_txid FROM payments WHERE id=$1").bind(payment_id).fetch_one(&mut *tx).await?;
            if external.is_some() && outcome.provider_txid.is_some() && external!=outcome.provider_txid {return Err(Error::bad("идентификатор счёта не совпадает"));}
        }
        if status == "refunded" || (status == "success" && outcome.status != PaymentStatus::Refunded) {
            // Уже обработан — тихо выходим, это нормальный дубликат.
            tx.commit().await?;
            return Ok(Some(payment_id));
        }

        if let Some(id)=client_id {
            sqlx::query("SELECT id FROM clients WHERE id=$1 FOR UPDATE").bind(id).fetch_one(&mut *tx).await?;
        }
        if outcome.status == PaymentStatus::Refunded {
            if outcome.currency.as_ref().is_some_and(|c|!c.eq_ignore_ascii_case(currency.trim())) {return Err(Error::bad("валюта возврата не совпадает"));}
            sqlx::query("UPDATE payments SET status='refunded',metadata=jsonb_set(metadata,'{refunded_minor}',to_jsonb(amount_minor)),error_message='Возврат подтверждён провайдером' WHERE id=$1").bind(payment_id).execute(&mut *tx).await?;
            sn_core::partners::adjust_refund(&mut tx,payment_id).await?;
            // Remove only addon entitlements. Subscription refunds remain visible for an administrator's access decision.
            if let Some(client)=client_id {
                sqlx::query("UPDATE subscription_addons SET expires_at=now(),ended_at=CASE WHEN activated_at IS NULL THEN now() ELSE ended_at END WHERE payment_id=$1 AND ended_at IS NULL").bind(payment_id).execute(&mut *tx).await?;
                sn_core::addons::maintain(&mut tx,client,false).await?;
            }
            tx.commit().await?;return Ok(Some(payment_id));
        }
        if outcome.status != PaymentStatus::Success {
            sqlx::query("UPDATE payments SET status = $2::payment_status, error_message = $3 WHERE id = $1")
                .bind(payment_id)
                .bind(outcome.status.as_db())
                .bind(&outcome.error)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(Some(payment_id));
        }

        if let Some(received_currency) = &outcome.currency {
            if !received_currency.eq_ignore_ascii_case(currency.trim()) {
                return Err(Error::bad("валюта платежа не совпадает с валютой счёта"));
            }
        }
        if provider_id == "http" && (outcome.amount_minor.is_none() || outcome.currency.is_none()) {
            return Err(Error::bad("платёжное уведомление не содержит сумму и валюту"));
        }
        // Сумма пришла меньше ожидаемой — не продлеваем, зовём человека.
        if let Some(paid) = outcome.amount_minor {
            if paid < expected_minor {
                sqlx::query(
                    "UPDATE payments SET status = 'failed',
                            error_message = $2 WHERE id = $1",
                )
                .bind(payment_id)
                .bind(format!(
                    "недоплата: пришло {paid}, ожидалось {expected_minor} {currency}"
                ))
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return Err(Error::bad("сумма платежа меньше ожидаемой"));
            }
        }

        sqlx::query(
            "UPDATE payments
                SET status = 'success', paid_at = now(),
                    provider_txid = COALESCE($2, provider_txid)
              WHERE id = $1",
        )
        .bind(payment_id)
        .bind(&outcome.provider_txid)
        .execute(&mut *tx)
        .await?;

        if let Some(id)=client_id { sn_core::addons::maintain(&mut tx,id,false).await?; }
        if kind == "addon" {
            let client=client_id.ok_or_else(||Error::bad("клиент платежа удалён"))?;
            sqlx::query("INSERT INTO subscription_addons(payment_id,client_id,kind,quantity) SELECT id,client_id,addon_snapshot->>'kind',(addon_snapshot->>'quantity')::bigint FROM payments WHERE id=$1 ON CONFLICT(payment_id) DO NOTHING")
                .bind(payment_id).execute(&mut *tx).await?;
            sn_core::addons::activate_pending(&mut tx,client).await?;
            sn_core::addons::maintain(&mut tx,client,false).await?;
        }
        // Продлеваем от большей из дат: если подписка ещё жива, срок прибавляется,
        // а если истекла — считаем от сегодня, а не от старой даты в прошлом.
        if let (Some(client_id), Some(days), false) = (client_id, period_days, kind == "addon") {
            sqlx::query(
                "UPDATE subscriptions
                    SET expires_at = GREATEST(COALESCE(expires_at, now()), now())
                                     + ($2 || ' days')::interval,
                        canceled_at = NULL
                  WHERE client_id = $1 AND is_current",
            )
            .bind(client_id)
            .bind(days.to_string())
            .execute(&mut *tx)
            .await?;

            sqlx::query("UPDATE subscriptions s SET tariff_id=p.tariff_id, device_limit=t.device_limit,
                traffic_limit_bytes=t.traffic_limit_bytes, reset_strategy=t.reset_strategy,
                traffic_reset_at=CASE t.reset_strategy WHEN 'day' THEN now()+interval '1 day'
                    WHEN 'week' THEN now()+interval '7 days' WHEN 'month' THEN now()+interval '1 month' ELSE NULL END
                FROM payments p JOIN tariffs t ON t.id=p.tariff_id
                WHERE p.id=$1 AND s.client_id=p.client_id AND s.is_current")
                .bind(payment_id).execute(&mut *tx).await?;
            sn_core::addons::restore_after_plan(&mut tx,client_id).await?;
            sqlx::query("INSERT INTO client_squads(client_id,squad_id)
                SELECT p.client_id,ts.squad_id FROM payments p JOIN tariff_squads ts ON ts.tariff_id=p.tariff_id
                WHERE p.id=$1 ON CONFLICT DO NOTHING")
                .bind(payment_id).execute(&mut *tx).await?;

            // Оплата возвращает доступ отключённым за неуплату.
            sqlx::query(
                "UPDATE clients SET status = 'active'
                  WHERE id = $1 AND status IN ('expired', 'limited')",
            )
            .bind(client_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query("UPDATE payments SET subscription_id = s.id FROM subscriptions s
                          WHERE s.client_id = $2 AND s.is_current AND payments.id = $1")
                .bind(payment_id)
                .bind(client_id)
                .execute(&mut *tx)
                .await?;
        }

        crate::promo::consume(&mut tx, payment_id).await?;
        sn_core::partners::accrue(&mut tx, payment_id).await?;
        tx.commit().await?;
        Ok(Some(payment_id))
    }

    pub async fn check_payment(&self,pool:&Pool,id:i64)->Result<()> {
        let row=sqlx::query("SELECT provider,provider_txid FROM payments WHERE id=$1 AND status IN ('pending','failed','canceled')").bind(id).fetch_optional(pool).await?;
        let Some(row)=row else{return Ok(())};self.refresh(pool).await;
        let provider:String=row.get("provider");
        if let (Some(p),Some(ext))=(self.get(&provider),row.get::<Option<String>,_>("provider_txid")) {
            if let Some(mut outcome)=p.poll_outcome(&ext).await? {
                if outcome.payment_id.is_some_and(|received|received!=id){return Err(Error::bad("счёт принадлежит другому заказу"));}
                outcome.payment_id=Some(id);self.apply_outcome(pool,&provider,&outcome).await?;
            }
        }
        Ok(())
    }

    /// Помечает просроченные счета. Вызывается по расписанию.
    pub async fn expire_stale(&self, pool: &Pool) -> Result<u64> {
        let res = sqlx::query(
            "UPDATE payments SET status = 'canceled', error_message = 'счёт истёк'
              WHERE status = 'pending' AND expires_at IS NOT NULL AND expires_at < now()",
        )
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }
}

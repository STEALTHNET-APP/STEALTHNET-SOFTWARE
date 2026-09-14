//! Модульные платежи.
//!
//! Ядро не знает ни одного платёжного сервиса в лицо. Оно умеет две вещи:
//! попросить модуль выставить счёт и передать модулю входящее уведомление.
//! Всё остальное — внутреннее дело модуля.
//!
//! Чтобы добавить свою платёжку, реализуйте [`PaymentProvider`] и
//! зарегистрируйте модуль в [`Registry::from_env`]. Подробности — в
//! `crates/payments/README.md`.

pub mod providers;
pub mod registry;
pub mod promo;

pub use registry::Registry;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sn_core::{Error, Result};
use std::sync::{Arc, RwLock};

/// Описание одного поля настроек модуля.
///
/// По этому списку панель сама рисует форму: ядру не нужно знать, что у
/// одной платёжки токен, а у другой пара «магазин + подпись».
#[derive(Debug, Clone, Serialize)]
pub struct SettingField {
    /// Ключ в конфиге модуля.
    pub key: &'static str,
    pub label: &'static str,
    /// Подсказка под полем: где взять значение.
    pub hint: &'static str,
    /// Секрет не показывается обратно в панель — только признак «задан».
    pub secret: bool,
    /// Без него модуль работать не может.
    pub required: bool,
    /// Что подставить, если администратор оставил поле пустым.
    pub default: Option<&'static str>,
}

impl SettingField {
    pub const fn secret(key: &'static str, label: &'static str, hint: &'static str) -> Self {
        Self { key, label, hint, secret: true, required: true, default: None }
    }
    pub const fn text(key: &'static str, label: &'static str, hint: &'static str) -> Self {
        Self { key, label, hint, secret: false, required: false, default: None }
    }
    pub const fn with_default(mut self, v: &'static str) -> Self {
        self.default = Some(v);
        self
    }
    pub const fn required(mut self, v: bool) -> Self {
        self.required = v;
        self
    }
}

/// Настройки модуля, заданные администратором в панели.
///
/// Живут в БД и перечитываются на ходу: поменяв ключ в панели, не нужно
/// перезапускать сервисы. Раньше значения брались только из окружения —
/// то есть править их мог лишь тот, у кого есть доступ к серверу.
#[derive(Clone, Default)]
pub struct Settings(Arc<RwLock<serde_json::Map<String, serde_json::Value>>>);

impl Settings {
    pub fn get(&self, key: &str) -> Option<String> {
        self.0
            .read()
            .ok()?
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn snapshot(&self)->serde_json::Map<String,serde_json::Value> { self.0.read().map(|v|v.clone()).unwrap_or_default() }

    /// Значение из панели, а если его нет — из окружения.
    ///
    /// Переменные окружения оставлены ради установок, настроенных до
    /// появления этой страницы: у них ничего не должно сломаться.
    pub fn get_or_env(&self, key: &str, env_key: &str) -> Option<String> {
        self.get(key)
            .or_else(|| std::env::var(env_key).ok().filter(|v| !v.trim().is_empty()))
    }

    pub fn replace(&self, map: serde_json::Map<String, serde_json::Value>) {
        if let Ok(mut w) = self.0.write() {
            *w = map;
        }
    }
}

/// Что именно оплачивают. Модулю это нужно для описания платежа
/// и для возврата к нужной подписке после оплаты.
#[derive(Debug, Clone, Serialize)]
pub struct InvoiceRequest {
    /// Внутренний id платежа — по нему мы найдём счёт, когда придёт вебхук.
    pub payment_id: i64,
    pub client_id: i64,
    /// Telegram ID, если платёж инициирован из бота (нужен Stars).
    pub telegram_id: Option<i64>,
    pub amount_minor: i64,
    pub currency: String,
    /// Человеческое описание: «PRO · 30 дней».
    pub description: String,
    /// Куда вернуть человека после оплаты.
    pub return_url: Option<String>,
}

/// Ответ модуля: как клиенту заплатить.
#[derive(Debug, Clone, Serialize)]
pub struct Invoice {
    /// Ссылка на оплату. Для Telegram Stars здесь ссылка на инвойс бота.
    pub pay_url: String,
    /// Идентификатор счёта на стороне провайдера, если он его выдал.
    pub external_id: Option<String>,
    /// Через сколько счёт протухнет.
    pub expires_in_minutes: i64,
    /// Что сохранить в `payments.provider_payload`.
    pub payload: serde_json::Value,
}

/// Итог разбора вебхука. Ядро само применит его идемпотентно.
#[derive(Debug, Clone)]
pub struct WebhookOutcome {
    /// Идентификатор события у провайдера — защита от повторов.
    pub external_event_id: Option<String>,
    /// Наш `payment_id`, если модуль смог его определить.
    pub payment_id: Option<i64>,
    /// Транзакция у провайдера.
    pub provider_txid: Option<String>,
    pub status: PaymentStatus,
    /// Фактически полученная сумма — сверяем с ожидаемой.
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub error: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    Pending,
    Success,
    Failed,
    Canceled,
    Refunded,
}

impl PaymentStatus {
    pub fn as_db(&self) -> &'static str {
        match self {
            PaymentStatus::Pending => "pending",
            PaymentStatus::Success => "success",
            PaymentStatus::Failed => "failed",
            PaymentStatus::Canceled => "canceled",
            PaymentStatus::Refunded => "refunded",
        }
    }
}

/// Контракт платёжного модуля.
///
/// Реализация обязана быть потокобезопасной: один экземпляр используется
/// всеми запросами.
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// Machine-имя, попадает в `payments.provider`. Менять нельзя —
    /// по нему находятся старые платежи.
    fn id(&self) -> &'static str;

    /// Only signature-verifying providers may receive public HTTP notifications.
    fn accepts_http_webhooks(&self) -> bool { false }

    /// Название для интерфейса и кнопки в боте.
    fn title(&self) -> &str;

    /// Валюты, которые модуль принимает (ISO-4217, либо `XTR` для Stars).
    /// Клиенту предлагаются только те способы, чья валюта есть в цене тарифа.
    fn currencies(&self) -> Vec<String>;

    /// Заданы ли ключи. Ненастроенный модуль виден в панели, но не предлагается
    /// клиентам — так администратор понимает, что осталось донастроить.
    fn is_configured(&self) -> bool;

    /// Начальное состояние при первой регистрации. Сохранённый выбор владельца
    /// при последующих запусках не меняется.
    fn enabled_by_default(&self) -> bool {
        false
    }

    /// Какие поля администратор заполняет в панели.
    ///
    /// Пустой список означает, что модулю настраивать нечего (например,
    /// подтверждение оплаты вручную).
    fn settings_schema(&self) -> Vec<SettingField> {
        vec![]
    }

    fn validate_settings(&self,_cfg:&serde_json::Map<String,serde_json::Value>)->Result<()> {Ok(())}

    /// Куда складывать значения из панели. Модуль отдаёт свой экземпляр
    /// [`Settings`], реестр наполняет его из БД при каждом обновлении.
    fn settings(&self) -> Option<Settings> {
        None
    }

    /// Порядок в списке способов оплаты.
    fn sort_order(&self) -> i32 {
        100
    }

    /// Выставить счёт.
    async fn create_invoice(&self, req: &InvoiceRequest) -> Result<Invoice>;

    /// Разобрать входящий вебхук.
    ///
    /// Модуль ОБЯЗАН проверить подпись и вернуть ошибку, если она не сходится:
    /// иначе кто угодно сможет «оплатить» подписку, отправив HTTP-запрос.
    async fn handle_webhook(
        &self,
        headers: &std::collections::HashMap<String, String>,
        body: &[u8],
    ) -> Result<WebhookOutcome>;

    /// Проверить статус счёта, если провайдер не шлёт вебхуки
    /// или уведомление потерялось. `None` — модуль не поддерживает опрос.
    async fn poll_status(&self, _external_id: &str) -> Result<Option<PaymentStatus>> {
        Ok(None)
    }

    /// Rich polling keeps amount and currency verification equivalent to a signed callback.
    async fn poll_outcome(&self,external_id:&str)->Result<Option<WebhookOutcome>> {
        Ok(self.poll_status(external_id).await?.map(|status|WebhookOutcome{external_event_id:None,payment_id:None,provider_txid:Some(external_id.into()),status,amount_minor:None,currency:None,error:None,raw:serde_json::Value::Null}))
    }

    /// Умеет ли модуль списывать без участия клиента.
    ///
    /// Это отдельная способность: Telegram Stars и криптоплатежи требуют
    /// подтверждения человеком и автопродление поддерживать не могут.
    /// Для них система вместо списания пришлёт напоминание со ссылкой.
    fn supports_recurring(&self) -> bool {
        false
    }

    /// Списать с ранее сохранённого метода оплаты.
    ///
    /// `provider_token` — то, что модуль сам сохранил при первой оплате.
    /// Карточных данных система не хранит и не передаёт.
    async fn charge_saved(
        &self,
        _req: &InvoiceRequest,
        _provider_token: &str,
    ) -> Result<WebhookOutcome> {
        Err(Error::bad("модуль не поддерживает автосписание"))
    }
}

/// Сумма в минорных единицах → в единицы валюты для API провайдера.
pub fn minor_to_units(amount_minor: i64, currency: &str) -> String {
    // Валюты без дробной части: у них «минорная единица» равна основной.
    const ZERO_DECIMAL: [&str; 4] = ["XTR", "JPY", "KRW", "VND"];
    if ZERO_DECIMAL.contains(&currency) {
        amount_minor.to_string()
    } else {
        format!("{}.{:02}", amount_minor / 100, amount_minor.abs() % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn перевод_в_единицы_валюты() {
        assert_eq!(minor_to_units(599, "USD"), "5.99");
        assert_eq!(minor_to_units(1000, "USD"), "10.00");
        assert_eq!(minor_to_units(5, "USD"), "0.05");
        // Telegram Stars и йены не имеют копеек — делить на 100 нельзя
        assert_eq!(minor_to_units(150, "XTR"), "150");
        assert_eq!(minor_to_units(1000, "JPY"), "1000");
    }
}

mod addons;

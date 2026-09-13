//! Сквады, тарифы и хосты: то, что связывает инбаунды ноды с подпиской клиента.
//!
//! Цепочка, которую надо держать в голове:
//! инбаунд ноды → сквад → тариф → подписка клиента → хост в его конфиге.
//! Разрыв в любом звене выглядит одинаково: «локация не появляется».

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::{AppState, CurrentAdmin};
use sn_core::{Error, Result};

pub fn catalog_routes() -> Router<AppState> {
    Router::new()
        .route("/api/squads", post(squad_create))
        .route(
            "/api/squads/{id}",
            axum::routing::patch(squad_update).delete(squad_delete),
        )
        .route("/api/squads/{id}/clients", post(squad_bulk_clients))
        .route("/api/tariffs", post(tariff_create))
        .route(
            "/api/tariffs/{id}",
            axum::routing::patch(tariff_update).delete(tariff_delete),
        )
        .route("/api/hosts", post(host_create))
        .route("/api/hosts/order", post(host_order))
        .route(
            "/api/hosts/{id}",
            axum::routing::patch(host_update).delete(host_delete),
        )
}

// ─────────────────────────── сквады ───────────────────────────

#[derive(Deserialize)]
struct SquadBody {
    name: Option<String>,
    description: Option<String>,
    /// Полный список тегов инбаундов. Заменяет прежний целиком.
    inbound_tags: Option<Vec<String>>,
    inbound_refs: Option<Vec<InboundRef>>,
}

#[derive(Deserialize)]
struct InboundRef { profile_id: i64, tag: String }

async fn set_squad_refs(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, squad_id: i64, refs: &[InboundRef]) -> Result<()> {
    sqlx::query("DELETE FROM squad_inbounds WHERE squad_id=$1").bind(squad_id).execute(&mut **tx).await?;
    for item in refs {
        let inbound: Option<i64> = sqlx::query_scalar("SELECT id FROM inbounds WHERE profile_id=$1 AND tag=$2")
            .bind(item.profile_id).bind(&item.tag).fetch_optional(&mut **tx).await?;
        let inbound = inbound.ok_or_else(|| Error::bad("выбранный инбаунд больше не существует"))?;
        sqlx::query("INSERT INTO squad_inbounds(squad_id,inbound_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(squad_id).bind(inbound).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn squad_create(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<SquadBody>,
) -> Result<Json<Value>> {
    let name = b.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::bad("название сквада не может быть пустым"));
    }

    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO squads (name, description) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(&b.description)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(refs) = &b.inbound_refs {
        set_squad_refs(&mut tx, id, refs).await?;
    } else if let Some(tags) = &b.inbound_tags {
        set_squad_inbounds(&mut tx, id, tags).await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'squad.create', 'squad', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn squad_update(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<SquadBody>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;

    let res = sqlx::query(
        "UPDATE squads SET name = COALESCE($2, name), description = COALESCE($3, description)
          WHERE id = $1",
    )
    .bind(id)
    .bind(b.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(&b.description)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    if let Some(refs) = &b.inbound_refs {
        set_squad_refs(&mut tx, id, refs).await?;
    } else if let Some(tags) = &b.inbound_tags {
        set_squad_inbounds(&mut tx, id, tags).await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id, payload)
         VALUES ('admin', $1, 'squad.update', 'squad', $2, $3)",
    )
    .bind(admin.id)
    .bind(id)
    .bind(json!({ "inbounds": b.inbound_tags }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Заменяет набор инбаундов сквада целиком.
///
/// Именно замена, а не добавление: иначе убрать инбаунд стало бы невозможно,
/// а это самая частая операция при переезде нод.
async fn set_squad_inbounds(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    squad_id: i64,
    tags: &[String],
) -> Result<()> {
    sqlx::query("DELETE FROM squad_inbounds WHERE squad_id = $1")
        .bind(squad_id)
        .execute(&mut **tx)
        .await?;

    if tags.is_empty() {
        return Ok(());
    }

    // Тег может встречаться в нескольких профилях — берём все совпадения:
    // так сквад продолжит работать после переноса инбаунда в другой профиль.
    let inserted = sqlx::query(
        "INSERT INTO squad_inbounds (squad_id, inbound_id)
         SELECT $1, i.id FROM inbounds i WHERE i.tag = ANY($2)
         ON CONFLICT DO NOTHING",
    )
    .bind(squad_id)
    .bind(tags)
    .execute(&mut **tx)
    .await?;

    if inserted.rows_affected() == 0 {
        return Err(Error::bad(
            "ни один из указанных инбаундов не найден — проверьте профиль",
        ));
    }
    Ok(())
}

async fn squad_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let members: i64 =
        sqlx::query_scalar("SELECT count(*) FROM client_squads WHERE squad_id = $1")
            .bind(id)
            .fetch_one(&st.pool)
            .await?;
    if members > 0 {
        return Err(Error::Conflict(format!(
            "в скваде {members} клиентов — они потеряют локации; сначала переведите их"
        )));
    }
    sqlx::query("DELETE FROM squads WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SquadClients {
    /// `add` — выдать сквад, `remove` — забрать.
    action: String,
    /// Пусто — применить ко всем клиентам.
    client_ids: Option<Vec<i64>>,
}

/// Массовая выдача сквада клиентам — иначе после создания новой локации
/// пришлось бы открывать каждого клиента вручную.
async fn squad_bulk_clients(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<SquadClients>,
) -> Result<Json<Value>> {
    let affected = match (b.action.as_str(), &b.client_ids) {
        ("add", Some(ids)) => {
            sqlx::query(
                "INSERT INTO client_squads (client_id, squad_id)
                 SELECT unnest($1::bigint[]), $2 ON CONFLICT DO NOTHING",
            )
            .bind(ids)
            .bind(id)
        }
        ("add", None) => sqlx::query(
            "INSERT INTO client_squads (client_id, squad_id)
             SELECT c.id, $1 FROM clients c WHERE c.deleted_at IS NULL
             ON CONFLICT DO NOTHING",
        )
        .bind(id),
        ("remove", Some(ids)) => {
            sqlx::query("DELETE FROM client_squads WHERE squad_id = $1 AND client_id = ANY($2)")
                .bind(id)
                .bind(ids)
        }
        ("remove", None) => {
            sqlx::query("DELETE FROM client_squads WHERE squad_id = $1").bind(id)
        }
        _ => return Err(Error::bad("action должен быть add или remove")),
    }
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "affected": affected.rows_affected() })))
}

// ─────────────────────────── тарифы ───────────────────────────

#[derive(Deserialize)]
struct PriceItem {
    period_days: i32,
    currency: String,
    amount_minor: i64,
}

#[derive(Deserialize)]
struct TariffBody {
    code: Option<String>,
    title: Option<String>,
    description: Option<String>,
    locales: Option<Value>,
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    badge: Option<Option<String>>,
    device_limit: Option<i32>,
    /// `null` — безлимит. Отсутствие поля — не менять.
    #[serde(default, deserialize_with = "crate::state::patch_field")]
    traffic_limit_bytes: Option<Option<i64>>,
    reset_strategy: Option<String>,
    is_active: Option<bool>,
    is_visible: Option<bool>,
    is_trial: Option<bool>,
    sort_order: Option<i32>,
    prices: Option<Vec<PriceItem>>,
    addons: Option<Vec<sn_core::addons::Package>>,
    /// Сквады, которые получит клиент этого тарифа.
    squad_names: Option<Vec<String>>,
}

fn validate_tariff_locales(v:&Value)->Result<()> {
    let object=v.as_object().ok_or_else(||Error::bad("Переводы тарифа должны быть объектом"))?;
    if object.keys().any(|k|k!="en"){return Err(Error::bad("У тарифа поддерживается перевод EN"));}
    if let Some(en)=object.get("en") {
        let en=en.as_object().ok_or_else(||Error::bad("Перевод EN должен быть объектом"))?;
        for (k,v) in en {let limit=match k.as_str(){"title"|"badge"=>200,"description"=>8000,_=>return Err(Error::bad("В переводе тарифа разрешены название, описание и тег"))};if v.as_str().is_none_or(|s|s.len()>limit){return Err(Error::bad("Слишком длинный или неверный перевод тарифа"));}}
    }
    Ok(())
}

async fn tariff_create(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<TariffBody>,
) -> Result<Json<Value>> {
    let code = b.code.as_deref().map(str::trim).unwrap_or_default().to_uppercase();
    if code.is_empty() {
        return Err(Error::bad("код тарифа обязателен"));
    }
    let title = b.title.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&code);

    if let Some(v)=&b.locales{validate_tariff_locales(v)?;}
    let mut tx = st.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tariffs (code, title, description, badge, device_limit,
                              traffic_limit_bytes, reset_strategy, is_active,
                              is_visible, is_trial, sort_order)
         VALUES ($1, $2, $3, $4, COALESCE($5, 1), $6, COALESCE($7::reset_strategy, 'month'),
                 COALESCE($8, true), COALESCE($9, true), COALESCE($10, false), COALESCE($11, 100))
         RETURNING id",
    )
    .bind(&code)
    .bind(title)
    .bind(&b.description)
    .bind(b.badge.clone().flatten())
    .bind(b.device_limit)
    .bind(b.traffic_limit_bytes.flatten())
    .bind(&b.reset_strategy)
    .bind(b.is_active)
    .bind(b.is_visible)
    .bind(b.is_trial)
    .bind(b.sort_order)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(v)=&b.locales {sqlx::query("UPDATE tariffs SET locales=$2 WHERE id=$1").bind(id).bind(v).execute(&mut *tx).await?;}
    if let Some(prices) = &b.prices {
        let cur = system_currency(&st.pool).await;
        set_prices(&mut tx, id, prices, &cur).await?;
    }
    if let Some(packs) = &b.addons {
        let cur = system_currency(&st.pool).await;
        sn_core::addons::set_catalog(&mut tx, id, packs, &cur).await?;
    }
    if let Some(squads) = &b.squad_names {
        set_tariff_squads(&mut tx, id, squads).await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'tariff.create', 'tariff', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn tariff_update(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<TariffBody>,
) -> Result<Json<Value>> {
    if b.code.as_deref().is_some_and(|s|s.trim().is_empty()) { return Err(Error::bad("код тарифа обязателен")); }
    if let Some(v)=&b.locales{validate_tariff_locales(v)?;}
    let mut tx = st.pool.begin().await?;

    let res = sqlx::query(
        "UPDATE tariffs SET
            title = COALESCE($2, title),
            description = COALESCE($3, description),
            badge = CASE WHEN $11 THEN $4 ELSE badge END,
            device_limit = COALESCE($5, device_limit),
            reset_strategy = COALESCE($6::reset_strategy, reset_strategy),
            is_active = COALESCE($7, is_active),
            is_visible = COALESCE($8, is_visible),
            is_trial = COALESCE($9, is_trial),
            sort_order = COALESCE($10, sort_order),
            code = COALESCE($12,code)
          WHERE id = $1",
    )
    .bind(id)
    .bind(b.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(&b.description)
    .bind(b.badge.clone().flatten())
    .bind(b.device_limit)
    .bind(&b.reset_strategy)
    .bind(b.is_active)
    .bind(b.is_visible)
    .bind(b.is_trial)
    .bind(b.sort_order)
    .bind(b.badge.is_some())
    .bind(b.code.as_deref().map(|s|s.trim().to_uppercase()))
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }

    // Лимит трафика трогаем, только если поле пришло: иначе нельзя было бы
    // отличить «не менять» от «сделать безлимитным».
    if let Some(limit) = b.traffic_limit_bytes {
        sqlx::query("UPDATE tariffs SET traffic_limit_bytes = $2 WHERE id = $1")
            .bind(id)
            .bind(limit)
            .execute(&mut *tx)
            .await?;
    }

    if let Some(v)=&b.locales {sqlx::query("UPDATE tariffs SET locales=$2 WHERE id=$1").bind(id).bind(v).execute(&mut *tx).await?;}
    if let Some(prices) = &b.prices {
        let cur = system_currency(&st.pool).await;
        set_prices(&mut tx, id, prices, &cur).await?;
    }
    if let Some(packs) = &b.addons {
        let cur = system_currency(&st.pool).await;
        sn_core::addons::set_catalog(&mut tx, id, packs, &cur).await?;
    }
    if let Some(squads) = &b.squad_names {
        set_tariff_squads(&mut tx, id, squads).await?;
    }

    sqlx::query(
        "INSERT INTO audit_log (actor_kind, actor_id, action, entity_type, entity_id)
         VALUES ('admin', $1, 'tariff.update', 'tariff', $2)",
    )
    .bind(admin.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Валюта, в которой работает сервис. Одна на всю систему.
pub async fn system_currency(pool: &sn_core::Pool) -> String {
    sn_core::money::service_currency(pool).await
}

/// Единицы Telegram Stars. Существуют только внутри Telegram и рублей
/// принять не могут, поэтому цена в них — законное исключение из правила
/// «одна валюта», а не вторая валюта системы.
const STARS: &str = "XTR";

async fn set_prices(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tariff_id: i64,
    prices: &[PriceItem],
    currency: &str,
) -> Result<()> {
    for p in prices {
        if p.period_days <= 0 {
            return Err(Error::bad("период должен быть больше нуля"));
        }
        if p.amount_minor < 0 {
            return Err(Error::bad("цена не может быть отрицательной"));
        }
        // Сервис работает в одной валюте. Чужая цена здесь означала бы
        // тариф, который в отчётах считается, а купить его нельзя.
        let cur = p.currency.trim().to_uppercase();
        if cur != currency && cur != STARS {
            return Err(Error::bad(format!(
                "валюта системы — {currency}. Допустима ещё только цена в {STARS} (Telegram Stars)"
            )));
        }
    }

    sqlx::query("DELETE FROM tariff_prices WHERE tariff_id = $1")
        .bind(tariff_id)
        .execute(&mut **tx)
        .await?;

    for p in prices {
        sqlx::query(
            "INSERT INTO tariff_prices (tariff_id, period_days, currency, amount_minor)
             VALUES ($1, $2, upper($3), $4)
             ON CONFLICT (tariff_id, period_days, currency)
             DO UPDATE SET amount_minor = EXCLUDED.amount_minor, is_active = true",
        )
        .bind(tariff_id)
        .bind(p.period_days)
        .bind(&p.currency)
        .bind(p.amount_minor)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Связь тарифа со сквадами. Именно она решает, какие локации увидит клиент.
async fn set_tariff_squads(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tariff_id: i64,
    names: &[String],
) -> Result<()> {
    sqlx::query("DELETE FROM tariff_squads WHERE tariff_id = $1")
        .bind(tariff_id)
        .execute(&mut **tx)
        .await?;

    if names.is_empty() {
        return Ok(());
    }

    let inserted = sqlx::query(
        "INSERT INTO tariff_squads (tariff_id, squad_id)
         SELECT $1, s.id FROM squads s WHERE s.name = ANY($2)",
    )
    .bind(tariff_id)
    .bind(names)
    .execute(&mut **tx)
    .await?;

    if inserted.rows_affected() == 0 {
        return Err(Error::bad("указанные сквады не найдены"));
    }

    // Уже купившим этот тариф выдаём новые сквады сразу: иначе изменение
    // подействует только на новых клиентов, а старые молча останутся без локации.
    sqlx::query(
        "INSERT INTO client_squads (client_id, squad_id)
         SELECT s.client_id, ts.squad_id
           FROM subscriptions s
           JOIN tariff_squads ts ON ts.tariff_id = s.tariff_id
          WHERE s.is_current AND s.tariff_id = $1
         ON CONFLICT DO NOTHING",
    )
    .bind(tariff_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn tariff_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    // Тариф с активными подписками не удаляем, а выключаем: иначе у людей
    // пропадёт название тарифа в карточке и в истории платежей.
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscriptions WHERE tariff_id = $1 AND is_current",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await?;

    if active > 0 {
        sqlx::query("UPDATE tariffs SET is_active = false, is_visible = false WHERE id = $1")
            .bind(id)
            .execute(&st.pool)
            .await?;
        return Ok(Json(
            json!({ "ok": true, "deactivated": true, "active_subscriptions": active }),
        ));
    }

    let res = sqlx::query("DELETE FROM tariffs WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected()==0 { return Err(Error::NotFound); }
    Ok(Json(json!({ "ok": true, "deleted": true })))
}

// ─────────────────────────── хосты ───────────────────────────

#[derive(Deserialize)]
struct HostBody {
    remark: Option<String>,
    address: Option<String>,
    port: Option<i32>,
    inbound_tag: Option<String>,
    profile_id: Option<i64>,
    security: Option<String>,
    // Двойная обёртка нарочно. Правка хоста приходит и целиком (из формы),
    // и по одному полю (переключатель в списке), поэтому «поля нет» обязано
    // значить «не трогать». А `null` — это «очистить»: стерев SNI в форме,
    // человек получал «Сохранено», но значение оставалось прежним, потому
    // что пустое и непереданное были для сервера одним и тем же.
    #[serde(default, deserialize_with = "двойной_option")]
    sni: Option<Option<String>>,
    #[serde(default, deserialize_with = "двойной_option")]
    fingerprint: Option<Option<String>>,
    #[serde(default, deserialize_with = "двойной_option")]
    alpn: Option<Option<String>>,
    #[serde(default, deserialize_with = "двойной_option")]
    path: Option<Option<String>>,
    #[serde(default, deserialize_with = "двойной_option")]
    public_key: Option<Option<String>>,
    #[serde(default, deserialize_with = "двойной_option")]
    short_id: Option<Option<String>>,
    sort_order: Option<i32>,
    is_enabled: Option<bool>,
    options: Option<Value>,
    #[serde(default, deserialize_with = "двойной_option")]
    host_header: Option<Option<String>>,
}

/// Отличить отсутствующее поле от переданного `null`.
///
/// Обычный `Option` схлопывает эти случаи в один, и очистить поле
/// частичным обновлением становится невозможно.
fn двойной_option<'de, D, T>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
}

/// Значение для записи: пустая строка — то же очищение, что и `null`.
/// Человек чаще стирает содержимое поля, чем удаляет само поле.
fn чистое(v: &Option<Option<String>>) -> Option<String> {
    v.clone().flatten().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

async fn validate_host_body(st:&AppState,b:&HostBody)->Result<()> {
    if b.port.is_some_and(|p|!(1..=65535).contains(&p)){return Err(Error::bad("Порт должен быть от 1 до 65535"))}
    if b.remark.as_deref().is_some_and(|s|s.trim().is_empty()||s.chars().count()>200){return Err(Error::bad("Название хоста: от 1 до 200 символов"))}
    if b.address.as_deref().is_some_and(|s|s.trim().is_empty()||s.len()>253||s.chars().any(char::is_whitespace)||s.contains('/')){return Err(Error::bad("Укажите домен или IP без схемы и пути"))}
    if b.security.as_deref().is_some_and(|s|!matches!(s,"none"|"tls"|"reality")){return Err(Error::bad("Неизвестный режим защиты"))}
    if let Some(options)=&b.options {
        sn_sub::overrides::validate_host(options,false).map_err(Error::bad)?;
        if let Some(id)=options.get("xray_template_id"){sn_sub::overrides::validate_templates(&st.pool,&json!({"xray_json":id})).await?;}
        for (key,table) in [("node_ids","nodes"),("exclude_squad_ids","squads")] {
            if let Some(ids)=options[key].as_array(){for id in ids {
                let exists:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=$1)")).bind(id.as_i64().unwrap()).fetch_one(&st.pool).await?;
                if !exists{return Err(Error::bad("Выбранная нода или сквад больше не существует"))}
            }}
        }
    }
    Ok(())
}

async fn resolve_host_inbound(st: &AppState, b: &HostBody) -> Result<Option<i64>> {
    let Some(tag) = b.inbound_tag.as_deref() else { return Ok(None) };
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM inbounds WHERE tag=$1 AND ($2::bigint IS NULL OR profile_id=$2)")
        .bind(tag).bind(b.profile_id).fetch_all(&st.pool).await?;
    match ids.as_slice() {
        [id] => Ok(Some(*id)),
        [] => Err(Error::bad("инбаунд не найден в выбранном профиле")),
        _ => Err(Error::bad("этот тег есть в нескольких профилях — укажите профиль")),
    }
}

async fn host_create(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<HostBody>,
) -> Result<Json<Value>> {
    validate_host_body(&st, &b).await?;
    let remark = b.remark.as_deref().map(str::trim).unwrap_or_default();
    let address = b.address.as_deref().map(str::trim).unwrap_or_default();
    let tag = b.inbound_tag.as_deref().unwrap_or_default();

    if remark.is_empty() || address.is_empty() || tag.is_empty() {
        return Err(Error::bad("нужны ремарка, адрес и инбаунд"));
    }
    let security = b.security.as_deref().unwrap_or("reality");

    let inbound_id = resolve_host_inbound(&st, &b).await?
        .ok_or_else(|| Error::bad("выберите инбаунд"))?;
    if security == "reality" {
        let key: Option<String> = sqlx::query_scalar("SELECT public_key FROM inbounds WHERE id=$1")
            .bind(inbound_id).fetch_one(&st.pool).await?;
        if key.as_deref().is_none_or(str::is_empty) {
            return Err(Error::bad("в выбранном инбаунде нет ключа Reality — проверьте приватный ключ в профиле"));
        }
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO hosts (sort_order, remark, address, port, inbound_id, security,
                            sni, fingerprint, alpn, path, public_key, short_id, is_enabled, options, host_header)
         VALUES (COALESCE($1, 100), $2, $3, COALESCE($4, 443), $5, $6::host_security,
                 $7, $8, $9, $10, $11, $12, COALESCE($13, true), $14, $15)
         RETURNING id",
    )
    .bind(b.sort_order)
    .bind(remark)
    .bind(address)
    .bind(b.port)
    .bind(inbound_id)
    .bind(security)
    // Через `чистое`, а не напрямую: пустая строка из формы должна лечь
    // в базу как NULL, иначе «пусто» и «пустая строка» начнут вести себя
    // по-разному в выдаче ссылок.
    .bind(чистое(&b.sni))
    .bind(чистое(&b.fingerprint))
    .bind(чистое(&b.alpn))
    .bind(чистое(&b.path))
    .bind(чистое(&b.public_key))
    .bind(чистое(&b.short_id))
    .bind(b.is_enabled)
    .bind(b.options.clone().unwrap_or_else(||json!({})))
    .bind(чистое(&b.host_header))
    .fetch_one(&st.pool)
    .await?;

    Ok(Json(json!({ "id": id })))
}

async fn host_update(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<HostBody>,
) -> Result<Json<Value>> {
    validate_host_body(&st, &b).await?;
    let inbound_id = resolve_host_inbound(&st, &b).await?;

    let res = sqlx::query(
        "UPDATE hosts SET
            remark = COALESCE($2, remark), address = COALESCE($3, address),
            port = COALESCE($4, port), inbound_id = COALESCE($5, inbound_id),
            security = COALESCE($6::host_security, security),
            -- Пара «передали ли» + «что»: COALESCE тут не годится, он не
            -- умеет записать NULL по желанию — а очистка поля это ровно оно.
            sni         = CASE WHEN $7  THEN $8  ELSE sni         END,
            fingerprint = CASE WHEN $9  THEN $10 ELSE fingerprint END,
            alpn        = CASE WHEN $11 THEN $12 ELSE alpn        END,
            path        = CASE WHEN $13 THEN $14 ELSE path        END,
            public_key  = CASE WHEN $15 THEN $16 ELSE public_key  END,
            short_id    = CASE WHEN $17 THEN $18 ELSE short_id    END,
            sort_order = COALESCE($19, sort_order), is_enabled = COALESCE($20, is_enabled),
            options = COALESCE($21, options),
            host_header = CASE WHEN $22 THEN $23 ELSE host_header END,
            updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(b.remark.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(b.address.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(b.port)
    .bind(inbound_id)
    .bind(&b.security)
    .bind(b.sni.is_some())
    .bind(чистое(&b.sni))
    .bind(b.fingerprint.is_some())
    .bind(чистое(&b.fingerprint))
    .bind(b.alpn.is_some())
    .bind(чистое(&b.alpn))
    .bind(b.path.is_some())
    .bind(чистое(&b.path))
    .bind(b.public_key.is_some())
    .bind(чистое(&b.public_key))
    .bind(b.short_id.is_some())
    .bind(чистое(&b.short_id))
    .bind(b.sort_order)
    .bind(b.is_enabled)
    .bind(&b.options)
    .bind(b.host_header.is_some())
    .bind(чистое(&b.host_header))
    .execute(&st.pool)
    .await?;

    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn host_delete(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let res = sqlx::query("DELETE FROM hosts WHERE id = $1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct HostOrder { ids: Vec<i64> }

async fn host_order(_a: CurrentAdmin, State(st): State<AppState>, Json(b): Json<HostOrder>) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    // Serialize creates/deletes with the complete-list check and the update.
    sqlx::query("LOCK TABLE hosts IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let current: Vec<i64> = sqlx::query_scalar("SELECT id FROM hosts ORDER BY id").fetch_all(&mut *tx).await?;
    let mut sorted = b.ids.clone(); sorted.sort_unstable();
    if sorted != current { return Err(Error::bad("Список хостов изменился. Обновите страницу и повторите сортировку.")); }
    sqlx::query("UPDATE hosts h SET sort_order=o.pos::integer, updated_at=now() FROM unnest($1::bigint[]) WITH ORDINALITY AS o(id,pos) WHERE h.id=o.id")
        .bind(&b.ids).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

#[cfg(test)]
mod правка_хоста {
    use super::*;

    /// Разбор тела так, как его присылает панель.
    fn тело(json: &str) -> HostBody {
        serde_json::from_str(json).expect("тело хоста разбирается")
    }

    #[test]
    fn пустое_значение_отличается_от_непереданного() {
        // Ровно тот баг: стирание SNI в форме давало «Сохранено», а
        // значение оставалось прежним — сервер не отличал «очистить» от
        // «не трогать» и оба случая пропускал через COALESCE.
        let очистка = тело(r#"{"sni": null}"#);
        assert!(очистка.sni.is_some(), "поле передали — трогаем");
        assert_eq!(чистое(&очистка.sni), None, "записываем NULL");

        let не_трогаем = тело(r#"{"is_enabled": true}"#);
        assert!(не_трогаем.sni.is_none(), "поля нет — не трогаем вовсе");
    }

    #[test]
    fn пробелы_считаются_очисткой() {
        // Человек чаще выделяет содержимое и стирает, чем удаляет поле;
        // пробел, оставшийся после этого, не должен попадать в ссылку.
        let b = тело(r#"{"sni": "   "}"#);
        assert!(b.sni.is_some());
        assert_eq!(чистое(&b.sni), None);
    }

    #[test]
    fn значение_сохраняется_обрезанным() {
        let b = тело(r#"{"sni": " example.com "}"#);
        assert_eq!(чистое(&b.sni).as_deref(), Some("example.com"));
    }

    #[test]
    fn правило_одинаково_для_всех_очищаемых_полей() {
        // Иначе однажды окажется, что SNI стирается, а alpn — нет.
        let b = тело(r#"{"sni":null,"fingerprint":null,"alpn":null,
                         "path":null,"public_key":null,"short_id":null}"#);
        for (имя, поле) in [
            ("sni", &b.sni), ("fingerprint", &b.fingerprint), ("alpn", &b.alpn),
            ("path", &b.path), ("public_key", &b.public_key), ("short_id", &b.short_id),
        ] {
            assert!(поле.is_some(), "{имя}: передано, должно очищаться");
            assert_eq!(чистое(поле), None, "{имя}");
        }
    }
}

#[cfg(test)]mod language_tests {
    use super::*;
    #[test]fn tariff_translation_cannot_change_entitlements(){
        assert!(validate_tariff_locales(&json!({"en":{"title":"Family","description":"For your family","badge":"Popular"}})).is_ok());
        assert!(validate_tariff_locales(&json!({"en":{"device_limit":100}})).is_err());
        assert!(validate_tariff_locales(&json!({"en":{"title":"a".repeat(201)}})).is_err());
        assert!(validate_tariff_locales(&json!({"en":null})).is_err());
    }
}

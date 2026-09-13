//! Authenticated profile library, impact previews and isolated rehearsals.
use crate::state::{AppState, CurrentAdmin};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sn_core::{profile_workflow as model, Error, Result};
use sqlx::Row;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/profile-templates",
            get(templates).post(template_create),
        )
        .route(
            "/api/profile-templates/{id}",
            axum::routing::patch(template_save).delete(template_delete),
        )
        .route("/api/profile-templates/import", post(import_github))
        .route("/api/profile-templates/sanitize", post(sanitize))
        .route("/api/profiles/prepare", post(prepare))
        .route("/api/profiles/{id}/impact", post(impact))
        .route("/api/profiles/{id}/history", get(history))
        .route("/api/profiles/{id}/status", get(status))
        .route(
            "/api/profiles/{id}/trial",
            get(trial_get).post(trial_create).delete(trial_finish),
        )
}
#[derive(Deserialize)]
pub struct Config {
    pub config: Value,
}
fn bounded(config: &Value) -> Result<()> {
    model::validate_shape(config).map_err(Error::bad)
}
async fn prepare(_a: CurrentAdmin, Json(b): Json<Config>) -> Result<Json<Value>> {
    bounded(&b.config)?;
    let generated = model::generate_missing(&b.config);
    let (config, added) = sn_core::service_parts::ensure_service_parts(&generated);
    Ok(Json(
        json!({"config":config,"added":added,"missing":model::missing_parameters(&config)}),
    ))
}
async fn sanitize(_a: CurrentAdmin, Json(b): Json<Config>) -> Result<Json<Value>> {
    bounded(&b.config)?;
    let config = model::sanitize_template(&b.config);
    Ok(Json(
        json!({"config":config,"parameters":model::missing_parameters(&config)}),
    ))
}
async fn templates(_a: CurrentAdmin, State(st): State<AppState>) -> Result<Json<Value>> {
    let rows: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM profile_templates t ORDER BY name,id")
            .fetch_all(&st.pool)
            .await?;
    Ok(Json(json!(rows)))
}
#[derive(Deserialize)]
struct Template {
    name: String,
    #[serde(default)]
    description_ru: String,
    #[serde(default)]
    description_en: String,
    #[serde(default)]
    author: String,
    source_url: Option<String>,
    source_revision: Option<String>,
    config: Value,
    version: Option<i32>,
}
fn validate_template(b: &Template) -> Result<Value> {
    bounded(&b.config)?;
    if b.name.trim().is_empty()
        || b.name.chars().count() > 160
        || b.description_ru.chars().count() > 4000
        || b.description_en.chars().count() > 4000
        || b.author.chars().count() > 160
    {
        return Err(Error::bad("Template metadata is too long or name is empty / Проверьте название и описание шаблона"));
    }
    if let Some(url) = &b.source_url {
        github_url(url)?;
    }
    if b.source_revision.as_ref().is_some_and(|r| r.len() > 160) {
        return Err(Error::bad("Invalid revision / Неверная ревизия"));
    }
    Ok(model::sanitize_template(&b.config))
}
async fn template_create(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Json(b): Json<Template>,
) -> Result<Json<Value>> {
    let cfg = validate_template(&b)?;
    let id:i64=sqlx::query_scalar("INSERT INTO profile_templates(name,description_ru,description_en,author,source_url,source_revision,config) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(b.name.trim()).bind(b.description_ru).bind(b.description_en).bind(b.author).bind(b.source_url).bind(b.source_revision).bind(cfg).fetch_one(&st.pool).await?;
    audit(&st, a.id, "template.create", id).await?;
    Ok(Json(json!({"id":id,"version":1})))
}
async fn template_save(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<Template>,
) -> Result<Json<Value>> {
    let cfg = validate_template(&b)?;
    let version:i32=sqlx::query_scalar("UPDATE profile_templates SET name=$2,description_ru=$3,description_en=$4,author=$5,source_url=$6,source_revision=$7,config=$8,version=version+1,updated_at=now() WHERE id=$1 AND version=$9 RETURNING version")
        .bind(id).bind(b.name.trim()).bind(b.description_ru).bind(b.description_en).bind(b.author).bind(b.source_url).bind(b.source_revision).bind(cfg).bind(b.version).fetch_optional(&st.pool).await?.ok_or_else(||Error::bad("Template changed; reopen it / Шаблон изменён, откройте его заново"))?;
    audit(&st, a.id, "template.save", id).await?;
    Ok(Json(json!({"id":id,"version":version})))
}
async fn template_delete(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    sqlx::query("DELETE FROM profile_templates WHERE id=$1")
        .bind(id)
        .execute(&st.pool)
        .await?;
    audit(&st, a.id, "template.delete", id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn audit(st: &AppState, actor: i64, action: &str, id: i64) -> Result<()> {
    sqlx::query("INSERT INTO audit_log(actor_kind,actor_id,action,entity_type,entity_id,payload) VALUES('admin',$1,$2,'config_profile',$3,'{}')").bind(actor).bind(action).bind(id).execute(&st.pool).await?;
    Ok(())
}
/// A pinned commit on the fixed raw GitHub origin. No redirects, credentials,
/// arbitrary origins, query strings or user-controlled DNS targets.
fn github_url(input: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(input)
        .map_err(|_| Error::bad("Invalid GitHub URL / Неверный адрес GitHub"))?;
    let parts: Vec<_> = url.path().trim_start_matches('/').split('/').collect();
    if url.scheme() != "https"
        || url.host_str() != Some("raw.githubusercontent.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || parts.len() < 4
        || parts[0].is_empty()
        || parts[1].is_empty()
        || parts[2].len() != 40
        || !parts[2].bytes().all(|c| c.is_ascii_hexdigit())
        || parts
            .iter()
            .any(|s| s.is_empty() || s.contains('%') || *s == "..")
    {
        return Err(Error::bad("Use https://raw.githubusercontent.com/OWNER/REPO/40-CHAR-COMMIT/path.json / Укажите raw-ссылку с полным commit из 40 символов"));
    }
    Ok(url)
}
#[derive(Deserialize)]
struct Import {
    url: String,
}
async fn import_github(_a: CurrentAdmin, Json(b): Json<Import>) -> Result<Json<Value>> {
    use sha2::{Digest, Sha256};
    let url = github_url(&b.url)?;
    let revision = url.path().split('/').nth(3).unwrap_or("").to_string();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|_| Error::bad("HTTP client unavailable"))?;
    let mut res = client
        .get(url)
        .send()
        .await
        .map_err(|_| Error::bad("GitHub unavailable / GitHub не отвечает"))?;
    if !res.status().is_success() {
        return Err(Error::bad(
            "GitHub returned an error / GitHub вернул ошибку",
        ));
    }
    if res
        .content_length()
        .is_some_and(|n| n > model::MAX_CONFIG_BYTES as u64)
    {
        return Err(Error::bad("File exceeds 512 KiB / Файл больше 512 КиБ"));
    }
    let mut bytes = vec![];
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|_| Error::bad("GitHub read failed / Ошибка чтения GitHub"))?
    {
        if bytes.len() + chunk.len() > model::MAX_CONFIG_BYTES {
            return Err(Error::bad("File exceeds 512 KiB / Файл больше 512 КиБ"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let config: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::bad("The file must contain JSON / В файле должен быть JSON"))?;
    bounded(&config)?;
    Ok(Json(
        json!({"config":config,"source_url":b.url,"source_revision":revision,"sha256":hex::encode(Sha256::digest(&bytes))}),
    ))
}
async fn impact(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<Config>,
) -> Result<Json<Value>> {
    bounded(&b.config)?;
    let old = sqlx::query("SELECT config,version FROM config_profiles WHERE id=$1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;
    let config = sn_core::service_parts::ensure_service_parts(&b.config).0;
    let paths = model::changed_paths(&old.get::<Value, _>("config"), &config);
    let tags: Vec<String> = sn_core::xray_check::check_structure(&config)
        .inbounds
        .into_iter()
        .filter(|i| !i.is_service)
        .map(|i| i.tag)
        .collect();
    let removed: Vec<String> =
        sqlx::query_scalar("SELECT tag FROM inbounds WHERE profile_id=$1 AND tag<>ALL($2)")
            .bind(id)
            .bind(&tags)
            .fetch_all(&st.pool)
            .await?;
    let hosts:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',h.id,'name',h.remark,'inbound',i.tag,'removed',i.tag=ANY($2)) FROM hosts h JOIN inbounds i ON i.id=h.inbound_id WHERE i.profile_id=$1").bind(id).bind(&removed).fetch_all(&st.pool).await?;
    let squads:Vec<Value>=sqlx::query_scalar("SELECT DISTINCT jsonb_build_object('id',s.id,'name',s.name,'inbound',i.tag,'removed',i.tag=ANY($2)) FROM squads s JOIN squad_inbounds si ON si.squad_id=s.id JOIN inbounds i ON i.id=si.inbound_id WHERE i.profile_id=$1").bind(id).bind(&removed).fetch_all(&st.pool).await?;
    let nodes = node_status(&st, id).await?;
    Ok(Json(
        json!({"version":old.get::<i32,_>("version"),"changed_paths":paths,"removed":removed,"hosts":hosts,"squads":squads,"nodes":nodes,"sensitive":paths.iter().any(|p|["port","privateKey","shortIds","serverNames","password","security","network"].iter().any(|k|p.contains(k)))}),
    ))
}
async fn node_status(st: &AppState, id: i64) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',n.id,'name',n.name,'engine_version',n.engine_version,'reported_version',n.reported_config_version,'applied',n.reported_config_version=p.version AND n.reported_users_version LIKE p.id::text||':%' AND n.engine_ok AND n.last_seen_at>now()-interval '90 seconds','target_version',p.version,'online',n.last_seen_at>now()-interval '90 seconds','engine_ok',n.engine_ok,'error',n.engine_error,'last_seen_at',n.last_seen_at) FROM nodes n JOIN config_profiles p ON p.id=n.profile_id WHERE n.profile_id=$1 AND n.deleted_at IS NULL ORDER BY n.name").bind(id).fetch_all(&st.pool).await?)
}
async fn status(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    Ok(Json(json!(node_status(&st, id).await?)))
}
async fn history(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('version',version,'config',config,'created_at',created_at) FROM profile_revisions WHERE profile_id=$1 ORDER BY version DESC LIMIT 30").bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!(rows)))
}

#[derive(Deserialize)]
struct Trial {
    node_id: i64,
    config: Value,
    expected_version: i32,
}
async fn trial_create(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<Trial>,
) -> Result<Json<Value>> {
    bounded(&b.config)?;
    // Only a free node is eligible. Rehearsals cannot change a production node.
    let mut tx = st.pool.begin().await?;
    let row = sqlx::query("SELECT name,version FROM config_profiles WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    if row.get::<i32, _>("version") != b.expected_version {
        return Err(Error::bad("Profile changed / Профиль изменён"));
    }
    let free:Option<i64>=sqlx::query_scalar("SELECT id FROM nodes WHERE id=$1 AND profile_id IS NULL AND deleted_at IS NULL AND last_seen_at>now()-interval '90 seconds' FOR UPDATE").bind(b.node_id).fetch_optional(&mut *tx).await?;
    if free.is_none() {
        return Err(Error::bad("Select an online node without a profile / Выберите ноду на связи без назначенного профиля"));
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM profile_trials WHERE profile_id=$1 AND state='testing')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if active {
        return Err(Error::bad(
            "Finish the active trial first / Сначала завершите текущую проверку",
        ));
    }
    let config = sn_core::service_parts::ensure_service_parts(&b.config).0;
    let check = sn_core::xray_check::check_structure(&config);
    if !check.valid {
        return Err(Error::bad(check.errors.join("; ")));
    }
    // Same create/validation/inbound synchronization as normal profiles; cleanup
    // on transaction failure prevents an abandoned candidate from accumulating.
    let Json(created) = crate::routes::profile_create(
        CurrentAdmin(a.clone()),
        State(st.clone()),
        Json(crate::routes::ProfileCreate {
            name: format!(
                "{} · test {}",
                row.get::<String, _>("name")
                    .chars()
                    .take(100)
                    .collect::<String>(),
                uuid::Uuid::new_v4()
            ),
            config: Some(config),
        }),
    )
    .await?;
    let candidate = created["id"]
        .as_i64()
        .ok_or_else(|| Error::bad("Candidate creation failed"))?;
    let result:Result<i64>=async {
        sqlx::query("UPDATE nodes SET profile_id=$2,reported_config_version=NULL WHERE id=$1").bind(b.node_id).bind(candidate).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM node_inbounds WHERE node_id=$1").bind(b.node_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO node_inbounds(node_id,inbound_id) SELECT $1,id FROM inbounds WHERE profile_id=$2").bind(b.node_id).bind(candidate).execute(&mut *tx).await?;
        let trial:i64=sqlx::query_scalar("INSERT INTO profile_trials(profile_id,candidate_id,node_id,base_version,candidate_version) VALUES($1,$2,$3,$4,1) RETURNING id").bind(id).bind(candidate).bind(b.node_id).bind(b.expected_version).fetch_one(&mut *tx).await?;tx.commit().await?;Ok(trial)
    }.await;
    match result {
        Ok(trial) => {
            audit(&st, a.id, "profile.trial", id).await?;
            Ok(Json(
                json!({"id":trial,"candidate_id":candidate,"node_id":b.node_id}),
            ))
        }
        Err(e) => {
            let _ = sqlx::query("DELETE FROM config_profiles WHERE id=$1")
                .bind(candidate)
                .execute(&st.pool)
                .await;
            Err(e)
        }
    }
}
async fn trial_get(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let trial:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(t)||jsonb_build_object('config',p.config,'current_version',p.version) FROM profile_trials t JOIN config_profiles p ON p.id=t.candidate_id WHERE t.profile_id=$1 AND t.state='testing'").bind(id).fetch_optional(&st.pool).await?;
    if let Some(mut t) = trial {
        let c = t["candidate_id"].as_i64().unwrap();
        t["nodes"] = json!(node_status(&st, c).await?);
        Ok(Json(t))
    } else {
        Ok(Json(Value::Null))
    }
}
async fn trial_finish(
    CurrentAdmin(a): CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    let pending = sqlx::query("SELECT id,node_id FROM profile_trials WHERE profile_id=$1 AND state='testing'")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    // Match node_update's lock order and recheck the same trial after waiting.
    sqlx::query("SELECT id FROM nodes WHERE id=$1 FOR UPDATE")
        .bind(pending.get::<i64, _>("node_id")).execute(&mut *tx).await?;
    let row=sqlx::query("SELECT id,candidate_id,node_id FROM profile_trials WHERE id=$1 AND state='testing' FOR UPDATE").bind(pending.get::<i64, _>("id")).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let node: i64 = row.get("node_id");
    let candidate: i64 = row.get("candidate_id");
    sqlx::query("DELETE FROM node_inbounds WHERE node_id=$1 AND EXISTS(SELECT 1 FROM nodes WHERE id=$1 AND profile_id=$2)").bind(node).bind(candidate).execute(&mut *tx).await?;
    sqlx::query("UPDATE nodes SET profile_id=NULL,reported_config_version=NULL,reported_users_version=NULL WHERE id=$1 AND profile_id=$2").bind(node).bind(candidate).execute(&mut *tx).await?;
    sqlx::query("UPDATE profile_trials SET state='finished' WHERE id=$1")
        .bind(row.get::<i64, _>("id"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(&st, a.id, "profile.trial.finish", id).await?;
    Ok(Json(json!({"ok":true,"candidate_id":candidate})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn github_origin_is_pinned() {
        for url in ["http://raw.githubusercontent.com/a/b/123/file","https://localhost/a/b","https://raw.githubusercontent.com.evil.com/a/b","https://raw.githubusercontent.com/a/b/main/x.json","https://user@raw.githubusercontent.com/a/b/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/x.json","https://raw.githubusercontent.com/a/b/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/x.json?x=1"]{assert!(github_url(url).is_err(),"{url}");}
        assert!(github_url("https://raw.githubusercontent.com/XTLS/Xray-examples/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/example/config.json").is_ok());
    }
}

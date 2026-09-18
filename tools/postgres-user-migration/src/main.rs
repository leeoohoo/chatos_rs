// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use argon2::password_hash::PasswordHash;
use chrono::{DateTime, Utc};
use clap::{ArgGroup, Parser};
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Json;

#[derive(Debug, Parser)]
#[command(
    about = "One-time, fail-closed User Service identity migration",
    group(ArgGroup::new("selection").required(true).args(["all_users", "username"])),
    group(ArgGroup::new("mode").args(["dry_run", "apply"]))
)]
struct Args {
    #[arg(long, env = "SOURCE_USER_MONGO_URL")]
    mongo_url: String,
    #[arg(long, default_value = "user_service")]
    mongo_database: String,
    #[arg(long, env = "TARGET_USER_POSTGRES_URL")]
    postgres_url: String,
    #[arg(long, conflicts_with = "username")]
    all_users: bool,
    #[arg(long, value_name = "NAME", action = clap::ArgAction::Append)]
    username: Vec<String>,
    #[arg(long)]
    include_wechat_identities: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    apply: bool,
    #[arg(long, requires = "apply")]
    allow_upsert: bool,
    /// Exact number of selected users expected from the source.
    #[arg(long)]
    expected_user_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserRecord {
    id: String,
    username: String,
    display_name: String,
    password_hash: String,
    role: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
    last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserExternalIdentityRecord {
    id: String,
    user_id: String,
    provider: String,
    app_id: String,
    open_id_hash: String,
    union_id_hash: Option<String>,
    #[serde(default)]
    companion_device_id: Option<String>,
    #[serde(default)]
    companion_device_public_key: Option<String>,
    created_at: String,
    updated_at: String,
    last_login_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Debug)]
struct ValidatedUser {
    record: UserRecord,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct ValidatedIdentity {
    record: UserExternalIdentityRecord,
    _created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    _last_login_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

#[tokio::main]
async fn main() {
    if let Err(message) = run(Args::parse()).await {
        eprintln!("migration failed: {message}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let dry_run = args.dry_run || !args.apply;
    if !is_mongo_url(&args.mongo_url) {
        return Err("--mongo-url must use mongodb:// or mongodb+srv://".to_string());
    }
    if !is_postgres_url(&args.postgres_url) {
        return Err("--postgres-url must use postgres:// or postgresql://".to_string());
    }

    let mongo = mongodb::Client::with_uri_str(&args.mongo_url)
        .await
        .map_err(redacted_error("connect to source database"))?;
    let source = mongo.database(&args.mongo_database);
    let filter = if args.all_users {
        doc! {}
    } else {
        doc! { "username": { "$in": &args.username } }
    };
    let mut cursor = source
        .collection::<UserRecord>("users")
        .find(filter, None)
        .await
        .map_err(redacted_error("read source users"))?;
    let mut users = Vec::new();
    while let Some(user) = cursor
        .try_next()
        .await
        .map_err(redacted_error("decode source users"))?
    {
        users.push(validate_user(user)?);
    }
    users.sort_by(|left, right| left.record.username.cmp(&right.record.username));
    validate_user_set(&users, &args)?;

    let selected_user_ids = users
        .iter()
        .map(|user| user.record.id.clone())
        .collect::<HashSet<_>>();
    let identities = if args.include_wechat_identities {
        load_identities(&source, &selected_user_ids).await?
    } else {
        Vec::new()
    };

    println!(
        "validated {} user(s) and {} active WeChat identity record(s)",
        users.len(),
        identities.len()
    );
    for user in &users {
        println!(
            "user id={} username={}",
            user.record.id, user.record.username
        );
    }
    if dry_run {
        println!("dry-run complete; PostgreSQL was not modified");
        return Ok(());
    }

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&args.postgres_url)
        .await
        .map_err(redacted_error("connect to target database"))?;
    let target_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .map_err(redacted_error("inspect target users"))?;
    if target_count != 0 && !args.allow_upsert {
        return Err(format!(
            "target users table contains {target_count} row(s); refusing without --allow-upsert"
        ));
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(redacted_error("begin target transaction"))?;
    for user in &users {
        let data = serde_json::to_value(&user.record)
            .map_err(|error| format!("serialize user {}: {error}", user.record.id))?;
        let statement = if args.allow_upsert {
            r#"INSERT INTO users
               (id, username, display_name, password_hash, role, enabled, created_at, updated_at, last_login_at, data)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (id) DO UPDATE SET
                 username = EXCLUDED.username,
                 display_name = EXCLUDED.display_name,
                 password_hash = EXCLUDED.password_hash,
                 role = EXCLUDED.role,
                 enabled = EXCLUDED.enabled,
                 created_at = EXCLUDED.created_at,
                 updated_at = EXCLUDED.updated_at,
                 last_login_at = EXCLUDED.last_login_at,
                 data = EXCLUDED.data"#
        } else {
            r#"INSERT INTO users
               (id, username, display_name, password_hash, role, enabled, created_at, updated_at, last_login_at, data)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#
        };
        sqlx::query(statement)
            .bind(&user.record.id)
            .bind(&user.record.username)
            .bind(&user.record.display_name)
            .bind(&user.record.password_hash)
            .bind(&user.record.role)
            .bind(user.record.enabled)
            .bind(user.created_at)
            .bind(user.updated_at)
            .bind(user.last_login_at)
            .bind(Json(data))
            .execute(&mut *transaction)
            .await
            .map_err(redacted_error("write target user"))?;
    }
    for identity in &identities {
        let data = serde_json::to_value(&identity.record).map_err(|error| {
            format!(
                "serialize external identity {}: {error}",
                identity.record.id
            )
        })?;
        let statement = if args.allow_upsert {
            r#"INSERT INTO user_external_identities
               (id, user_id, provider, app_id, open_id_hash, union_id_hash, revoked_at, updated_at, data)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (id) DO UPDATE SET
                 user_id = EXCLUDED.user_id,
                 provider = EXCLUDED.provider,
                 app_id = EXCLUDED.app_id,
                 open_id_hash = EXCLUDED.open_id_hash,
                 union_id_hash = EXCLUDED.union_id_hash,
                 revoked_at = EXCLUDED.revoked_at,
                 updated_at = EXCLUDED.updated_at,
                 data = EXCLUDED.data"#
        } else {
            r#"INSERT INTO user_external_identities
               (id, user_id, provider, app_id, open_id_hash, union_id_hash, revoked_at, updated_at, data)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#
        };
        sqlx::query(statement)
            .bind(&identity.record.id)
            .bind(&identity.record.user_id)
            .bind(&identity.record.provider)
            .bind(&identity.record.app_id)
            .bind(&identity.record.open_id_hash)
            .bind(&identity.record.union_id_hash)
            .bind(identity.revoked_at)
            .bind(identity.updated_at)
            .bind(Json(data))
            .execute(&mut *transaction)
            .await
            .map_err(redacted_error("write target external identity"))?;
    }
    verify_target(&mut transaction, &selected_user_ids, identities.len()).await?;
    transaction
        .commit()
        .await
        .map_err(redacted_error("commit target transaction"))?;
    println!(
        "migration committed and verified: {} user(s), {} active WeChat identity record(s)",
        users.len(),
        identities.len()
    );
    Ok(())
}

fn validate_user(record: UserRecord) -> Result<ValidatedUser, String> {
    require_nonempty("user id", &record.id)?;
    require_nonempty("username", &record.username)?;
    require_nonempty("display_name", &record.display_name)?;
    require_nonempty("role", &record.role)?;
    let password_hash = PasswordHash::new(&record.password_hash)
        .map_err(|_| format!("user {} has an invalid password hash", record.id))?;
    if !password_hash.algorithm.as_str().starts_with("argon2") {
        return Err(format!("user {} password hash is not Argon2", record.id));
    }
    Ok(ValidatedUser {
        created_at: parse_time("created_at", &record.id, &record.created_at)?,
        updated_at: parse_time("updated_at", &record.id, &record.updated_at)?,
        last_login_at: parse_optional_time("last_login_at", &record.id, &record.last_login_at)?,
        record,
    })
}

fn validate_user_set(users: &[ValidatedUser], args: &Args) -> Result<(), String> {
    if users.is_empty() {
        return Err("source selection returned zero users".to_string());
    }
    if let Some(expected) = args.expected_user_count {
        if users.len() != expected {
            return Err(format!(
                "source selection returned {} user(s), expected {expected}",
                users.len()
            ));
        }
    }
    if !args.all_users && users.len() != args.username.len() {
        return Err(format!(
            "requested {} username(s), but source returned {}; refusing partial selection",
            args.username.len(),
            users.len()
        ));
    }
    let mut ids = HashSet::new();
    let mut usernames = HashSet::new();
    for user in users {
        if !ids.insert(user.record.id.as_str()) {
            return Err(format!("duplicate user id {}", user.record.id));
        }
        if !usernames.insert(user.record.username.as_str()) {
            return Err(format!("duplicate username {}", user.record.username));
        }
    }
    Ok(())
}

async fn load_identities(
    source: &mongodb::Database,
    selected_user_ids: &HashSet<String>,
) -> Result<Vec<ValidatedIdentity>, String> {
    let ids = selected_user_ids.iter().cloned().collect::<Vec<_>>();
    let filter = doc! {
        "user_id": { "$in": ids },
        "provider": "wechat_mini_program",
        "revoked_at": null,
    };
    let mut cursor = source
        .collection::<UserExternalIdentityRecord>("user_external_identities")
        .find(filter, None)
        .await
        .map_err(redacted_error("read source external identities"))?;
    let mut identities = Vec::new();
    let mut identity_ids = HashSet::new();
    while let Some(record) = cursor
        .try_next()
        .await
        .map_err(redacted_error("decode source external identities"))?
    {
        require_nonempty("external identity id", &record.id)?;
        require_nonempty("external identity user_id", &record.user_id)?;
        require_nonempty("external identity provider", &record.provider)?;
        require_nonempty("external identity app_id", &record.app_id)?;
        require_nonempty("external identity open_id_hash", &record.open_id_hash)?;
        if !selected_user_ids.contains(&record.user_id) {
            return Err(format!(
                "external identity {} refers to an unselected user",
                record.id
            ));
        }
        if !identity_ids.insert(record.id.clone()) {
            return Err(format!("duplicate external identity id {}", record.id));
        }
        identities.push(ValidatedIdentity {
            _created_at: parse_time("created_at", &record.id, &record.created_at)?,
            updated_at: parse_time("updated_at", &record.id, &record.updated_at)?,
            _last_login_at: parse_optional_time(
                "last_login_at",
                &record.id,
                &record.last_login_at,
            )?,
            revoked_at: parse_optional_time("revoked_at", &record.id, &record.revoked_at)?,
            record,
        });
    }
    Ok(identities)
}

async fn verify_target(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    selected_user_ids: &HashSet<String>,
    expected_identity_count: usize,
) -> Result<(), String> {
    let ids = selected_user_ids.iter().cloned().collect::<Vec<_>>();
    let imported_users = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM users WHERE id = ANY($1) AND username <> '' AND password_hash <> ''",
    )
    .bind(&ids)
    .fetch_one(&mut **transaction)
    .await
    .map_err(redacted_error("verify target users"))?;
    if imported_users != ids.len() as i64 {
        return Err(format!(
            "target verification found {imported_users} valid selected user(s), expected {}",
            ids.len()
        ));
    }
    let imported_identities = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM user_external_identities WHERE user_id = ANY($1) AND revoked_at IS NULL",
    )
    .bind(&ids)
    .fetch_one(&mut **transaction)
    .await
    .map_err(redacted_error("verify target external identities"))?;
    if imported_identities != expected_identity_count as i64 {
        return Err(format!(
            "target verification found {imported_identities} active identity record(s), expected {expected_identity_count}"
        ));
    }
    Ok(())
}

fn parse_time(field: &str, id: &str, value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| format!("record {id} has invalid RFC3339 {field}"))
}

fn parse_optional_time(
    field: &str,
    id: &str,
    value: &Option<String>,
) -> Result<Option<DateTime<Utc>>, String> {
    value
        .as_deref()
        .map(|value| parse_time(field, id, value))
        .transpose()
}

fn require_nonempty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

fn is_mongo_url(value: &str) -> bool {
    value.starts_with("mongodb://") || value.starts_with("mongodb+srv://")
}

fn is_postgres_url(value: &str) -> bool {
    value.starts_with("postgres://") || value.starts_with("postgresql://")
}

fn redacted_error<E: std::fmt::Display>(context: &'static str) -> impl FnOnce(E) -> String {
    move |_| format!("{context} failed (details redacted to protect credentials and hashes)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Args {
        Args {
            mongo_url: "mongodb://source".to_string(),
            mongo_database: "user_service".to_string(),
            postgres_url: "postgresql://target".to_string(),
            all_users: true,
            username: Vec::new(),
            include_wechat_identities: false,
            dry_run: true,
            apply: false,
            allow_upsert: false,
            expected_user_count: None,
        }
    }

    fn valid_user(id: &str, username: &str) -> ValidatedUser {
        validate_user(UserRecord {
            id: id.to_string(),
            username: username.to_string(),
            display_name: username.to_string(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$wJsM0LvMbM2Xxg7gCtuN7V2N6hMzuXFhhfzuNqTMGvw".to_string(),
            role: "user".to_string(),
            enabled: true,
            created_at: "2026-09-17T00:00:00Z".to_string(),
            updated_at: "2026-09-17T00:00:00Z".to_string(),
            last_login_at: None,
        })
        .expect("valid user")
    }

    #[test]
    fn rejects_duplicate_source_identity() {
        let users = vec![valid_user("same", "first"), valid_user("same", "second")];
        assert!(validate_user_set(&users, &args()).is_err());
    }

    #[test]
    fn rejects_invalid_timestamp_without_fallback() {
        let mut user = valid_user("one", "first").record;
        user.created_at = "not-a-time".to_string();
        assert!(validate_user(user).is_err());
    }

    #[test]
    fn requires_exact_expected_count() {
        let mut arguments = args();
        arguments.expected_user_count = Some(2);
        assert!(validate_user_set(&[valid_user("one", "first")], &arguments).is_err());
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ensure_empty_database_bootstrap_allowed, AppStore, SuperAdminBootstrapConfig};
use crate::models::{UserRecord, USER_ROLE_SUPER_ADMIN, USER_ROLE_USER};

const BOOTSTRAP_USERNAME: &str = "bootstrap-admin";
const BOOTSTRAP_PASSWORD: &str = "local-bootstrap-password";
const BOOTSTRAP_DISPLAY_NAME: &str = "Bootstrap Admin";

fn bootstrap_config(
    allow_empty_database_admin_creation: bool,
) -> SuperAdminBootstrapConfig<'static> {
    SuperAdminBootstrapConfig {
        username: BOOTSTRAP_USERNAME,
        password: BOOTSTRAP_PASSWORD,
        display_name: BOOTSTRAP_DISPLAY_NAME,
        allow_empty_database_admin_creation,
    }
}

#[test]
fn empty_database_bootstrap_policy_requires_explicit_local_opt_in() {
    let production_disabled = ensure_empty_database_bootstrap_allowed(true, false)
        .expect_err("production empty database must be rejected");
    assert!(production_disabled.contains("empty in production"));
    assert!(production_disabled.contains("migrate users into PostgreSQL"));

    let production_enabled = ensure_empty_database_bootstrap_allowed(true, true)
        .expect_err("production override must not bypass the empty database gate");
    assert_eq!(production_enabled, production_disabled);

    let local_disabled = ensure_empty_database_bootstrap_allowed(false, false)
        .expect_err("local empty database requires an explicit opt-in");
    assert!(local_disabled.contains("automatic administrator creation is disabled"));
    assert!(local_disabled.contains("USER_SERVICE_ALLOW_EMPTY_DATABASE_ADMIN_CREATION"));

    ensure_empty_database_bootstrap_allowed(false, true)
        .expect("explicit non-production bootstrap should be allowed");
}

#[tokio::test]
#[ignore = "requires USER_SERVICE_TEST_DATABASE_URL with CREATE SCHEMA privilege"]
async fn postgres_super_admin_bootstrap_contract() {
    let database_url = std::env::var("USER_SERVICE_TEST_DATABASE_URL")
        .expect("USER_SERVICE_TEST_DATABASE_URL must be set");
    let root_pool = sqlx::PgPool::connect(database_url.as_str())
        .await
        .expect("connect contract test database");
    let schema = format!("user_admin_gate_{}", uuid::Uuid::new_v4().simple());
    let quoted_schema = format!("\"{schema}\"");
    sqlx::query(format!("CREATE SCHEMA {quoted_schema}").as_str())
        .execute(&root_pool)
        .await
        .expect("create isolated contract test schema");

    let search_path = format!("SET search_path TO {quoted_schema}");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _metadata| {
            let search_path = search_path.clone();
            Box::pin(async move {
                sqlx::query(search_path.as_str())
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url.as_str())
        .await
        .expect("connect isolated contract test pool");
    sqlx::query(
        r#"CREATE TABLE users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            enabled BOOLEAN NOT NULL,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,
            last_login_at TIMESTAMPTZ NULL,
            data JSONB NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .expect("create isolated users table");
    let store = AppStore::new(pool.clone());

    let local_error = store
        .ensure_default_super_admin_for_environment(bootstrap_config(false), false)
        .await
        .expect_err("local empty database without opt-in must fail");
    assert!(local_error.contains("automatic administrator creation is disabled"));
    assert_eq!(user_count(&pool).await, 0);

    let production_error = store
        .ensure_default_super_admin_for_environment(bootstrap_config(true), true)
        .await
        .expect_err("production empty database must fail even with opt-in");
    assert!(production_error.contains("empty in production"));
    assert_eq!(user_count(&pool).await, 0);

    store
        .ensure_default_super_admin_for_environment(bootstrap_config(true), false)
        .await
        .expect("explicit local bootstrap");
    let created = store
        .find_user_by_username(BOOTSTRAP_USERNAME)
        .await
        .expect("read created administrator")
        .expect("created administrator");
    assert_eq!(created.username, BOOTSTRAP_USERNAME);
    assert_eq!(created.display_name, BOOTSTRAP_DISPLAY_NAME);
    assert_eq!(created.role, USER_ROLE_SUPER_ADMIN);
    assert!(created.enabled);
    assert!(created.password_hash.starts_with("$argon2"));

    sqlx::query("DELETE FROM users")
        .execute(&pool)
        .await
        .expect("reset users before promotion contract");
    let existing = existing_user(BOOTSTRAP_USERNAME, "existing-password-hash");
    store
        .insert_user_record(&existing)
        .await
        .expect("insert same-name ordinary user");
    store
        .ensure_default_super_admin_for_environment(bootstrap_config(false), true)
        .await
        .expect("non-empty production database may preserve existing promotion behavior");
    let promoted = store
        .find_user_by_username(BOOTSTRAP_USERNAME)
        .await
        .expect("read promoted user")
        .expect("promoted user");
    assert_eq!(promoted.role, USER_ROLE_SUPER_ADMIN);
    assert_eq!(promoted.id, existing.id);
    assert_eq!(promoted.username, existing.username);
    assert_eq!(promoted.display_name, existing.display_name);
    assert_eq!(promoted.password_hash, existing.password_hash);
    assert_eq!(promoted.enabled, existing.enabled);
    assert_eq!(promoted.created_at, existing.created_at);
    assert_eq!(promoted.last_login_at, existing.last_login_at);

    sqlx::query("DELETE FROM users")
        .execute(&pool)
        .await
        .expect("reset users before non-matching contract");
    let unrelated = existing_user("unrelated-user", "unrelated-password-hash");
    store
        .insert_user_record(&unrelated)
        .await
        .expect("insert unrelated user");
    store
        .ensure_default_super_admin_for_environment(bootstrap_config(true), true)
        .await
        .expect("non-empty database without matching user remains unchanged");
    assert_eq!(user_count(&pool).await, 1);
    assert!(store
        .find_user_by_username(BOOTSTRAP_USERNAME)
        .await
        .expect("look up absent bootstrap user")
        .is_none());

    pool.close().await;
    sqlx::query(format!("DROP SCHEMA {quoted_schema} CASCADE").as_str())
        .execute(&root_pool)
        .await
        .expect("drop isolated contract test schema");
    root_pool.close().await;
}

fn existing_user(username: &str, password_hash: &str) -> UserRecord {
    UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.to_string(),
        display_name: "Existing User".to_string(),
        password_hash: password_hash.to_string(),
        role: USER_ROLE_USER.to_string(),
        enabled: false,
        created_at: "2026-01-01T00:00:00+00:00".to_string(),
        updated_at: "2026-01-01T00:00:00+00:00".to_string(),
        last_login_at: Some("2026-01-02T00:00:00+00:00".to_string()),
    }
}

async fn user_count(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(pool)
        .await
        .expect("count users")
}

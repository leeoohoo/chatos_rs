// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ensure_empty_database_bootstrap_allowed, now_rfc3339, AppStore,
    RegistrationEmailCodeReservationError, RegistrationTransactionError, SuperAdminBootstrapConfig,
};
use crate::models::{InviteCodeRecord, UserRecord, USER_ROLE_SUPER_ADMIN, USER_ROLE_USER};

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
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE SCHEMA {quoted_schema}"
    )))
    .execute(&root_pool)
    .await
    .expect("create isolated contract test schema");

    let search_path = format!("SET search_path TO {quoted_schema}");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _metadata| {
            let search_path = search_path.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(search_path))
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
            credential_version BIGINT NOT NULL DEFAULT 0,
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
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA {quoted_schema} CASCADE"
    )))
    .execute(&root_pool)
    .await
    .expect("drop isolated contract test schema");
    root_pool.close().await;
}

#[tokio::test]
#[ignore = "requires USER_SERVICE_TEST_DATABASE_URL with CREATE SCHEMA privilege"]
async fn postgres_registration_code_counters_are_atomic() {
    let database_url = std::env::var("USER_SERVICE_TEST_DATABASE_URL")
        .expect("USER_SERVICE_TEST_DATABASE_URL must be set");
    let root_pool = sqlx::PgPool::connect(database_url.as_str())
        .await
        .expect("connect contract test database");
    let schema = format!("registration_code_atomic_{}", uuid::Uuid::new_v4().simple());
    let quoted_schema = format!("\"{schema}\"");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE SCHEMA {quoted_schema}"
    )))
    .execute(&root_pool)
    .await
    .expect("create isolated contract test schema");
    let search_path = format!("SET search_path TO {quoted_schema}");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .after_connect(move |connection, _metadata| {
            let search_path = search_path.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(search_path))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url.as_str())
        .await
        .expect("connect isolated contract test pool");
    sqlx::query(
        r#"CREATE TABLE registration_email_codes (
            email TEXT PRIMARY KEY, expires_at BIGINT NOT NULL,
            consumed_at TIMESTAMPTZ NULL, updated_at TIMESTAMPTZ NOT NULL, data JSONB NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .expect("create registration code table");
    sqlx::query(
        r#"CREATE TABLE users (
            id TEXT PRIMARY KEY,username TEXT NOT NULL UNIQUE,display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL,credential_version BIGINT NOT NULL DEFAULT 0,
            role TEXT NOT NULL,enabled BOOLEAN NOT NULL,created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,last_login_at TIMESTAMPTZ NULL,data JSONB NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .expect("create users table");
    sqlx::query(
        r#"CREATE TABLE invite_codes (
            id TEXT PRIMARY KEY,code_hash TEXT NOT NULL UNIQUE,created_by_user_id TEXT NOT NULL,
            max_uses BIGINT NOT NULL,used_count BIGINT NOT NULL,expires_at BIGINT NULL,
            revoked_at TIMESTAMPTZ NULL,created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,data JSONB NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .expect("create invite table");
    let store = AppStore::new(pool.clone());
    let mut registered = existing_user("registered@example.com", "password-hash");
    registered.enabled = true;
    store
        .insert_user_record(&registered)
        .await
        .expect("insert existing registration email");
    let suppressed = store
        .reserve_registration_email_code_send(
            "registered@example.com",
            "unused-code".to_string(),
            "invite".to_string(),
            999,
            "2024-12-31T23:59:59Z".to_string(),
            600,
            60,
            5,
        )
        .await
        .expect("existing email response remains successful");
    assert!(suppressed.is_none());
    assert!(store
        .find_registration_email_code("registered@example.com")
        .await
        .expect("read suppressed reservation")
        .is_none());
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(12));
    let mut sends = Vec::new();
    for index in 0..12 {
        let store = store.clone();
        let barrier = barrier.clone();
        sends.push(tokio::spawn(async move {
            barrier.wait().await;
            store
                .reserve_registration_email_code_send(
                    "atomic@example.com",
                    format!("code-{index}"),
                    "invite".to_string(),
                    1_000,
                    "2025-01-01T00:00:00Z".to_string(),
                    600,
                    60,
                    5,
                )
                .await
        }));
    }
    let mut reserved = 0;
    let mut throttled = 0;
    for task in sends {
        match task.await.expect("join concurrent reservation") {
            Ok(_) => reserved += 1,
            Err(RegistrationEmailCodeReservationError::ResendTooSoon) => throttled += 1,
            Err(error) => panic!("unexpected reservation result: {error:?}"),
        }
    }
    assert_eq!(reserved, 1);
    assert_eq!(throttled, 11);

    let record = store
        .find_registration_email_code("atomic@example.com")
        .await
        .expect("read reservation")
        .expect("reserved record");
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(20));
    let mut guesses = Vec::new();
    for _ in 0..20 {
        let store = store.clone();
        let barrier = barrier.clone();
        guesses.push(tokio::spawn(async move {
            barrier.wait().await;
            store
                .verify_registration_email_code_attempt(
                    "atomic@example.com",
                    "wrong-code",
                    "invite",
                    1_001,
                    now_rfc3339().as_str(),
                    5,
                )
                .await
        }));
    }
    for task in guesses {
        assert!(!task
            .await
            .expect("join concurrent guess")
            .expect("record guess"));
    }
    let exhausted = store
        .find_registration_email_code("atomic@example.com")
        .await
        .expect("read exhausted record")
        .expect("exhausted record");
    assert_eq!(exhausted.attempts, 5);
    assert_ne!(record.code_hash, "wrong-code");

    let registration_time = "2026-10-06T00:00:00+00:00";
    store
        .reserve_registration_email_code_send(
            "register@example.com",
            "correct-code".to_string(),
            "invite-transaction".to_string(),
            2_000,
            registration_time.to_string(),
            600,
            60,
            5,
        )
        .await
        .expect("reserve transactional registration code")
        .expect("new transactional registration email");
    store
        .insert_invite_code(&InviteCodeRecord {
            id: "invite-transaction-id".to_string(),
            code_hash: "invite-transaction".to_string(),
            label: None,
            created_by_user_id: "bootstrap".to_string(),
            max_uses: 2,
            used_count: 0,
            expires_at_unix: None,
            revoked_at: None,
            last_used_at: None,
            created_at: registration_time.to_string(),
            updated_at: registration_time.to_string(),
        })
        .await
        .expect("insert transactional invite");
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let mut registrations = Vec::new();
    for suffix in ["first", "second"] {
        let store = store.clone();
        let barrier = barrier.clone();
        let mut user = existing_user("register@example.com", "password-hash");
        user.id = format!("user-{suffix}");
        user.enabled = true;
        registrations.push(tokio::spawn(async move {
            barrier.wait().await;
            store
                .register_user_with_invite_and_email_code(
                    &user,
                    "invite-transaction",
                    "correct-code",
                    2_001,
                    "2026-10-06T00:00:01+00:00",
                    5,
                )
                .await
        }));
    }
    let mut succeeded = 0;
    for registration in registrations {
        match registration.await.expect("join concurrent registration") {
            Ok(()) => succeeded += 1,
            Err(RegistrationTransactionError::InvalidVerificationCode) => {}
            Err(error) => panic!("unexpected transactional registration result: {error:?}"),
        }
    }
    assert_eq!(succeeded, 1);
    assert_eq!(user_count(&pool).await, 2);
    let invite = store
        .find_invite_code_by_id("invite-transaction-id")
        .await
        .expect("read transactional invite")
        .expect("transactional invite");
    assert_eq!(invite.used_count, 1);

    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA {quoted_schema} CASCADE"
    )))
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
        credential_version: 0,
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

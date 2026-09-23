// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn retention_configuration_rejects_zero_values() {
    let pool = sqlx::PgPool::connect_lazy("postgresql://unused:unused@localhost/unused")
        .expect("lazy pool");
    assert!(UserDataRetention::new(pool.clone(), Duration::ZERO, 1).is_err());
    assert!(UserDataRetention::new(pool, Duration::from_secs(1), 0).is_err());
}

#[tokio::test]
#[ignore = "requires USER_SERVICE_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn postgres_retention_prunes_expired_ephemeral_user_data() {
    let database_url = std::env::var("USER_SERVICE_TEST_DATABASE_URL")
        .expect("USER_SERVICE_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("retention-user-{suffix}");
    let expired = format!("retention-expired-{suffix}");
    let live = format!("retention-live-{suffix}");

    sqlx::query(
        "INSERT INTO users(id,username,display_name,password_hash,role,enabled,created_at,updated_at,data) \
         VALUES($1,$2,'Retention','unused','user',true,now(),now(),'{}'::jsonb)",
    )
    .bind(&user_id)
    .bind(format!("retention-{suffix}@test.invalid"))
    .execute(&pool)
    .await
    .expect("test user");

    insert_unix_expiry_records(&pool, &user_id, &expired, true).await;
    insert_unix_expiry_records(&pool, &user_id, &live, false).await;
    insert_timestamp_expiry_records(&pool, &expired, true).await;
    insert_timestamp_expiry_records(&pool, &live, false).await;

    assert_eq!(prune_expired_user_data(&pool, 10).await.unwrap(), 7);
    for (table, column, expired_id, live_id) in record_keys(&expired, &live) {
        let remaining: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT {column} FROM {table} WHERE {column}=ANY($1) ORDER BY {column}"
        ))
        .bind(vec![expired_id, live_id.clone()])
        .fetch_all(&pool)
        .await
        .expect("remaining retention records");
        assert_eq!(remaining, vec![live_id]);
    }

    for (table, column, expired_id, live_id) in record_keys(&expired, &live) {
        sqlx::query(&format!("DELETE FROM {table} WHERE {column}=ANY($1)"))
            .bind(vec![expired_id, live_id])
            .execute(&pool)
            .await
            .expect("cleanup retention records");
    }
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("cleanup user");
}

async fn insert_unix_expiry_records(pool: &sqlx::PgPool, user_id: &str, id: &str, expired: bool) {
    let expiry = if expired {
        "extract(epoch FROM now())::bigint-1"
    } else {
        "extract(epoch FROM now())::bigint+3600"
    };
    sqlx::query(&format!(
        "INSERT INTO revoked_tokens(jti,subject_id,revoked_at,expires_at) \
         VALUES($1,$2,now(),{expiry})"
    ))
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("revoked token");
    sqlx::query(&format!(
        "INSERT INTO registration_email_codes(email,expires_at,updated_at,data) \
         VALUES($1,{expiry},now(),'{{}}'::jsonb)"
    ))
    .bind(format!("{id}@test.invalid"))
    .execute(pool)
    .await
    .expect("registration code");
    sqlx::query(&format!(
        "INSERT INTO local_connector_auth_tickets(id,ticket_hash,user_id,expires_at,updated_at,data) \
         VALUES($1,$2,$3,{expiry},now(),'{{}}'::jsonb)"
    ))
    .bind(id)
    .bind(format!("ticket-hash-{id}"))
    .bind(user_id)
    .execute(pool)
    .await
    .expect("local connector ticket");
    sqlx::query(&format!(
        "INSERT INTO wechat_bind_tickets(id,ticket_hash,user_id,app_id,status,expires_at,updated_at,data) \
         VALUES($1,$2,$3,'contract','pending',{expiry},now(),'{{}}'::jsonb)"
    ))
    .bind(format!("wechat-{id}"))
    .bind(format!("wechat-hash-{id}"))
    .bind(user_id)
    .execute(pool)
    .await
    .expect("wechat ticket");
    sqlx::query(&format!(
        "INSERT INTO client_sessions(id,user_id,client_type,token_jti,expires_at,updated_at,data) \
         VALUES($1,$2,'contract',$3,{expiry},now(),'{{}}'::jsonb)"
    ))
    .bind(format!("session-{id}"))
    .bind(user_id)
    .bind(format!("session-jti-{id}"))
    .execute(pool)
    .await
    .expect("client session");
}

async fn insert_timestamp_expiry_records(pool: &sqlx::PgPool, id: &str, expired: bool) {
    let expiry = if expired {
        "now()-interval '1 second'"
    } else {
        "now()+interval '1 hour'"
    };
    sqlx::query(&format!(
        "INSERT INTO device_proof_nonces(id,expires_at) VALUES($1,{expiry})"
    ))
    .bind(id)
    .execute(pool)
    .await
    .expect("device proof nonce");
    sqlx::query(&format!(
        "INSERT INTO login_throttle(key,attempts,window_start_unix,expires_at) \
         VALUES($1,1,1,{expiry})"
    ))
    .bind(id)
    .execute(pool)
    .await
    .expect("login throttle");
}

fn record_keys(expired: &str, live: &str) -> Vec<(&'static str, &'static str, String, String)> {
    vec![
        (
            "revoked_tokens",
            "jti",
            expired.to_string(),
            live.to_string(),
        ),
        (
            "registration_email_codes",
            "email",
            format!("{expired}@test.invalid"),
            format!("{live}@test.invalid"),
        ),
        (
            "local_connector_auth_tickets",
            "id",
            expired.to_string(),
            live.to_string(),
        ),
        (
            "wechat_bind_tickets",
            "id",
            format!("wechat-{expired}"),
            format!("wechat-{live}"),
        ),
        (
            "client_sessions",
            "id",
            format!("session-{expired}"),
            format!("session-{live}"),
        ),
        (
            "device_proof_nonces",
            "id",
            expired.to_string(),
            live.to_string(),
        ),
        (
            "login_throttle",
            "key",
            expired.to_string(),
            live.to_string(),
        ),
    ]
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::test_state;
use axum::http::StatusCode;

#[tokio::test]
#[ignore = "requires USER_SERVICE_TEST_DATABASE_URL with CREATE SCHEMA privilege"]
async fn administrator_session_round_trip_and_revocation() {
    use super::super::admin_session;
    use crate::models::{LoginRequest, UserRecord};
    use crate::store::AppStore;
    use axum::extract::{ConnectInfo, State};
    use axum::http::{header, HeaderMap, HeaderValue};
    use axum::Json;

    let database_url = std::env::var("USER_SERVICE_TEST_DATABASE_URL").unwrap();
    let root_pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let schema = format!("admin_session_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&root_pool)
        .await
        .unwrap();
    let search_path = format!("SET search_path TO {schema}");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let search_path = search_path.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(search_path))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .unwrap();
    crate::store::MIGRATOR.run(&pool).await.unwrap();
    let mut state = test_state().await;
    state.store = AppStore::new(pool.clone());
    state.login_throttle = crate::login_throttle::LoginThrottle::new(pool.clone());
    let password = "a unique administrator test password";
    let mut user = UserRecord {
        id: "test-admin".to_string(),
        username: "test-admin".to_string(),
        display_name: "Test Admin".to_string(),
        password_hash: crate::auth::hash_password(password).unwrap(),
        credential_version: 0,
        role: "super_admin".to_string(),
        enabled: true,
        created_at: crate::store::now_rfc3339(),
        updated_at: crate::store::now_rfc3339(),
        last_login_at: None,
    };
    state.store.insert_user_record(&user).await.unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://admin.jgoool.com"),
    );
    headers.insert("x-chatos-admin-session", HeaderValue::from_static("1"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    let response = admin_session::login(
        State(state.clone()),
        ConnectInfo("127.0.0.1:12345".parse().unwrap()),
        headers.clone(),
        Json(LoginRequest {
            username: user.username.clone(),
            password: password.to_string(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    let bytes = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    let login: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = login["token"].as_str().unwrap();
    // A new page can restore using only the cookie, repeatedly and without rotation races.
    for _ in 0..2 {
        let restored = admin_session::restore(State(state.clone()), headers.clone())
            .await
            .unwrap();
        assert_eq!(restored.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(restored.into_body(), 8192)
            .await
            .unwrap();
        let restored: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored["token"], token);
    }
    let ordinary_token = crate::auth::encode_user_token(&state.config, &user).unwrap();
    let mut invalid_headers = headers.clone();
    invalid_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("__Secure-chatos_admin_session={ordinary_token}")).unwrap(),
    );
    assert_eq!(
        admin_session::restore(State(state.clone()), invalid_headers.clone())
            .await
            .unwrap_err()
            .0,
        StatusCode::UNAUTHORIZED
    );
    let mut expired_claims = crate::auth::decode_user_service_token(token, &state.config).unwrap();
    expired_claims.exp = 0;
    let expired_token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &expired_claims,
        &jsonwebtoken::EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .unwrap();
    invalid_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("__Secure-chatos_admin_session={expired_token}")).unwrap(),
    );
    assert_eq!(
        admin_session::restore(State(state.clone()), invalid_headers)
            .await
            .unwrap_err()
            .0,
        StatusCode::UNAUTHORIZED
    );
    user.enabled = false;
    state.store.update_user_record(&user).await.unwrap();
    assert_eq!(
        admin_session::restore(State(state.clone()), headers.clone())
            .await
            .unwrap_err()
            .0,
        StatusCode::UNAUTHORIZED
    );
    user.enabled = true;
    user.credential_version = 1;
    state.store.update_user_record(&user).await.unwrap();
    assert_eq!(
        admin_session::restore(State(state.clone()), headers.clone())
            .await
            .unwrap_err()
            .0,
        StatusCode::UNAUTHORIZED
    );
    user.credential_version = 0;
    user.role = "user".to_string();
    state.store.update_user_record(&user).await.unwrap();
    assert_eq!(
        admin_session::login(
            State(state.clone()),
            ConnectInfo("127.0.0.1:12345".parse().unwrap()),
            headers.clone(),
            Json(LoginRequest {
                username: user.username.clone(),
                password: password.to_string(),
            })
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        admin_session::restore(State(state.clone()), headers.clone())
            .await
            .unwrap_err()
            .0,
        StatusCode::FORBIDDEN
    );
    user.role = "super_admin".to_string();
    state.store.update_user_record(&user).await.unwrap();
    let cleared = admin_session::logout(State(state.clone()), headers.clone())
        .await
        .unwrap();
    assert_eq!(cleared.status(), StatusCode::NO_CONTENT);
    assert!(cleared.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));
    assert_eq!(
        admin_session::restore(State(state.clone()), headers)
            .await
            .unwrap_err()
            .0,
        StatusCode::UNAUTHORIZED
    );
    let claims = crate::auth::decode_user_service_token(token, &state.config).unwrap();
    assert!(state.store.is_token_revoked(&claims.jti).await.unwrap());
    drop(state);
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&root_pool)
        .await
        .unwrap();
    root_pool.close().await;
}

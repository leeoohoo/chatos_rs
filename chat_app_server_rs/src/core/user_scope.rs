use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;

fn user_scope_forbidden_response() -> (StatusCode, Json<Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "user_id 与登录用户不一致",
            "code": "user_scope_forbidden"
        })),
    )
}

pub fn ensure_user_id_matches(
    user_id: Option<&str>,
    auth: &AuthUser,
) -> Result<(), (StatusCode, Json<Value>)> {
    if user_id.is_some_and(|uid| uid != auth.user_id.as_str()) {
        return Err(user_scope_forbidden_response());
    }
    Ok(())
}

pub fn ensure_and_set_user_id(
    user_id: &mut Option<String>,
    auth: &AuthUser,
) -> Result<(), (StatusCode, Json<Value>)> {
    ensure_user_id_matches(user_id.as_deref(), auth)?;
    *user_id = Some(auth.user_id.clone());
    Ok(())
}

pub fn resolve_user_id(
    user_id: Option<String>,
    auth: &AuthUser,
) -> Result<String, (StatusCode, Json<Value>)> {
    ensure_user_id_matches(user_id.as_deref(), auth)?;
    Ok(user_id.unwrap_or_else(|| auth.user_id.clone()))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::{ensure_user_id_matches, resolve_user_id};
    use crate::core::auth::AuthUser;

    fn mock_auth_user() -> AuthUser {
        AuthUser {
            user_id: "user-1".to_string(),
            email: "user-1@example.com".to_string(),
        }
    }

    #[test]
    fn rejects_mismatched_user_id_with_structured_code() {
        let auth = mock_auth_user();
        let err =
            ensure_user_id_matches(Some("user-2"), &auth).expect_err("should reject mismatch");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert_eq!(
            err.1 .0,
            json!({
                "error": "user_id 与登录用户不一致",
                "code": "user_scope_forbidden"
            })
        );
    }

    #[test]
    fn resolves_user_id_from_auth_when_missing() {
        let auth = mock_auth_user();
        let resolved = resolve_user_id(None, &auth).expect("should resolve auth user id");
        assert_eq!(resolved, "user-1");
    }
}

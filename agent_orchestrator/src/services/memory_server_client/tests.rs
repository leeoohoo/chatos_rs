use super::{MemoryAuthLoginResponse, MemoryAuthMeResponse};

#[test]
fn auth_login_response_supports_user_id_field() {
    let value = serde_json::json!({
        "token": "t1",
        "user_id": "alice",
        "role": "user"
    });
    let parsed: MemoryAuthLoginResponse =
        serde_json::from_value(value).expect("login response with user_id should parse");
    assert_eq!(parsed.user_id, "alice");
}

#[test]
fn auth_login_response_supports_username_alias() {
    let value = serde_json::json!({
        "token": "t1",
        "username": "alice",
        "role": "user"
    });
    let parsed: MemoryAuthLoginResponse =
        serde_json::from_value(value).expect("login response with username should parse");
    assert_eq!(parsed.user_id, "alice");
}

#[test]
fn auth_me_response_supports_user_id_field() {
    let value = serde_json::json!({
        "user_id": "alice",
        "role": "user"
    });
    let parsed: MemoryAuthMeResponse =
        serde_json::from_value(value).expect("me response with user_id should parse");
    assert_eq!(parsed.user_id, "alice");
}

#[test]
fn auth_me_response_supports_username_alias() {
    let value = serde_json::json!({
        "username": "alice",
        "role": "user"
    });
    let parsed: MemoryAuthMeResponse =
        serde_json::from_value(value).expect("me response with username should parse");
    assert_eq!(parsed.user_id, "alice");
}

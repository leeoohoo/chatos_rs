use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::models::{CreateRemoteServerRequest, RemoteServerRecord};

use super::{normalized_optional, validate_required};

pub(super) fn build_remote_server_record(
    input: CreateRemoteServerRequest,
    creator: Option<&CurrentUser>,
    task_id: Option<String>,
    now: String,
) -> Result<RemoteServerRecord, String> {
    validate_required("name", &input.name)?;
    validate_required("host", &input.host)?;
    validate_required("username", &input.username)?;
    validate_required("auth_type", &input.auth_type)?;

    let record = RemoteServerRecord {
        id: Uuid::new_v4().to_string(),
        name: input.name.trim().to_string(),
        host: input.host.trim().to_string(),
        port: normalize_remote_server_port(input.port)?,
        username: input.username.trim().to_string(),
        auth_type: normalize_remote_server_auth_type(&input.auth_type)?,
        password: normalized_optional(input.password),
        private_key_path: normalized_optional(input.private_key_path),
        certificate_path: normalized_optional(input.certificate_path),
        default_remote_path: normalized_optional(input.default_remote_path),
        host_key_policy: normalize_remote_server_host_key_policy(input.host_key_policy.as_deref())?,
        enabled: input.enabled.unwrap_or(true),
        last_tested_at: None,
        last_test_status: None,
        last_test_message: None,
        last_active_at: None,
        creator_user_id: creator.map(|user| user.id.clone()),
        creator_username: creator.map(|user| user.username.clone()),
        creator_display_name: creator.map(|user| user.display_name.clone()),
        owner_user_id: creator
            .and_then(|user| user.effective_owner_user_id().map(ToOwned::to_owned)),
        owner_username: creator
            .and_then(|user| user.effective_owner_username().map(ToOwned::to_owned)),
        owner_display_name: creator.and_then(|user| {
            user.effective_owner_display_name()
                .map(ToOwned::to_owned)
                .or_else(|| user.effective_owner_username().map(ToOwned::to_owned))
        }),
        task_id,
        created_at: now.clone(),
        updated_at: now,
    };
    validate_remote_server_auth_fields(&record)?;
    Ok(record)
}

pub(super) fn normalize_remote_server_port(value: Option<i64>) -> Result<i64, String> {
    let port = value.unwrap_or(22);
    if port <= 0 || port > u16::MAX as i64 {
        Err("port 必须在 1-65535 之间".to_string())
    } else {
        Ok(port)
    }
}

pub(super) fn normalize_remote_server_auth_type(value: &str) -> Result<String, String> {
    let normalized = value.trim();
    match normalized {
        "password" | "private_key" | "private_key_cert" => Ok(normalized.to_string()),
        _ => Err("auth_type 仅支持 password / private_key / private_key_cert".to_string()),
    }
}

pub(super) fn normalize_remote_server_host_key_policy(
    value: Option<&str>,
) -> Result<String, String> {
    let normalized = value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or("accept_new");
    match normalized {
        "accept_new" | "strict" => Ok(normalized.to_string()),
        _ => Err("host_key_policy 仅支持 accept_new / strict".to_string()),
    }
}

pub(super) fn validate_remote_server_auth_fields(
    record: &RemoteServerRecord,
) -> Result<(), String> {
    match record.auth_type.as_str() {
        "password" => {
            if record
                .password
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err("password 模式需要提供 password".to_string());
            }
        }
        "private_key" | "private_key_cert" => {
            if record
                .private_key_path
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err("private_key 模式需要提供 private_key_path".to_string());
            }
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}

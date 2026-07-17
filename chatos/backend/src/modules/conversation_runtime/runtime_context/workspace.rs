// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::warn;

use super::support::normalize_optional_text;
use crate::api::fs::policy::FsPathPolicy;
use crate::core::auth::AuthUser;

pub(super) async fn authorize_runtime_workspace_dir(
    user_id: Option<&str>,
    raw: Option<String>,
) -> Option<String> {
    let raw = normalize_optional_text(raw.as_deref())?;
    let Some(user_id) = user_id.map(str::trim).filter(|value| !value.is_empty()) else {
        warn!("runtime workspace path dropped: missing effective user id");
        return None;
    };
    let auth = AuthUser {
        user_id: user_id.to_string(),
        role: "user".to_string(),
    };
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(policy) => policy,
        Err(err) => {
            warn!(
                user_id,
                error = err.message(),
                "runtime workspace path dropped: policy unavailable"
            );
            return None;
        }
    };
    let authorized = match policy.authorize_existing_dir(
        raw.as_str(),
        "杩愯宸ヤ綔鐩綍涓嶅瓨鍦ㄦ垨涓嶆槸鐩綍",
        "杩愯宸ヤ綔鐩綍涓嶅瓨鍦ㄦ垨涓嶆槸鐩綍",
    ) {
        Ok(path) => path,
        Err(err) => {
            warn!(
                user_id,
                workspace_dir = raw.as_str(),
                error = err.message(),
                "runtime workspace path dropped: unauthorized"
            );
            return None;
        }
    };
    if let Err(err) = policy.require_write(&authorized) {
        warn!(
            user_id,
            workspace_dir = raw.as_str(),
            error = err.message(),
            "runtime workspace path dropped: not writable"
        );
        return None;
    }
    Some(authorized.path.to_string_lossy().to_string())
}

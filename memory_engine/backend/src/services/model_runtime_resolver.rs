// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::models::EngineModelProfile;

pub(super) async fn resolve_model_runtime_for_profile(
    _config: &AppConfig,
    profile: &EngineModelProfile,
    _owner_user_id: Option<&str>,
) -> Result<EngineModelProfile, String> {
    let has_embedded_runtime = profile
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        && profile
            .base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
    if has_embedded_runtime {
        return Ok(profile.clone());
    }
    Err(format!(
        "cloud_model_credentials_required: Memory Engine profile {} must contain cloud-resident api_key and base_url; Local Connector credential lookup is disabled",
        profile.id
    ))
}

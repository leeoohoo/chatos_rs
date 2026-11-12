// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::SummaryJobSettings;
use crate::config::AppConfig;

pub(crate) async fn load_summary_job_settings(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<SummaryJobSettings, String> {
    let policy =
        crate::services::control_plane::get_effective_job_policy(config, owner_user_id, "summary")
            .await?;
    Ok(SummaryJobSettings {
        token_limit: policy.max_model_input_tokens,
        source_batch_token_limit: policy.max_model_input_tokens,
        cloud_owner_entity_id: None,
        cloud_resume_kind: None,
    })
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskRecord};

pub(super) async fn resolve_model_runtime_for_task(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<ModelConfigRecord, String> {
    let _ = (config, task);
    let has_embedded_runtime =
        !model_config.api_key.trim().is_empty() && !model_config.base_url.trim().is_empty();
    if has_embedded_runtime {
        return Ok(model_config.clone());
    }
    Err(format!(
        "cloud_model_credentials_required: task runner model config {} must contain cloud-resident api_key and base_url; Local Connector credential lookup is disabled",
        model_config.id
    ))
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::TestModelConfigRequest;

use super::support::{
    model_config_for_user, model_configs_for_user, model_visible_to_user, require_admin_tool,
};
use super::{
    decode_args, text_result, ModelConfigIdArgs, TaskRunnerMcpService, TestModelConfigArgs,
};

impl TaskRunnerMcpService {
    pub(super) async fn call_model_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
    ) -> Result<Value, String> {
        match name {
            "list_model_configs" => {
                let _ = decode_args::<Value>(args).ok();
                let models = self.model_config_service.list_model_configs().await?;
                Ok(text_result(json!(model_configs_for_user(
                    models,
                    current_user
                ))))
            }
            "get_model_config" => {
                let args: ModelConfigIdArgs = decode_args(args)?;
                let model = self
                    .model_config_service
                    .get_model_config(args.model_config_id.as_str())
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                if !model.enabled || !model_visible_to_user(&model, current_user) {
                    return Err(format!("模型配置不存在: {}", args.model_config_id));
                }
                Ok(text_result(model_config_for_user(model, current_user)))
            }
            "test_model_config" => {
                require_admin_tool(current_user)?;
                let args: TestModelConfigArgs = decode_args(args)?;
                let result = self
                    .model_config_service
                    .test_model_config(
                        args.model_config_id.as_str(),
                        TestModelConfigRequest {
                            prompt: args.prompt,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(json!(result)))
            }
            other => Err(format!("unsupported model tool: {other}")),
        }
    }
}

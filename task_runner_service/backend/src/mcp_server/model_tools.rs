use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{CreateModelConfigRequest, TestModelConfigRequest};

use super::support::{model_config_for_user, model_configs_for_user, require_admin_tool};
use super::{
    decode_args, text_result, ModelConfigIdArgs, TaskRunnerMcpService, TestModelConfigArgs,
    UpdateModelConfigArgs,
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
                Ok(text_result(model_config_for_user(model, current_user)))
            }
            "create_model_config" => {
                require_admin_tool(current_user)?;
                let input: CreateModelConfigRequest = decode_args(args)?;
                let model = self.model_config_service.create_model_config(input).await?;
                Ok(text_result(json!(model)))
            }
            "update_model_config" => {
                require_admin_tool(current_user)?;
                let args: UpdateModelConfigArgs = decode_args(args)?;
                let model = self
                    .model_config_service
                    .update_model_config(args.model_config_id.as_str(), args.patch)
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(json!(model)))
            }
            "delete_model_config" => {
                require_admin_tool(current_user)?;
                let args: ModelConfigIdArgs = decode_args(args)?;
                let deleted = self
                    .model_config_service
                    .delete_model_config(args.model_config_id.as_str())
                    .await?;
                if !deleted {
                    return Err(format!("模型配置不存在: {}", args.model_config_id));
                }
                Ok(text_result(json!({
                    "deleted": true,
                    "model_config_id": args.model_config_id,
                })))
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

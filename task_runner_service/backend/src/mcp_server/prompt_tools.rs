use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{CancelUiPromptRequest, SubmitUiPromptRequest};

use super::{
    decode_args, text_result, CancelPromptArgs, ListPromptsArgs, PromptIdArgs, SubmitPromptArgs,
    TaskRunnerMcpService,
};

impl TaskRunnerMcpService {
    pub(super) async fn call_prompt_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
    ) -> Result<Value, String> {
        match name {
            "list_prompts" => {
                let args: ListPromptsArgs = decode_args(args)?;
                if let Some(task_id) = args.task_id.as_deref() {
                    self.require_task_for_user(task_id, current_user).await?;
                }
                if let Some(run_id) = args.run_id.as_deref() {
                    self.require_run_for_user(run_id, current_user).await?;
                }
                let prompts = self
                    .ui_prompt_service
                    .list_prompts(args.task_id.as_deref(), args.run_id.as_deref(), args.status)
                    .await?;
                let prompts = self.filter_prompts_for_user(prompts, current_user).await?;
                Ok(text_result(json!(prompts)))
            }
            "get_prompt" => {
                let args: PromptIdArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                self.require_prompt_for_user(&prompt, current_user).await?;
                Ok(text_result(json!(prompt)))
            }
            "submit_prompt" => {
                let args: SubmitPromptArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                self.require_prompt_for_user(&prompt, current_user).await?;
                let prompt = self
                    .ui_prompt_service
                    .submit_prompt(
                        args.prompt_id.as_str(),
                        SubmitUiPromptRequest {
                            values: args.values,
                            selection: args.selection,
                            reason: args.reason,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                Ok(text_result(json!(prompt)))
            }
            "cancel_prompt" => {
                let args: CancelPromptArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                self.require_prompt_for_user(&prompt, current_user).await?;
                let prompt = self
                    .ui_prompt_service
                    .cancel_prompt(
                        args.prompt_id.as_str(),
                        CancelUiPromptRequest {
                            reason: args.reason,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                Ok(text_result(json!(prompt)))
            }
            other => Err(format!("unsupported prompt tool: {other}")),
        }
    }
}

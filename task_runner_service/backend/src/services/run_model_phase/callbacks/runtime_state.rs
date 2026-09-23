// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::run_model_phase::supply_chain::SupplyChainEvidenceState;
use async_trait::async_trait;
use chatos_ai_runtime::{
    AiResponse, RuntimeBeforeModelRequest, RuntimeFinalResponseAction, RuntimeFinalResponseContext,
    RuntimeIterationContext, RuntimeLifecycleHook, TaskAcceptanceEvidence, TaskExecutionOutcome,
    TaskExecutionOutcomeStatus, TaskExecutionProgressState, TaskExecutionReviewCheckpoint,
    TaskExecutionReviewPolicy, TaskExecutionReviewTrigger,
};
#[cfg(test)]
#[path = "runtime_state/tests.rs"]
mod tests;

const TASK_EXECUTION_OUTCOME_METADATA_KEY: &str = "task_execution_outcome";
const TASK_OUTCOME_REPORT_REQUIRED_REASON: &str = "task_outcome_report_required";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(in crate::services) struct TaskRunnerLifecycleState {
    pub(in crate::services) visible_response: Option<AiResponse>,
    pub(in crate::services) execution_outcome: Option<TaskExecutionOutcome>,
}

struct TaskRunnerLifecycleHook {
    finalization: TaskFinalizationLifecycleHook,
    progress: Arc<TaskExecutionProgressState>,
    active_review: parking_lot::Mutex<Option<TaskExecutionReviewCheckpoint>>,
    state: Arc<parking_lot::Mutex<TaskRunnerLifecycleState>>,
    store: crate::store::AppStore,
    run_id: String,
    expected_acceptance_criteria: Vec<String>,
    mcp_runtime_session: Option<chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle>,
}

impl TaskRunnerLifecycleHook {
    fn new(
        max_iterations: usize,
        progress: Arc<TaskExecutionProgressState>,
        state: Arc<parking_lot::Mutex<TaskRunnerLifecycleState>>,
        store: crate::store::AppStore,
        run_id: String,
        expected_acceptance_criteria: Vec<String>,
        mcp_runtime_session: Option<chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle>,
    ) -> Self {
        Self {
            finalization: TaskFinalizationLifecycleHook::new(max_iterations),
            progress,
            active_review: parking_lot::Mutex::new(None),
            state,
            store,
            run_id,
            expected_acceptance_criteria,
            mcp_runtime_session,
        }
    }

    fn record_review_checkpoint(
        &self,
        checkpoint: TaskExecutionReviewCheckpoint,
        confirmed_project_paths: &[String],
    ) {
        let payload = json!({
            "iteration": checkpoint.iteration,
            "trigger": checkpoint.trigger.as_str(),
            "read_only_iterations": checkpoint.read_only_iterations,
            "missing_read_failures": checkpoint.missing_read_failures,
            "checkpoints_since_action": checkpoint.checkpoints_since_action,
            "policy": {
                "read_only_iterations": checkpoint.policy.read_only_iterations,
                "missing_read_failures": checkpoint.policy.missing_read_failures,
                "repeat_interval_iterations": checkpoint.policy.repeat_interval_iterations,
            },
            "context_action": "persistent_guidance",
            "disabled_tool_names": [],
            "review_contract": "evidence_driven_next_action",
            "confirmed_project_paths": confirmed_project_paths,
        });
        self.store.append_run_event_sync(TaskRunEventRecord::new(
            self.run_id.clone(),
            "execution_review_checkpoint",
            Some("已进入证据驱动的工程决策复盘".to_string()),
            Some(payload),
        ));
    }
}

#[async_trait]
impl RuntimeLifecycleHook for TaskRunnerLifecycleHook {
    async fn before_model_request(
        &self,
        context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        self.progress.begin_iteration(context.iteration);
        if self.reported_task_outcome().await?.is_some() {
            return Ok(RuntimeBeforeModelRequest::unchanged()
                .with_tools_enabled(false)
                .with_input_items(vec![task_outcome_final_response_message()]));
        }
        if context.reason == TASK_OUTCOME_REPORT_REQUIRED_REASON {
            return Ok(RuntimeBeforeModelRequest::unchanged());
        }
        let protected_skill_items = match self.mcp_runtime_session.as_ref() {
            Some(session) => {
                session
                    .routes()
                    .await
                    .map_err(|error| {
                        format!("load protected Plugin Skill context failed: {error}")
                    })?
                    .protected_skill_instruction_items
            }
            None => Vec::new(),
        };
        let iteration_input = context.input.to_string();
        let iteration = context.iteration;
        let mut before = self.finalization.before_model_request(context).await?;
        if !before.tools_enabled {
            return Ok(RuntimeBeforeModelRequest::unchanged()
                .with_input_items(vec![task_outcome_budget_exhausted_message()]));
        }

        let detected_checkpoint = self.progress.should_trigger_review(iteration);
        let confirmed_project_paths = self.progress.confirmed_project_paths();
        if let Some(checkpoint) = detected_checkpoint {
            self.record_review_checkpoint(checkpoint, &confirmed_project_paths);
        }
        if let Some(checkpoint) =
            persistent_review_checkpoint(&self.active_review, detected_checkpoint)
        {
            // Keep the decision contract present after each tool result. Otherwise a bounded
            // locate/edit action can return to an unconstrained exploration loop on the next turn.
            before.input_items.push(checkpoint_guidance_message(
                checkpoint,
                &confirmed_project_paths,
            ));
        }
        before.input_items.extend(
            protected_skill_items
                .into_iter()
                .filter(|item| !protected_skill_item_is_already_present(item, &iteration_input)),
        );
        Ok(before)
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        let Some(reported_outcome) = self.reported_task_outcome().await? else {
            return Ok(RuntimeFinalResponseAction::Continue {
                input_items: vec![
                    json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": context.response.content
                        }]
                    }),
                    task_outcome_report_required_message(),
                ],
                reason: TASK_OUTCOME_REPORT_REQUIRED_REASON.to_string(),
            });
        };
        let mut response = context.response;
        response.content = normalized_task_final_response_content(response.content.as_str())
            .unwrap_or_else(|| reported_outcome.reason.clone());
        let outcome = task_execution_outcome_from_ai_report(
            response.content.as_str(),
            self.expected_acceptance_criteria.as_slice(),
            self.progress.confirmed_project_paths(),
            self.progress.confirmed_validation_commands(),
            self.progress.confirmed_acceptance_tools(),
            self.progress.pending_completion_requirements(),
            reported_outcome,
        );
        let mut state = self.state.lock();
        state.visible_response = Some(response);
        state.execution_outcome = Some(outcome);
        Ok(RuntimeFinalResponseAction::Accept)
    }

    async fn final_response_metadata(
        &self,
        _context: RuntimeFinalResponseContext,
    ) -> Result<Option<Value>, String> {
        self.state
            .lock()
            .execution_outcome
            .clone()
            .map(|outcome| {
                serde_json::to_value(outcome)
                    .map(|outcome| json!({(TASK_EXECUTION_OUTCOME_METADATA_KEY): outcome}))
                    .map_err(|err| format!("failed to serialize task execution outcome: {err}"))
            })
            .transpose()
    }
}

fn normalized_task_final_response_content(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return Some(content.to_string());
    };
    if value.get("type").and_then(Value::as_str) != Some("output_text") {
        return Some(content.to_string());
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

impl TaskRunnerLifecycleHook {
    async fn reported_task_outcome(&self) -> Result<Option<AiReportedTaskOutcome>, String> {
        let event = self
            .store
            .get_run_event_by_type(self.run_id.as_str(), "task_outcome_reported")
            .await?;
        event.map(ai_reported_task_outcome_from_event).transpose()
    }
}

fn protected_skill_item_is_already_present(item: &Value, current_input: &str) -> bool {
    item.pointer("/_meta/chatos~1protectedSkillActivationRef")
        .and_then(Value::as_str)
        .is_some_and(|activation_ref| current_input.contains(activation_ref))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiReportedTaskOutcome {
    status: TaskExecutionOutcomeStatus,
    reason: String,
}

fn ai_reported_task_outcome_from_event(
    event: TaskRunEventRecord,
) -> Result<AiReportedTaskOutcome, String> {
    let payload = event
        .payload
        .ok_or_else(|| "task outcome event has no payload".to_string())?;
    let status = match payload.get("status").and_then(Value::as_str) {
        Some("succeeded") => TaskExecutionOutcomeStatus::Succeeded,
        Some("failed") => TaskExecutionOutcomeStatus::Failed,
        Some("blocked") => TaskExecutionOutcomeStatus::Blocked,
        Some(status) => return Err(format!("task outcome event has invalid status: {status}")),
        None => return Err("task outcome event has no status".to_string()),
    };
    let reason = payload
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .ok_or_else(|| "task outcome event has no reason".to_string())?
        .to_string();
    Ok(AiReportedTaskOutcome { status, reason })
}

fn task_outcome_report_required_message() -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": "[Task Outcome Required]\nThe previous response cannot finish this run because no explicit task outcome was reported. Call `task_run_process_report_outcome` now exactly once with `status` set to `succeeded`, `failed`, or `blocked` and a short concrete `reason`. Do not call any other tool. After that tool succeeds, provide the final user-facing response without doing more work."
        }]
    })
}

fn task_outcome_budget_exhausted_message() -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": "[Task Runner Finalization]\nThe execution-tool budget is exhausted. Do not perform more implementation or verification work. Call `task_run_process_report_outcome` now exactly once with the actual status (`succeeded`, `failed`, or `blocked`) and a concrete reason. Do not call any other tool."
        }]
    })
}

fn task_outcome_final_response_message() -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": "[Task Outcome Reported]\nThe task outcome has been recorded. Tools are now disabled. Provide the final user-facing response based on the completed work and the reported outcome. Do not perform more work or revise the reported status."
        }]
    })
}

fn task_execution_outcome_from_ai_report(
    content: &str,
    expected_acceptance_criteria: &[String],
    confirmed_project_paths: Vec<String>,
    confirmed_validation_commands: Vec<String>,
    confirmed_acceptance_tools: Vec<String>,
    pending_completion_requirements: std::collections::BTreeMap<String, String>,
    reported_outcome: AiReportedTaskOutcome,
) -> TaskExecutionOutcome {
    let summary = task_execution_summary(content);
    let paths = normalized_unique_strings(confirmed_project_paths);
    let commands = normalized_unique_strings(confirmed_validation_commands);
    let tools = normalized_unique_strings(confirmed_acceptance_tools);
    let criteria = normalized_unique_strings(expected_acceptance_criteria.to_vec());

    if reported_outcome.status == TaskExecutionOutcomeStatus::Succeeded
        && !pending_completion_requirements.is_empty()
    {
        let missing = pending_completion_requirements
            .iter()
            .map(|(id, verifier)| format!("{id} -> {verifier}"))
            .collect::<Vec<_>>();
        let reason = format!(
            "AI reported succeeded before required completion proof was recorded: {}",
            missing.join(", ")
        );
        return TaskExecutionOutcome {
            status: TaskExecutionOutcomeStatus::Blocked,
            summary,
            blocking_reason: Some(reason.clone()),
            unmet_acceptance_criteria: if criteria.is_empty() {
                vec![reason.clone()]
            } else {
                criteria
            },
            verification_evidence: vec![reason],
            acceptance_evidence: Vec::new(),
            referenced_paths: paths,
            referenced_endpoints: Vec::new(),
        };
    }

    let mut verification_evidence = vec![format!(
        "AI 显式上报任务终态 {}：{}",
        task_execution_outcome_status_name(reported_outcome.status),
        reported_outcome.reason
    )];
    verification_evidence.extend(
        commands
            .iter()
            .map(|command| format!("成功验证命令：{command}"))
            .collect::<Vec<_>>(),
    );
    if !paths.is_empty() {
        verification_evidence.push(format!("已确认项目路径：{}", paths.join("、")));
    }
    if !tools.is_empty() {
        verification_evidence.push(format!("成功验收工具：{}", tools.join("、")));
    }
    if reported_outcome.status != TaskExecutionOutcomeStatus::Succeeded {
        let unmet_acceptance_criteria = if criteria.is_empty() {
            vec![reported_outcome.reason.clone()]
        } else {
            criteria.clone()
        };
        return TaskExecutionOutcome {
            status: reported_outcome.status,
            summary,
            blocking_reason: Some(reported_outcome.reason),
            unmet_acceptance_criteria,
            verification_evidence,
            acceptance_evidence: Vec::new(),
            referenced_paths: paths,
            referenced_endpoints: Vec::new(),
        };
    }

    let acceptance_evidence = criteria
        .iter()
        .map(|criterion| TaskAcceptanceEvidence {
            criterion: criterion.clone(),
            evidence: vec![format!("本轮最终结果已覆盖验收项：{criterion}")],
            referenced_paths: paths.clone(),
            commands: commands.clone(),
            tool_names: tools.clone(),
        })
        .collect();
    TaskExecutionOutcome {
        status: TaskExecutionOutcomeStatus::Succeeded,
        summary,
        blocking_reason: None,
        unmet_acceptance_criteria: Vec::new(),
        verification_evidence,
        acceptance_evidence,
        referenced_paths: paths,
        referenced_endpoints: Vec::new(),
    }
}

fn task_execution_outcome_status_name(status: TaskExecutionOutcomeStatus) -> &'static str {
    match status {
        TaskExecutionOutcomeStatus::Succeeded => "succeeded",
        TaskExecutionOutcomeStatus::Failed => "failed",
        TaskExecutionOutcomeStatus::Blocked => "blocked",
        TaskExecutionOutcomeStatus::Cancelled => "cancelled",
    }
}

fn normalized_unique_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn task_execution_summary(content: &str) -> String {
    let summary = content
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with("```")
                && !matches!(*line, "完成结果" | "Result" | "Summary")
        })
        .unwrap_or("任务执行已结束。");
    summary.chars().take(1_000).collect()
}

fn persistent_review_checkpoint(
    active_review: &parking_lot::Mutex<Option<TaskExecutionReviewCheckpoint>>,
    detected_checkpoint: Option<TaskExecutionReviewCheckpoint>,
) -> Option<TaskExecutionReviewCheckpoint> {
    let mut active_review = active_review.lock();
    if let Some(checkpoint) = detected_checkpoint {
        *active_review = Some(checkpoint);
    }
    *active_review
}

fn checkpoint_guidance_message(
    checkpoint: TaskExecutionReviewCheckpoint,
    confirmed_project_paths: &[String],
) -> Value {
    let decision_focus = match checkpoint.trigger {
        TaskExecutionReviewTrigger::ReadOnlyLoop =>
            "归并现有文件与命令证据：如果它们已覆盖全部硬性验收项就 COMPLETE；否则锁定依赖顺序中第一项未满足要求，并选择 IMPLEMENT 或 VERIFY。",
        TaskExecutionReviewTrigger::MissingTargetedReads =>
            "已有工具结果否定了当前路径假设。选择 LOCATE，只执行一次限定目录、关键词和预期命中的定位动作；定位成功后直接转入 IMPLEMENT。",
        TaskExecutionReviewTrigger::PlaceholderProgressWrite =>
            "占位产物不是验收证据。忽略它并锁定第一项真实业务缺口；修改对应项目文件，或在既有证据完整时 COMPLETE。",
        TaskExecutionReviewTrigger::StaleProjectWrite =>
            "已有失败结果表明最近一次代码写入未生效。选择 IMPLEMENT，把最近一次成功读取的目标内容作为权威版本，直接生成基于该文本的精确编辑；写入成功后转入必要验证。",
    };
    let confirmed_path_guidance = if confirmed_project_paths.is_empty() {
        "当前还没有来自成功读取、搜索或修改结果的已确认项目路径。只有现有证据确实无法定位实现时，只允许执行一次限定目录、关键词和预期命中的 LOCATE 动作；该动作返回后必须使用新证据进入 IMPLEMENT、VERIFY、COMPLETE 或 BLOCKED，不得继续扩大搜索范围。".to_string()
    } else {
        format!(
            "以下路径已由成功的读取、搜索或修改工具结果确认，可直接作为后续动作的项目路径索引：{}。必须直接复用这些路径，不得从任务描述或自然语言摘要重新猜测路径，也不得仅为确认它们存在而重复全仓搜索。",
            serde_json::to_string(confirmed_project_paths)
                .expect("confirmed project paths must serialize")
        )
    };
    json!({
        "role": "system",
        "content": format!(
            "[工程决策复盘]\n\
             你现在承担工程复盘决策角色。当前上下文已经提供任务目标、验收标准、已读取文件、已执行命令及其结果。你的职责是依据这些证据替执行过程选定并推进下一步，而不是评价先前行为、提醒发生了重复，或输出复盘说明。\n\
             本次决策重点：{decision_focus}\n\
             路径证据：{confirmed_path_guidance}\n\
             \n\
             请在内部完成决策，不向用户展示分析草稿：\n\
             1. 重建验收契约：从任务目标和验收标准中提取硬性要求，使用现有文件内容、函数实现、命令、退出码和错误结果逐项判断。每个结论都必须有具体证据；没有证据的要求视为未满足。\n\
             2. 选择依赖顺序中第一项未满足要求。若有多个候选，选择一次动作最能直接关闭的缺口；若没有缺口且必要验证已通过，选择 COMPLETE。\n\
             3. 只选择一种状态：\n\
                - IMPLEMENT：已确认存在实现缺口。指令必须点明目标文件或函数、要修改的行为以及修改后的完成判据；下一步直接修改真实项目文件。\n\
                - LOCATE：只有现有证据无法定位真实实现时才能选择。指令必须限定一次目录列举或文本搜索的范围、关键词和期望定位结果。\n\
                - VERIFY：实现证据已存在，但最近一次真实修改之后仍缺一项必要验证。指令必须给出唯一的验证命令和明确通过条件。\n\
                - COMPLETE：全部硬性验收项已有代码证据，且相关必要验证已经通过。立即停止调用工具并输出最终结果。\n\
                - BLOCKED：只有外部输入、权限或服务状态确实阻止继续时才能选择。指令必须引用具体失败证据并说明需要什么变化；不得用 BLOCKED 代替困难实现。\n\
             4. 在内部形成唯一指令：`状态 + 证据依据 + 未满足的验收项 + 目标文件/函数或唯一命令 + 具体动作 + 完成判据`。若已有证据足以定位修改点，直接编辑；若已有成功结果足以证明验证项，直接采用该结果。\n\
             5. 做出判断后，下一条可见输出只能是执行该指令的一次工具调用，或 COMPLETE/BLOCKED 对应的最终结论；禁止输出批评、警告、过程复述或“需要继续检查”之类没有动作参数的文字。\n\
             6. 工具结果返回后继续使用同一决策契约：完成判据满足就进入 VERIFY 或 COMPLETE；失败就引用新失败的具体原因给出纠正后的 IMPLEMENT、LOCATE、VERIFY 或 BLOCKED 动作，不得只报告失败。\n\
             7. 相同代码状态下，已经成功的等价命令结果就是有效证据；除非代码发生变化或验证目标不同，不得重复运行或仅改写命令形式。工具始终完整可用，不得创建假进展文件或要求用户代为完成工程动作。"
        ),
    })
}

include!("runtime_state_part01.rs");

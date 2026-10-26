use async_trait::async_trait;
use chatos_ai_runtime::{AiRuntime, AiRuntimeResult, AiSingleStepOutcome, ModelRuntimeConfig};
use chatos_local_agent_host::{
    application::{skill_model_tools, task_model_tools, LocalAgentRuntime},
    infrastructure::SqliteClientStorage,
    ChatosAiRuntimeStepExecutor, ControlPlaneLocalAiStepPlanner, DurableAiProfile,
    ExecutedLocalAiStep, LocalAiStepExecutor, LocalAiStepPlanner, LocalControlPlaneSnapshot,
    LocalSkillToolExecutor, LocalTaskToolExecutor, LocalToolRegistry, LocalToolScheduler,
    NamedReadOnlyTools, ResolvedLocalCapabilities, ToolSchedulerTick, TransientLocalModelRuntime,
    AGENT_SKILL_TOOL_NAMES, TASK_APPROVAL_EXEMPT_TOOLS, TASK_READ_ONLY_TOOLS, TASK_TOOL_NAMES,
};
use chatos_local_agent_protocol::*;
use chatos_local_agent_runtime::LocalAgentProfile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;

pub const OWNER: &str = "eval-user";
pub const MAIN: &str = "000-eval-main";
pub const CONVERSATION: &str = "eval-conversation";
pub const SKILL: &str = "product-skill:chatos-async-task-orchestration";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayCall {
    pub name: String,
    #[serde(default = "empty_object")]
    pub arguments: Value,
}
fn empty_object() -> Value {
    json!({})
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayStep {
    #[serde(default)]
    pub calls: Vec<ReplayCall>,
    #[serde(default)]
    pub final_text: Option<String>,
}

#[derive(Serialize)]
pub struct Trace {
    pub step: usize,
    pub tool_name: String,
    pub arguments: Value,
    pub status: LocalAgentToolStatus,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub struct Session {
    pub runtime: Arc<LocalAgentRuntime>,
    pub control: Arc<LocalControlPlaneSnapshot>,
    pub scheduler: LocalToolScheduler,
    pub traces: Vec<Trace>,
    pub seed_ids: Vec<String>,
    pub final_text: Option<String>,
    pub failure: Option<String>,
    pub requests: usize,
}

pub async fn command(runtime: &LocalAgentRuntime, input: Value) -> Result<HostResult, String> {
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: format!("eval-{}", Uuid::new_v4()),
            command: serde_json::from_value(input).map_err(|_| "invalid_eval_command")?,
        })
        .await
        .map_err(|error| error.to_string())
}

pub fn planner(
    control: Arc<LocalControlPlaneSnapshot>,
) -> Result<ControlPlaneLocalAiStepPlanner, String> {
    let mut tools = task_model_tools();
    tools.extend(skill_model_tools());
    ControlPlaneLocalAiStepPlanner::main_chat(control.clone(), control).with_local_tools(tools)
}

fn safety() -> NamedReadOnlyTools {
    NamedReadOnlyTools::new(
        TASK_READ_ONLY_TOOLS
            .into_iter()
            .chain(AGENT_SKILL_TOOL_NAMES),
    )
    .with_approval_exempt(TASK_APPROVAL_EXEMPT_TOOLS)
}

pub fn live_profile(
    control: Arc<LocalControlPlaneSnapshot>,
) -> Result<Box<dyn LocalAgentProfile>, String> {
    Ok(Box::new(DurableAiProfile::new(
        ChatosAiRuntimeStepExecutor::new(planner(control)?),
        safety(),
    )))
}

pub fn replay_profile(
    control: Arc<LocalControlPlaneSnapshot>,
    steps: Vec<ReplayStep>,
) -> Result<Box<dyn LocalAgentProfile>, String> {
    Ok(Box::new(DurableAiProfile::new(
        ReplayExecutor {
            planner: planner(control)?,
            steps: Mutex::new(steps.into()),
        },
        safety(),
    )))
}

struct ReplayExecutor {
    planner: ControlPlaneLocalAiStepPlanner,
    steps: Mutex<VecDeque<ReplayStep>>,
}

#[async_trait]
impl LocalAiStepExecutor for ReplayExecutor {
    async fn execute_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<ExecutedLocalAiStep, String> {
        let prepared = self.planner.prepare_ai_step(claim).await?;
        let tool_definitions = prepared.request.model_request.tools.clone();
        let step = self
            .steps
            .lock()
            .await
            .pop_front()
            .ok_or("replay_exhausted")?;
        if step.calls.is_empty() == step.final_text.is_none() {
            return Err("replay_step_requires_calls_or_final".into());
        }
        for call in &step.calls {
            if !prepared
                .request
                .model_request
                .tools
                .iter()
                .any(|tool| tool["name"] == call.name)
            {
                return Err("replay_called_unavailable_tool".into());
            }
        }
        let calls: Vec<Value> = step
            .calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                json!({"call_id": format!("eval-call-{}-{index}", claim.run.iteration),
                   "name": call.name, "arguments": call.arguments})
            })
            .collect();
        let output_items = calls
            .iter()
            .map(|call| {
                json!({"type": "function_call", "call_id": call["call_id"], "name": call["name"],
                   "arguments": call["arguments"].to_string()})
            })
            .collect();
        let mut inputs = prepared.request.prefixed_input_items;
        inputs.extend(prepared.request.current_input_items);
        let response = AiRuntimeResult {
            content: step.final_text.unwrap_or_default(),
            reasoning: None,
            tool_calls: (!calls.is_empty()).then(|| json!(calls)),
            finish_reason: Some("stop".into()),
            usage: None,
            response_id: None,
            response_output_items: output_items,
            request_input_items: inputs,
        };
        let outcome = if calls.is_empty() {
            AiSingleStepOutcome::Final(response)
        } else {
            AiSingleStepOutcome::ToolCommand {
                response,
                tool_calls: json!(calls),
            }
        };
        Ok(ExecutedLocalAiStep {
            outcome,
            tool_definitions,
        })
    }
}

impl Session {
    pub async fn new(
        snapshots: &[LocalCapabilityPolicySnapshot],
        config: ModelRuntimeConfig,
        prompt: &str,
    ) -> Result<Self, String> {
        if snapshots.len() != 5
            || snapshots
                .iter()
                .any(|snapshot| snapshot.owner_user_id != OWNER)
        {
            return Err("invalid_eval_snapshot_set".into());
        }
        let main = snapshots
            .iter()
            .find(|snapshot| snapshot.profile_key == "main_chat")
            .ok_or("missing_main_snapshot")?;
        let revision = main.capability_policy_revision.clone();
        let mut profiles = snapshots
            .iter()
            .map(|s| s.profile_key.as_str())
            .collect::<Vec<_>>();
        profiles.sort_unstable();
        if profiles
            != [
                "main_chat",
                "skill_bundle_internal",
                "task_execution",
                "task_policy_internal",
                "task_skill_bundle_internal",
            ]
        {
            return Err("invalid_eval_snapshot_profiles".into());
        }
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .map_err(|e| e.to_string())?,
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage.clone()));
        runtime.initialize(OWNER).await.map_err(|e| e.to_string())?;
        let control = Arc::new(LocalControlPlaneSnapshot::new().with_capability_store(storage));
        for snapshot in snapshots {
            if snapshot.capability_policy_revision != revision {
                return Err("mixed_eval_snapshot_revisions".into());
            }
            snapshot.validate()?;
            if snapshot.profile_key.ends_with("skill_bundle_internal") {
                let bundle: LocalSkillBundleSnapshot = serde_json::from_str(
                    snapshot
                        .instructions
                        .as_deref()
                        .ok_or("missing_skill_bundle")?,
                )
                .map_err(|_| "invalid_skill_bundle")?;
                bundle.validate()?;
            }
            control
                .publish_capabilities(
                    OWNER,
                    &snapshot.profile_key,
                    &revision,
                    ResolvedLocalCapabilities {
                        instructions: snapshot.instructions.clone(),
                        prefixed_input_items: snapshot.prefixed_input_items.clone(),
                        tools: snapshot.tools.clone(),
                    },
                )
                .await?;
        }
        // Live credentials stay transient. The durable model is intentionally incapable of network execution.
        command(&runtime, json!({"type": "put_model_config_snapshot", "snapshot": {
            "owner_user_id": OWNER, "model_config_ref": "eval-model",
            "model_config_revision": "eval-model-v1", "credential_ref": "env:UNUSED_EVAL_CREDENTIAL",
            "base_url": "https://example.invalid", "model": "eval-placeholder", "provider": "gpt",
            "supports_responses": true, "include_prompt_cache_retention": false
        }})).await?;
        control.publish_model_runtime(
            OWNER,
            "eval-model",
            "eval-model-v1",
            TransientLocalModelRuntime {
                runner: Arc::new(AiRuntime::builder().build_contextual_turn_runner()),
                model_config: config,
            },
        )?;
        command(
            &runtime,
            json!({"type": "create_conversation", "conversation_id": CONVERSATION,
                "owner_user_id": OWNER, "title": "Isolated orchestration evaluation",
                "resource": {"kind": "project", "resource_id": "eval-project"}
            }),
        )
        .await?;
        command(&runtime, json!({"type": "start_conversation_turn", "owner_user_id": OWNER,
            "conversation_id": CONVERSATION, "expected_conversation_version": 1,
            "turn_id": "eval-turn", "message_id": "eval-message", "run_id": MAIN, "message": prompt,
            "model_config_ref": "eval-model", "model_config_revision": "eval-model-v1",
            "capability_policy_revision": revision, "max_iterations": 32
        })).await?;
        let mut tools = LocalToolRegistry::new();
        let tasks = Arc::new(LocalTaskToolExecutor::new(runtime.clone(), OWNER)?);
        for name in TASK_TOOL_NAMES {
            tools.register_shared(name, tasks.clone())?;
        }
        LocalSkillToolExecutor::new(runtime.clone(), OWNER)?.register_into(&mut tools)?;
        let scheduler = LocalToolScheduler::new(runtime.clone(), tools, OWNER, "eval-tools")?
            .with_tool_filter(
                Some(
                    TASK_TOOL_NAMES
                        .into_iter()
                        .chain(AGENT_SKILL_TOOL_NAMES)
                        .map(str::to_string)
                        .collect(),
                ),
                Vec::new(),
            )?
            .with_execution_route(LocalAgentToolExecutionRoute::Shared);
        Ok(Self {
            runtime,
            control,
            scheduler,
            traces: vec![],
            seed_ids: vec![],
            final_text: None,
            failure: None,
            requests: 0,
        })
    }

    pub async fn step(&mut self, profile: &dyn LocalAgentProfile) -> Result<bool, String> {
        let HostResult::Claim { claim: Some(claim) } = command(
            &self.runtime,
            json!({
                "type": "claim_next_run", "owner_user_id": OWNER, "worker_id": "eval-model",
                "lease_duration_ms": 300000
            }),
        )
        .await?
        else {
            return Err("eval_main_not_claimable".into());
        };
        // Main was created before all generated Task Runs. Abort rather than executing ANY other Run.
        if claim.run.run_id != MAIN || claim.run.profile_key != "main_chat" {
            return Err("business_run_claimed_evaluation_aborted".into());
        }
        let prepared = planner(self.control.clone())?
            .prepare_ai_step(&claim)
            .await?;
        let available = prepared.request.model_request.tools;
        self.requests += 1;
        let outcome = tokio::time::timeout(Duration::from_secs(45), profile.execute_step(&claim))
            .await
            .map_err(|_| "model_step_timeout")?
            .map_err(|error| {
                if error == "replay_called_unavailable_tool" {
                    error
                } else {
                    "model_step_error".into()
                }
            })?;
        if let LocalAgentStepOutcome::WaitForTool { tool_calls, .. } = &outcome {
            if tool_calls.len() > 32
                || tool_calls.iter().any(|call| {
                    !available.iter().any(|tool| tool["name"] == call.tool_name)
                        || !TASK_TOOL_NAMES.contains(&call.tool_name.as_str())
                            && !AGENT_SKILL_TOOL_NAMES.contains(&call.tool_name.as_str())
                })
            {
                return Err("model_called_unavailable_tool".into());
            }
        }
        let final_output = match &outcome {
            LocalAgentStepOutcome::Succeed { output } => {
                Some(output["content"].as_str().unwrap_or("").to_owned())
            }
            LocalAgentStepOutcome::WaitForTool { .. } | LocalAgentStepOutcome::Continue { .. } => {
                None
            }
            _ => return Err("model_did_not_complete_planning".into()),
        };
        command(&self.runtime, json!({"type": "commit_step", "owner_user_id": OWNER,
            "run_id": MAIN, "claim_token": claim.claim_token, "expected_version": claim.run.version,
            "outcome": outcome
        })).await?;
        if let Some(text) = final_output {
            self.final_text = Some(text);
            return Ok(true); // Crucially, NEVER claim again after Main Chat terminates.
        }
        for _ in 0..33 {
            match self.scheduler.run_once().await.map_err(|e| e.to_string())? {
                ToolSchedulerTick::Idle => return Ok(false),
                ToolSchedulerTick::Committed(result) => {
                    if result.invocation.run_id != MAIN {
                        return Err("foreign_tool_invocation".into());
                    }
                    self.traces.push(Trace {
                        step: self.requests,
                        tool_name: result.invocation.tool_name,
                        arguments: result.invocation.arguments,
                        status: result.invocation.status,
                        result: result.invocation.result,
                        error: result.invocation.error,
                    });
                }
            }
        }
        Err("tool_budget_exceeded".into())
    }

    pub async fn seed_followup(&mut self, seed: &Value, prompt: &str) -> Result<(), String> {
        let steps = vec![
            ReplayStep {
                calls: vec![ReplayCall {
                    name: "agent_skill_activate".into(),
                    arguments: json!({"skill_ref": SKILL}),
                }],
                final_text: None,
            },
            ReplayStep {
                calls: vec![ReplayCall {
                    name: "create_task".into(),
                    arguments: seed.clone(),
                }],
                final_text: None,
            },
        ];
        let profile = replay_profile(self.control.clone(), steps)?;
        for _ in 0..2 {
            self.step(profile.as_ref()).await?;
        }
        let tasks = self.tasks().await?;
        if tasks.len() != 1 {
            return Err("invalid_followup_seed".into());
        }
        self.seed_ids = tasks.iter().map(|task| task.task_id.clone()).collect();
        let HostResult::Conversation { conversation } = command(
            &self.runtime,
            json!({
                "type": "get_conversation", "owner_user_id": OWNER, "conversation_id": CONVERSATION
            }),
        )
        .await?
        else {
            return Err("missing_eval_conversation".into());
        };
        command(&self.runtime, json!({"type": "guide_conversation_turn", "owner_user_id": OWNER,
            "conversation_id": CONVERSATION, "expected_conversation_version": conversation.conversation.version,
            "turn_id": "eval-turn", "message_id": "eval-followup", "message": prompt
        })).await?;
        self.requests = 0;
        self.traces.clear();
        Ok(())
    }

    pub async fn tasks(&self) -> Result<Vec<LocalTaskRecord>, String> {
        self.runtime
            .list_tasks_for_conversation(
                OWNER,
                CONVERSATION,
                None,
                None,
                None,
                None,
                None,
                None,
                500,
                0,
            )
            .await
            .map_err(|e| e.to_string())
    }
}

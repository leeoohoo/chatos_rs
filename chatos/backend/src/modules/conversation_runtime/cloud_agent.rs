// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chatos_agent::ChatosAgentProfile;
use chatos_ai_runtime::{
    AiRuntimeOptions, ContextualTurnRequest, RuntimeCallbacks, RuntimeLifecycleHook,
};
use chatos_cloud_agent_protocol::{CloudAgentRunRecord, CloudAgentRunStatus};
use chatos_cloud_agent_runtime::{
    cloud_agent_mcp_result_callback_payload, cloud_agent_trigger_execution_identity,
    cloud_agent_trigger_input_items, create_cloud_agent_run, CloudAgentModelTrigger,
    CloudAgentProfile, CloudAgentProfileRegistry, CloudAgentServiceRuntime,
    CloudAgentSingleStepExecution, CloudAgentSingleStepOutput, NewCloudAgentRun,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tracing::warn;
use uuid::Uuid;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::chat_stream::build_chat_stream_callbacks;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::memory_runtime_types::TurnRuntimeSnapshotPluginCommandInvocationDto;
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::build_user_content_parts;
use crate::services::chatos_memory_engine::resolve_chatos_memory_scope;
use crate::services::shared_ai_runtime::{
    build_shared_contextual_turn_runner_with_max_iterations,
    shared_model_runtime_config_from_resolved, ChatosMemoryRecordWriterAdapter,
};
use crate::utils::abort_registry;
use crate::utils::attachments::Attachment;

use super::chat_execution::{
    build_chatos_record_options, cloud_task_turn_review_metadata,
    cloud_track_project_execution_planner_completion, cloud_track_project_planning_integrity,
    compose_agent_instructions, CloudChatosRuntimeLifecycleHook, CloudTaskTurnLifecycleState,
    PreparedMcpExecution,
};
use super::chat_runner::{build_chat_event_sink, finalize_chat_result};
use super::runtime_context::{resume_mcp_management_gateway, ResolvedConversationRuntimeContext};

pub const CHATOS_CLOUD_AGENT_ROUTING_KEY: &str = "cloud_agent.chatos.runtime";
pub const CHATOS_CLOUD_AGENT_RETRY_ROUTING_KEY: &str = "cloud_agent.chatos.runtime.retry";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatosCloudAgentRunInput {
    session_id: String,
    turn_id: String,
    user_message_id: String,
    owner_user_id: String,
    project_id: Option<String>,
    content: String,
    persisted_user_message_content: Option<String>,
    persisted_user_message_metadata: Option<Value>,
    attachments: Vec<Attachment>,
    model_config_id: String,
    model_name: String,
    model_provider: String,
    prompt_vendor: Option<String>,
    plan_mode: bool,
    project_requirement_execution_planner: bool,
    effective_settings: Value,
    max_tokens: Option<i64>,
    prefixed_input_items: Vec<Value>,
    unavailable_tools: Vec<Value>,
    base_system_prompt: Option<String>,
    agent_system_prompt: Option<String>,
    contact_system_prompt: Option<String>,
    builtin_mcp_system_prompt: Option<String>,
    internal_context_locale: InternalContextLocale,
    user_output_locale: InternalContextLocale,
    contact_agent_id: Option<String>,
    resolved_project_id: Option<String>,
    resolved_project_name: Option<String>,
    resolved_project_root: Option<String>,
    default_remote_connection_id: Option<String>,
    workspace_root: Option<String>,
    enabled_mcp_ids_for_snapshot: Vec<String>,
    plugin_command_invocations_for_snapshot: Vec<TurnRuntimeSnapshotPluginCommandInvocationDto>,
    mcp_session_id: String,
    mcp_command_queue: String,
    max_iterations: usize,
    max_task_follow_up_rounds: usize,
    lifecycle: CloudTaskTurnLifecycleState,
    owner_context: Option<Value>,
}

impl ChatosCloudAgentRunInput {
    fn profile(&self) -> ChatosAgentProfile {
        ChatosAgentProfile::from_flags(self.plan_mode, self.project_requirement_execution_planner)
    }

    fn validate_identity(&self, run: &CloudAgentRunRecord) -> Result<(), String> {
        if self.session_id != run.owner_entity_id
            || self.owner_user_id != run.owner_user_id
            || self.model_config_id != run.model_config_ref
            || self.profile().key().as_str() != run.agent_key
            || self.mcp_session_id != run.mcp_runtime_session_ref.as_deref().unwrap_or_default()
        {
            return Err("ChatOS Cloud Agent persisted input identity changed".to_string());
        }
        Ok(())
    }
}

pub struct StartChatosCloudAgent<'a> {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub user_message_id: &'a str,
    pub content: &'a str,
    pub persisted_user_message_content: Option<String>,
    pub persisted_user_message_metadata: Option<Value>,
    pub attachments: Vec<Attachment>,
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub effective_settings: Value,
    pub max_tokens: Option<i64>,
    pub runtime_context: &'a ResolvedConversationRuntimeContext,
    pub prepared_mcp: PreparedMcpExecution,
    pub owner_context: Option<Value>,
}

pub async fn start_chatos_cloud_agent(input: StartChatosCloudAgent<'_>) -> Result<String, String> {
    let owner_user_id = required_text(input.user_id.as_deref(), "owner_user_id")?.to_string();
    let model_config_id = required_text(
        input.model_runtime.model_config_id.as_deref(),
        "model_config_id",
    )?
    .to_string();
    let mcp_session_id = input
        .runtime_context
        .mcp_management_runtime_session
        .as_ref()
        .map(|session| session.session_id().to_string())
        .ok_or_else(|| "ChatOS Cloud Agent requires an MCP runtime session".to_string())?;
    let mcp_command_queue = required_text(
        input.runtime_context.mcp_command_queue.as_deref(),
        "mcp_command_queue",
    )?
    .to_string();
    let agent_profile = input.runtime_context.agent_profile;
    let max_iterations =
        super::chat_execution::max_iterations_from_settings(&input.effective_settings);
    let lifecycle = CloudTaskTurnLifecycleState {
        project_execution_planner_guard: input
            .runtime_context
            .project_requirement_execution_planner,
        project_planning_integrity_guard: agent_profile.plan_mode_header()
            && !input.runtime_context.project_requirement_execution_planner,
        ..CloudTaskTurnLifecycleState::default()
    };
    let run_input = ChatosCloudAgentRunInput {
        session_id: input.session_id.to_string(),
        turn_id: input.turn_id.to_string(),
        user_message_id: input.user_message_id.to_string(),
        owner_user_id: owner_user_id.clone(),
        project_id: input.project_id,
        content: input.content.to_string(),
        persisted_user_message_content: input.persisted_user_message_content,
        persisted_user_message_metadata: input.persisted_user_message_metadata.clone(),
        attachments: input.attachments,
        model_config_id: model_config_id.clone(),
        model_name: input.model_runtime.model.clone(),
        model_provider: input.model_runtime.provider.clone(),
        prompt_vendor: input.model_runtime.prompt_vendor.clone(),
        plan_mode: agent_profile.plan_mode_header(),
        project_requirement_execution_planner: input
            .runtime_context
            .project_requirement_execution_planner,
        effective_settings: input.effective_settings.clone(),
        max_tokens: input.max_tokens,
        prefixed_input_items: input.prepared_mcp.prefixed_input_items,
        unavailable_tools: input.prepared_mcp.unavailable_tools,
        base_system_prompt: input.runtime_context.base_system_prompt.clone(),
        agent_system_prompt: input.runtime_context.agent_system_prompt.clone(),
        contact_system_prompt: input.runtime_context.contact_system_prompt.clone(),
        builtin_mcp_system_prompt: input.runtime_context.builtin_mcp_system_prompt.clone(),
        internal_context_locale: input.runtime_context.internal_context_locale,
        user_output_locale: input.runtime_context.user_output_locale,
        contact_agent_id: input.runtime_context.contact_agent_id.clone(),
        resolved_project_id: input.runtime_context.resolved_project_id.clone(),
        resolved_project_name: input.runtime_context.resolved_project_name.clone(),
        resolved_project_root: input.runtime_context.resolved_project_root.clone(),
        default_remote_connection_id: input.runtime_context.default_remote_connection_id.clone(),
        workspace_root: input.runtime_context.workspace_root.clone(),
        enabled_mcp_ids_for_snapshot: input.runtime_context.enabled_mcp_ids_for_snapshot.clone(),
        plugin_command_invocations_for_snapshot: input
            .runtime_context
            .plugin_command_invocations_for_snapshot
            .clone(),
        mcp_session_id: mcp_session_id.clone(),
        mcp_command_queue: mcp_command_queue.clone(),
        max_iterations,
        max_task_follow_up_rounds: super::chat_execution::task_follow_up_max_rounds_from_settings(
            &input.effective_settings,
        ),
        lifecycle,
        owner_context: input.owner_context,
    };
    let agent_run_id = Uuid::new_v4().to_string();
    let prompt_text = run_input.agent_system_prompt.as_deref().unwrap_or_default();
    let prompt_checksum = hex::encode(Sha256::digest(prompt_text.as_bytes()));
    let store = crate::modules::cloud_agent_runtime::store()?;
    create_cloud_agent_run(
        &store,
        NewCloudAgentRun {
            ordering_lane_key: format!("conversation:{}", input.session_id),
            agent_run_id: agent_run_id.clone(),
            owner_service: "chatos".to_string(),
            owner_entity_type: "conversation".to_string(),
            owner_entity_id: input.session_id.to_string(),
            owner_user_id,
            agent_key: agent_profile.key().as_str().to_string(),
            input: serde_json::to_value(&run_input)
                .map_err(|error| format!("encode ChatOS Cloud Agent input failed: {error}"))?,
            model_config_ref: model_config_id,
            model_runtime_snapshot_ref: format!(
                "{}:{}",
                input.model_runtime.provider, input.model_runtime.model
            ),
            agent_prompt_revision: "managed-current".to_string(),
            agent_prompt_checksum: prompt_checksum,
            capability_policy_revision: "mcp-runtime-session".to_string(),
            mcp_runtime_session_ref: Some(mcp_session_id),
            current_input_items_ref: format!(
                "conversation:{}:turn:{}:initial",
                input.session_id, input.turn_id
            ),
            max_iterations: u32::try_from(max_iterations).unwrap_or(u32::MAX),
            deadline_at: None,
            runtime_routing_key: CHATOS_CLOUD_AGENT_ROUTING_KEY.to_string(),
            start_causation_id: input.user_message_id.to_string(),
            start_payload: json!({
                "conversation_id": input.session_id,
                "turn_id": input.turn_id,
                "user_message_id": input.user_message_id,
            }),
        },
    )
    .await?;
    Ok(agent_run_id)
}

#[derive(Clone, Copy)]
pub struct ChatosCloudAgentAdapter;

#[async_trait]
impl CloudAgentProfile for ChatosCloudAgentAdapter {
    async fn execute_single_step(
        &self,
        run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        let mut input: ChatosCloudAgentRunInput = serde_json::from_value(run.input.clone())
            .map_err(|error| format!("decode ChatOS Cloud Agent input failed: {error}"))?;
        input.validate_identity(run)?;
        let mut model_runtime =
            crate::services::model_runtime_resolver::resolve_model_runtime_for_request(
                Some(input.model_config_id.as_str()),
                None,
                Some(input.session_id.as_str()),
                Some(input.owner_user_id.as_str()),
                input.model_name.as_str(),
                None,
                true,
            )
            .await?;
        if model_runtime.model != input.model_name || model_runtime.provider != input.model_provider
        {
            return Err("ChatOS model identity changed during the Cloud Agent run".to_string());
        }
        model_runtime.system_prompt = input.base_system_prompt.clone();

        let resumed = resume_mcp_management_gateway(input.mcp_session_id.as_str()).await?;
        if resumed.command_queue != input.mcp_command_queue {
            return Err("ChatOS MCP command queue changed during the run".to_string());
        }
        let mut runtime_context =
            reconstructed_runtime_context(&input, resumed.server, resumed.runtime_session);
        let prepared = super::chat_execution::prepare_mcp_execution(
            input.session_id.as_str(),
            input.turn_id.as_str(),
            &mut runtime_context,
            false,
        )
        .await?;
        let sink = build_chat_event_sink(
            None,
            Some(input.owner_user_id.clone()),
            input.session_id.as_str(),
            Some(input.turn_id.clone()),
            input.project_id.clone(),
            Some(input.user_message_id.clone()),
        );
        let stream_callbacks = build_chat_stream_callbacks(&sink, input.session_id.as_str(), true);
        let lifecycle_state = Arc::new(Mutex::new(input.lifecycle.clone()));
        let mut callbacks = shared_callbacks(stream_callbacks.clone());
        callbacks = cloud_track_project_planning_integrity(callbacks, Arc::clone(&lifecycle_state));
        if input.project_requirement_execution_planner {
            callbacks = cloud_track_project_execution_planner_completion(
                callbacks,
                Arc::clone(&lifecycle_state),
            );
        }
        if let CloudAgentModelTrigger::ToolResults { items, .. } = trigger {
            persist_mcp_tool_results(
                &input,
                run,
                run.pending_tool_calls.as_slice(),
                items.as_slice(),
            )
            .await?;
            if let Some(on_start) = callbacks.on_tools_start.as_ref() {
                on_start(Value::Array(run.pending_tool_calls.clone()));
            }
            if let Some(on_end) = callbacks.on_tools_end.as_ref() {
                on_end(cloud_agent_mcp_result_callback_payload(
                    run.pending_tool_calls.as_slice(),
                    items.as_slice(),
                )?);
            }
        }
        let lifecycle = Arc::new(CloudChatosRuntimeLifecycleHook {
            session_id: input.session_id.clone(),
            turn_id: input.turn_id.clone(),
            model_name: input.model_name.clone(),
            supports_images: Some(model_runtime.supports_images),
            callbacks: stream_callbacks,
            max_task_follow_up_rounds: input.max_task_follow_up_rounds,
            task_turn: Arc::clone(&lifecycle_state),
        }) as Arc<dyn RuntimeLifecycleHook>;
        let shared_model_config = shared_model_runtime_config_from_resolved(&model_runtime)
            .with_instructions(compose_agent_instructions(&runtime_context, &model_runtime))
            .with_max_output_tokens(input.max_tokens)
            .with_prompt_cache_key(Some(input.session_id.clone()))
            .with_previous_response_id(None)
            .with_request_cwd(None)
            .with_prompt_cache_retention(true);
        let initial_items = vec![json!({
            "type": "message",
            "role": "user",
            "content": build_user_content_parts(
                shared_model_config.model.as_str(),
                input.content.as_str(),
                input.attachments.as_slice(),
                shared_model_config.supports_images,
            ).await,
        })];
        let current_input_items = cloud_agent_trigger_input_items(run, trigger, initial_items)?;
        let retry_input_items = current_input_items.clone();
        let hidden_turn = input
            .persisted_user_message_metadata
            .as_ref()
            .and_then(|metadata| metadata.get("hidden"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let record_identity = format!(
            "cloud-agent:{}:{}:{}",
            run.ordering.agent_run_id, run.ordering.generation, run.ordering.step_seq
        );
        let runtime_options =
            AiRuntimeOptions::new(Some(input.session_id.clone()), Some(input.turn_id.clone()))
                .with_caller_model_runtime(Some(shared_model_config.to_tool_caller_model_runtime()))
                .with_abort_checker(Some(Arc::new(|session_id: &str| {
                    abort_registry::is_aborted(session_id)
                })))
                .with_abort_token(abort_registry::abort_token_for_turn(
                    input.session_id.as_str(),
                    Some(input.turn_id.as_str()),
                ))
                .with_callbacks(callbacks)
                .with_lifecycle_hook(Some(lifecycle))
                .with_record_options(
                    build_chatos_record_options(
                        crate::services::ai_common::TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
                        input.model_name.as_str(),
                        hidden_turn,
                    )
                    .with_assistant_message_id(format!("{record_identity}:assistant"))
                    .with_tool_message_id_prefix(format!("{record_identity}:tool")),
                );
        let tools = if runtime_context.use_tools {
            prepared.executor.get_available_tools()
        } else {
            Vec::new()
        };
        let request = ContextualTurnRequest::new(
            shared_model_config.to_model_request(Value::Null, tools),
            runtime_options,
            current_input_items,
        )
        .with_memory_scope(resolve_chatos_memory_scope(input.session_id.as_str()).await?)
        .with_prefixed_input_items(input.prefixed_input_items.clone());
        let runner = build_shared_contextual_turn_runner_with_max_iterations(
            runtime_context.use_tools.then_some(prepared.executor),
            MessageManager::new(),
            input.max_iterations,
        )?;
        let (reason, model_attempt) = cloud_agent_trigger_execution_identity(trigger);
        let outcome = runner
            .execute_once(
                request,
                usize::try_from(run.iteration.saturating_add(1)).unwrap_or(usize::MAX),
                reason,
                model_attempt,
            )
            .await?;
        input.lifecycle = lifecycle_state
            .lock()
            .map_err(|_| "ChatOS lifecycle state lock poisoned".to_string())?
            .clone();
        let next_input = serde_json::to_value(&input)
            .map_err(|error| format!("encode ChatOS Cloud Agent input failed: {error}"))?;
        Ok(CloudAgentSingleStepExecution::Apply(
            CloudAgentSingleStepOutput::new(outcome)
                .with_mcp_runtime(input.mcp_session_id, input.mcp_command_queue)
                .with_retry_input_items(retry_input_items)
                .with_next_input(next_input),
        ))
    }

    async fn finalize_terminal(&self, run: &CloudAgentRunRecord) -> Result<(), String> {
        finalize_terminal(run).await
    }
}

fn runtime() -> Result<CloudAgentServiceRuntime<CloudAgentProfileRegistry>, String> {
    let store = crate::modules::cloud_agent_runtime::store()?;
    let registry = CloudAgentProfileRegistry::new("chatos", store).register(
        [
            SystemAgentKey::ChatosConversationAgent.as_str(),
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent.as_str(),
        ],
        ChatosCloudAgentAdapter,
    )?;
    Ok(CloudAgentServiceRuntime::new(
        registry,
        CHATOS_CLOUD_AGENT_ROUTING_KEY,
    ))
}

pub fn spawn_outbox_reconciler() -> Result<JoinHandle<()>, String> {
    let topology = topology()?;
    Ok(chatos_cloud_agent_runtime::spawn_cloud_agent_outbox_reconciler(topology, runtime()?))
}

pub fn spawn_consumer() -> Result<JoinHandle<()>, String> {
    let topology = topology()?;
    Ok(chatos_cloud_agent_runtime::spawn_cloud_agent_consumer(
        topology,
        runtime()?,
    ))
}

fn topology() -> Result<chatos_cloud_agent_runtime::CloudAgentRabbitMqTopology, String> {
    let cfg = crate::config::Config::try_get()?;
    let namespace = cfg.mcp_result_queue_prefix.trim_end_matches('.');
    Ok(chatos_cloud_agent_runtime::CloudAgentRabbitMqTopology {
        rabbitmq_url: cfg.mcp_result_rabbitmq_url.clone(),
        exchange: format!("{namespace}.cloud_agent"),
        runtime_queue: CHATOS_CLOUD_AGENT_ROUTING_KEY.to_string(),
        retry_queue: CHATOS_CLOUD_AGENT_RETRY_ROUTING_KEY.to_string(),
        consumer_tag: "chatos-cloud-agent-runtime".to_string(),
        reconnect_delay: Duration::from_secs(3),
        outbox_reconcile_interval: Duration::from_secs(1),
        outbox_batch_size: 100,
        prefetch_count: 32,
        consumer_concurrency: 4,
        conflict_retry_delay: Duration::from_secs(1),
    })
}

async fn finalize_terminal(run: &CloudAgentRunRecord) -> Result<(), String> {
    let agent_run_id = run.ordering.agent_run_id.as_str();
    let input: ChatosCloudAgentRunInput = serde_json::from_value(run.input.clone())
        .map_err(|error| format!("decode ChatOS Cloud Agent input failed: {error}"))?;
    input.validate_identity(run)?;
    if let Ok(resumed) = resume_mcp_management_gateway(input.mcp_session_id.as_str()).await {
        if let Err(error) = resumed.runtime_session.close().await {
            warn!(agent_run_id, error = %error, "close ChatOS MCP runtime session failed");
        }
    }
    if let Some(owner_context) = input.owner_context.clone() {
        let owner_context = enrich_owner_context_with_terminal_outcome(owner_context, run);
        if let Err(error) =
            crate::api::projects::reconcile_requirement_planner_owner_context(owner_context).await
        {
            warn!(
                agent_run_id,
                error = error.as_str(),
                "reconcile requirement planner terminal outcome failed"
            );
        }
    }
    let sink = build_chat_event_sink(
        None,
        Some(input.owner_user_id.clone()),
        input.session_id.as_str(),
        Some(input.turn_id.clone()),
        input.project_id.clone(),
        Some(input.user_message_id.clone()),
    );
    let result = if run.status == CloudAgentRunStatus::Succeeded {
        let outcome = run.terminal_outcome.clone().unwrap_or(Value::Null);
        Ok(crate::services::ai_common::attach_ai_client_success_extra(
            crate::services::ai_common::build_ai_client_success_payload(
                outcome
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                outcome
                    .get("reasoning")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                outcome
                    .get("finish_reason")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                0,
            ),
            cloud_task_turn_review_metadata(&input.lifecycle),
        ))
    } else {
        Err(run
            .terminal_outcome
            .as_ref()
            .and_then(|outcome| outcome.get("error"))
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                if run.status == CloudAgentRunStatus::Cancelled {
                    "aborted"
                } else {
                    "ChatOS Cloud Agent failed"
                }
            })
            .to_string())
    };
    let chunk_sent = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let streamed_content = Arc::new(Mutex::new(String::new()));
    finalize_chat_result(
        &sink,
        input.session_id.as_str(),
        input.turn_id.as_str(),
        input.user_message_id.as_str(),
        true,
        task_runner_async_success_status_for_lifecycle(&input.lifecycle),
        &chunk_sent,
        &streamed_content,
        result,
        false,
        || crate::utils::log_helpers::log_chat_cancelled(input.session_id.as_str()),
        crate::utils::log_helpers::log_chat_error,
    )
    .await;
    super::guidance::close_active_turn(input.session_id.as_str(), input.turn_id.as_str());
    Ok(())
}

fn enrich_owner_context_with_terminal_outcome(
    mut owner_context: Value,
    run: &CloudAgentRunRecord,
) -> Value {
    let Some(owner) = owner_context.as_object_mut() else {
        return owner_context;
    };
    if let Ok(status) = serde_json::to_value(run.status) {
        owner.insert("agent_run_status".to_string(), status);
    }
    if let Some(error) = run
        .terminal_outcome
        .as_ref()
        .and_then(|outcome| outcome.get("error"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        owner.insert(
            "agent_run_error".to_string(),
            Value::String(error.to_string()),
        );
    }
    owner_context
}

fn task_runner_async_success_status_for_lifecycle(
    lifecycle: &CloudTaskTurnLifecycleState,
) -> Option<&'static str> {
    (lifecycle.project_planning_task_created || lifecycle.project_execution_plan_materialized)
        .then_some("processing")
}

fn reconstructed_runtime_context(
    input: &ChatosCloudAgentRunInput,
    server: crate::services::mcp_loader::McpHttpServer,
    runtime_session: chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle,
) -> ResolvedConversationRuntimeContext {
    ResolvedConversationRuntimeContext {
        agent_profile: input.profile(),
        internal_context_locale: input.internal_context_locale,
        user_output_locale: input.user_output_locale,
        contact_agent_id: input.contact_agent_id.clone(),
        base_system_prompt: input.base_system_prompt.clone(),
        agent_system_prompt: input.agent_system_prompt.clone(),
        contact_system_prompt: input.contact_system_prompt.clone(),
        builtin_mcp_system_prompt: input.builtin_mcp_system_prompt.clone(),
        plugin_instruction_items: Vec::new(),
        selected_commands_for_snapshot: Arc::new(Mutex::new(Vec::new())),
        plugin_command_invocations_for_snapshot: input
            .plugin_command_invocations_for_snapshot
            .clone(),
        resolved_project_id: input.resolved_project_id.clone(),
        resolved_project_name: input.resolved_project_name.clone(),
        resolved_project_root: input.resolved_project_root.clone(),
        default_remote_connection_id: input.default_remote_connection_id.clone(),
        workspace_root: input.workspace_root.clone(),
        mcp_enabled: true,
        enabled_mcp_ids_for_snapshot: input.enabled_mcp_ids_for_snapshot.clone(),
        mcp_server_bundle: (vec![server], Vec::new(), Vec::new()),
        mcp_management_runtime_session: Some(runtime_session),
        mcp_command_queue: Some(input.mcp_command_queue.clone()),
        use_tools: true,
        memory_summary_prompt: None,
        runtime_error: None,
        project_requirement_execution_planner: input.project_requirement_execution_planner,
    }
}

fn shared_callbacks(callbacks: AiClientCallbacks) -> RuntimeCallbacks {
    super::chat_execution::shared_runtime_callbacks_from_chatos(&callbacks)
}

fn required_text<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required for ChatOS Cloud Agent"))
}

async fn persist_mcp_tool_results(
    input: &ChatosCloudAgentRunInput,
    run: &CloudAgentRunRecord,
    calls: &[Value],
    results: &[Value],
) -> Result<(), String> {
    use chatos_ai_runtime::MemoryRecordWriter;

    if calls.len() != results.len() {
        return Err("MCP aggregate result count does not match pending tool calls".to_string());
    }
    let records = calls
        .iter()
        .zip(results)
        .enumerate()
        .map(|(index, (call, result))| {
            let tool_call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
                .ok_or_else(|| format!("pending tool call {index} has no id"))?;
            let tool_name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)
                .ok_or_else(|| format!("pending tool call {index} has no name"))?;
            let completed = result.get("status").and_then(Value::as_str) == Some("completed");
            let structured_result = result.get("result").cloned();
            let content = if completed {
                structured_result.clone().unwrap_or(Value::Null).to_string()
            } else {
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP tool call failed")
                    .to_string()
            };
            Ok(chatos_ai_runtime::SaveToolRecordInput {
                conversation_id: input.session_id.clone(),
                conversation_turn_id: Some(input.turn_id.clone()),
                message_id: Some(format!(
                    "cloud-agent:{}:{}:{}:tool:{index}",
                    run.ordering.agent_run_id, run.ordering.generation, run.ordering.step_seq
                )),
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
                content,
                success: completed,
                is_error: !completed,
                is_stream: false,
                structured_result,
                metadata: None,
                message_mode: Some(
                    crate::services::ai_common::TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE.to_string(),
                ),
                message_source: Some(input.model_name.clone()),
                summary_status: None,
                summary_id: None,
                summarized_at: None,
                created_at: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    ChatosMemoryRecordWriterAdapter::new(MessageManager::new())
        .save_tool_records(records)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_background_tasks_keep_source_message_processing() {
        let mut lifecycle = CloudTaskTurnLifecycleState::default();
        assert_eq!(
            task_runner_async_success_status_for_lifecycle(&lifecycle),
            None
        );

        lifecycle.project_planning_task_created = true;
        assert_eq!(
            task_runner_async_success_status_for_lifecycle(&lifecycle),
            Some("processing")
        );

        lifecycle.project_planning_task_created = false;
        lifecycle.project_execution_plan_materialized = true;
        assert_eq!(
            task_runner_async_success_status_for_lifecycle(&lifecycle),
            Some("processing")
        );
    }
}

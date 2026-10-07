// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ask_user_model_tools, notepad_model_tools, requirement_survey_model_tools, task_model_tools,
    task_process_model_tools, task_process_prompt_item, LocalAskUserToolExecutor,
    LocalNotepadToolExecutor, LocalRequirementSurveyToolExecutor, LocalTaskProcessToolExecutor,
    ASK_USER_TOOL_NAMES, NOTEPAD_READ_ONLY_TOOLS, NOTEPAD_TOOL_NAMES,
    REQUIREMENT_SURVEY_CREATE_TOOL, REQUIREMENT_SURVEY_TOOL_NAMES, TASK_PROCESS_TOOL_NAMES,
};
use crate::{
    ChatosAiRuntimeStepExecutor, ControlPlaneLocalAiStepPlanner, DurableAiProfile,
    LocalAgentHostCoordinator, LocalAgentScheduler, LocalCapabilityResolver, LocalMemorySyncWorker,
    LocalModelRuntimeResolver, LocalTaskToolExecutor, LocalToolExecutor, LocalToolRegistry,
    LocalToolScheduler, NamedReadOnlyTools, MAIN_CHAT_PROFILE_KEY, TASK_APPROVAL_EXEMPT_TOOLS,
    TASK_EXECUTION_PROFILE_KEY, TASK_READ_ONLY_TOOLS, TASK_TOOL_NAMES,
};
use chatos_local_agent_runtime::{LocalAgentProfileRegistry, LocalAgentRuntime};
use std::sync::Arc;

/// Fully assembled durable execution services for one native client process.
/// Platform code owns storage creation and concrete control-plane/tool adapters;
/// the shared Host owns Profile registration and scheduler wiring.
pub struct LocalAgentHostAssembly {
    runtime: Arc<LocalAgentRuntime>,
    coordinator: Arc<LocalAgentHostCoordinator>,
}

impl LocalAgentHostAssembly {
    pub fn new<M, C, I, S>(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
        model_resolver: M,
        capability_resolver: C,
        tools: LocalToolRegistry,
        read_only_tools: I,
    ) -> Result<Self, String>
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::build(
            runtime,
            owner_user_id.into(),
            model_resolver,
            capability_resolver,
            Some(tools),
            read_only_tools,
            Vec::new(),
            None,
            None,
        )
    }

    /// Builds a Host whose platform tools are claimed and committed by the
    /// native client through IPC. Rust retains the reserved Task tools.
    pub fn with_external_tool_worker<M, C, I, S>(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
        model_resolver: M,
        capability_resolver: C,
        read_only_tools: I,
        approval_exempt_tools: Vec<String>,
    ) -> Result<Self, String>
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::build(
            runtime,
            owner_user_id.into(),
            model_resolver,
            capability_resolver,
            None,
            read_only_tools,
            approval_exempt_tools,
            None,
            None,
        )
    }

    /// Builds the external-tool Host with retained Memory context and record
    /// persistence enabled for both production Profiles.
    pub fn with_external_tool_worker_and_memory<M, C, I, S>(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
        model_resolver: M,
        capability_resolver: C,
        read_only_tools: I,
        approval_exempt_tools: Vec<String>,
        memory_source_id: impl Into<String>,
        memory_sync_worker: LocalMemorySyncWorker,
    ) -> Result<Self, String>
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::build(
            runtime,
            owner_user_id.into(),
            model_resolver,
            capability_resolver,
            None,
            read_only_tools,
            approval_exempt_tools,
            Some(memory_source_id.into()),
            Some(memory_sync_worker),
        )
    }

    fn build<M, C, I, S>(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: String,
        model_resolver: M,
        capability_resolver: C,
        tools: Option<LocalToolRegistry>,
        read_only_tools: I,
        approval_exempt_tools: Vec<String>,
        memory_source_id: Option<String>,
        memory_sync_worker: Option<LocalMemorySyncWorker>,
    ) -> Result<Self, String>
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let model_resolver: Arc<dyn LocalModelRuntimeResolver> = Arc::new(model_resolver);
        let capability_resolver: Arc<dyn LocalCapabilityResolver> = Arc::new(capability_resolver);
        let safety = NamedReadOnlyTools::new(
            read_only_tools
                .into_iter()
                .map(Into::into)
                .chain(TASK_READ_ONLY_TOOLS.map(str::to_string))
                .chain(NOTEPAD_READ_ONLY_TOOLS.map(str::to_string)),
        )
        .with_approval_exempt(
            approval_exempt_tools
                .into_iter()
                .chain(TASK_APPROVAL_EXEMPT_TOOLS.map(str::to_string))
                .chain(ASK_USER_TOOL_NAMES.map(str::to_string))
                .chain(TASK_PROCESS_TOOL_NAMES.map(str::to_string))
                .chain([REQUIREMENT_SURVEY_CREATE_TOOL.to_string()]),
        );
        let main_tools = task_model_tools();
        let mut main_chat_planner = ControlPlaneLocalAiStepPlanner::main_chat(
            Arc::clone(&model_resolver),
            Arc::clone(&capability_resolver),
        )
        .with_local_tools(main_tools)?;
        let mut task_tools = notepad_model_tools();
        task_tools.extend(ask_user_model_tools());
        task_tools.extend(requirement_survey_model_tools());
        task_tools.extend(task_process_model_tools());
        let mut task_execution_planner = ControlPlaneLocalAiStepPlanner::task_execution(
            Arc::clone(&model_resolver),
            Arc::clone(&capability_resolver),
        )
        .with_local_tools(task_tools)?
        .with_local_tool_prefixes([
            "ask_user_",
            "notepad_",
            "requirement_survey_",
            "task_run_process_",
        ])?
        .with_local_prefixed_input_items(vec![task_process_prompt_item()]);
        if let Some(source_id) = memory_source_id {
            main_chat_planner = main_chat_planner.with_memory_source_id(source_id.clone())?;
            task_execution_planner = task_execution_planner.with_memory_source_id(source_id)?;
        }
        let mut profiles = LocalAgentProfileRegistry::new();
        profiles.register(
            MAIN_CHAT_PROFILE_KEY,
            DurableAiProfile::new(
                ChatosAiRuntimeStepExecutor::new(main_chat_planner),
                safety.clone(),
            ),
        )?;
        profiles.register(
            TASK_EXECUTION_PROFILE_KEY,
            DurableAiProfile::new(
                ChatosAiRuntimeStepExecutor::new(task_execution_planner),
                safety,
            ),
        )?;
        let model_scheduler = LocalAgentScheduler::new(
            Arc::clone(&runtime),
            profiles,
            owner_user_id.clone(),
            "local-model-worker",
        )?;
        let external_tool_worker = tools.is_none();
        let mut tools = tools.unwrap_or_default();
        let task_tools: Arc<dyn LocalToolExecutor> = Arc::new(LocalTaskToolExecutor::new(
            Arc::clone(&runtime),
            owner_user_id.clone(),
        )?);
        for tool_name in TASK_TOOL_NAMES {
            tools.register_shared(tool_name, Arc::clone(&task_tools))?;
        }
        LocalNotepadToolExecutor::new(Arc::clone(&runtime), owner_user_id.clone())?
            .register_into(&mut tools)?;
        LocalAskUserToolExecutor::new(Arc::clone(&runtime), owner_user_id.clone())?
            .register_into(&mut tools)?;
        LocalRequirementSurveyToolExecutor::new(Arc::clone(&runtime), owner_user_id.clone())?
            .register_into(&mut tools)?;
        let task_process_tools: Arc<dyn LocalToolExecutor> = Arc::new(
            LocalTaskProcessToolExecutor::new(Arc::clone(&runtime), owner_user_id.clone())?,
        );
        for tool_name in TASK_PROCESS_TOOL_NAMES {
            tools.register_shared(tool_name, Arc::clone(&task_process_tools))?;
        }
        let mut tool_scheduler = LocalToolScheduler::new(
            Arc::clone(&runtime),
            tools,
            owner_user_id.clone(),
            "local-tool-worker",
        )?;
        if external_tool_worker {
            let internal_tools = TASK_TOOL_NAMES
                .into_iter()
                .chain(ASK_USER_TOOL_NAMES)
                .chain(NOTEPAD_TOOL_NAMES)
                .chain(REQUIREMENT_SURVEY_TOOL_NAMES)
                .chain(TASK_PROCESS_TOOL_NAMES)
                .map(str::to_string)
                .collect();
            tool_scheduler = tool_scheduler.with_tool_filter(Some(internal_tools), Vec::new())?;
        }
        let mut coordinator = LocalAgentHostCoordinator::new(
            Arc::clone(&runtime),
            owner_user_id.clone(),
            Some(model_scheduler),
            Some(tool_scheduler),
        )?
        .with_reserved_ipc_tools(
            TASK_TOOL_NAMES
                .into_iter()
                .chain(ASK_USER_TOOL_NAMES)
                .chain(NOTEPAD_TOOL_NAMES)
                .chain(REQUIREMENT_SURVEY_TOOL_NAMES)
                .chain(TASK_PROCESS_TOOL_NAMES),
        )?;
        if let Some(worker) = memory_sync_worker {
            coordinator = coordinator.with_memory_sync_worker(worker)?;
        }
        let coordinator = Arc::new(coordinator);
        Ok(Self {
            runtime,
            coordinator,
        })
    }

    pub fn runtime(&self) -> Arc<LocalAgentRuntime> {
        Arc::clone(&self.runtime)
    }

    pub fn coordinator(&self) -> Arc<LocalAgentHostCoordinator> {
        Arc::clone(&self.coordinator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalToolExecutor;
    use async_trait::async_trait;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{LocalAgentToolInvocationRecord, LocalAgentToolOutcome};

    struct ModelResolver;

    #[async_trait]
    impl LocalModelRuntimeResolver for ModelResolver {
        async fn resolve_model_runtime(
            &self,
            _owner_user_id: &str,
            _model_config_ref: &str,
            _model_config_revision: &str,
        ) -> Result<crate::TransientLocalModelRuntime, String> {
            Err("not used while assembling".to_string())
        }
    }

    struct Capabilities;

    #[async_trait]
    impl LocalCapabilityResolver for Capabilities {
        async fn resolve_capabilities(
            &self,
            _owner_user_id: &str,
            _profile_key: &str,
            _capability_policy_revision: &str,
        ) -> Result<crate::ResolvedLocalCapabilities, String> {
            Ok(crate::ResolvedLocalCapabilities::default())
        }
    }

    struct ReadFile;

    #[async_trait]
    impl LocalToolExecutor for ReadFile {
        async fn execute_tool(
            &self,
            _invocation: &LocalAgentToolInvocationRecord,
        ) -> Result<LocalAgentToolOutcome, String> {
            Ok(LocalAgentToolOutcome::Succeeded {
                output: serde_json::json!({"content": "ok"}),
            })
        }
    }

    #[tokio::test]
    async fn assembles_both_profiles_and_tool_scheduler() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        let mut tools = LocalToolRegistry::new();
        tools.register("read_file", ReadFile).expect("tool");
        let assembly = LocalAgentHostAssembly::new(
            Arc::clone(&runtime),
            "user-1",
            ModelResolver,
            Capabilities,
            tools,
            ["read_file"],
        )
        .expect("assembly");

        assert!(Arc::ptr_eq(&assembly.runtime(), &runtime));
        assembly.coordinator().wake();
    }

    #[tokio::test]
    async fn external_tool_worker_does_not_require_a_rust_tool_registry() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        let assembly = LocalAgentHostAssembly::with_external_tool_worker(
            Arc::clone(&runtime),
            "user-1",
            ModelResolver,
            Capabilities,
            ["read_file"],
            Vec::new(),
        )
        .expect("assembly");

        assert!(Arc::ptr_eq(&assembly.runtime(), &runtime));
        assembly.coordinator().wake();
    }
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskRunnerCapabilityPolicy;

mod initialization;
mod preparation;

impl RunService {
    pub(in crate::services) async fn initialize_model_phase(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        effective_workspace_dir: &str,
        prerequisite_context: &[PrerequisiteTaskContext],
        authoritative_policy: bool,
    ) -> bool {
        initialization::initialize_model_phase(
            self,
            task,
            run,
            effective_workspace_dir,
            prerequisite_context,
            authoritative_policy,
        )
        .await
    }

    pub(in crate::services) async fn prepare_model_execution(
        &self,
        task: &TaskRecord,
        model_config: &ModelConfigRecord,
        run: &mut TaskRunRecord,
        input: &StartTaskRunRequest,
        effective_workspace_dir: &str,
        prerequisite_context: &[PrerequisiteTaskContext],
        capability_policy: Option<&TaskRunnerCapabilityPolicy>,
        mcp_runtime_session_ref: Option<&str>,
        plugin_session_snapshots: Option<
            Vec<crate::services::plugin_runtime_relay::PreparedPluginSessionSnapshot>,
        >,
    ) -> Result<PreparedModelExecution, String> {
        preparation::prepare_model_execution(
            self,
            task,
            model_config,
            run,
            input,
            effective_workspace_dir,
            prerequisite_context,
            capability_policy,
            mcp_runtime_session_ref,
            plugin_session_snapshots,
        )
        .await
    }
}

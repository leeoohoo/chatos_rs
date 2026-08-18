// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

type UnknownRecord = Record<string, unknown>;

export interface MessageTaskRunnerWorkspaceExecution {
  execution_group_id?: string | null;
  execution_branch_ref?: string | null;
  integration_status?: string | null;
  result_commit?: string | null;
  integrated_commit?: string | null;
  promoted_commit?: string | null;
  waived_at?: string | null;
  waiver_reason?: string | null;
  conflict_files?: string[];
  conflict_message?: string | null;
  integration_last_error?: string | null;
  integration_attempt_count?: number;
}

export interface MessageTaskRunnerRunChangedFile {
  status: string;
  path: string;
  old_path?: string | null;
}

export interface MessageTaskRunnerRunChanges {
  project_id: string;
  run_id: string;
  branch_ref: string;
  base_commit: string;
  result_commit: string;
  files: MessageTaskRunnerRunChangedFile[];
  patch: string;
  patch_truncated: boolean;
}

export interface MessageTaskRunnerTaskSummary {
  id: string;
  title?: string | null;
  status?: string | null;
  default_model_config_id?: string | null;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  last_run_id?: string | null;
  updated_at?: string | null;
  [key: string]: unknown;
}

export interface MessageTaskRunnerModelConfigSummary {
  id: string;
  name?: string | null;
  provider?: string | null;
  model?: string | null;
  usage_scenario?: string | null;
  enabled?: boolean;
  updated_at?: string | null;
  [key: string]: unknown;
}

export interface MessageTaskRunnerRunSummary {
  id: string;
  task_id?: string | null;
  model_config_id?: string | null;
  status?: string | null;
  model_phase_status?: string | null;
  workspace_execution?: MessageTaskRunnerWorkspaceExecution | null;
  started_at?: string | null;
  finished_at?: string | null;
  result_summary?: string | null;
  report?: unknown;
  error_message?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  [key: string]: unknown;
}

export interface MessageTaskRunnerTask {
  id: string;
  title: string;
  description?: string | null;
  objective?: string | null;
  status?: string | null;
  priority?: number | null;
  tags?: string[];
  default_model_config_id?: string | null;
  default_model_config?: MessageTaskRunnerModelConfigSummary | null;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  result_summary?: string | null;
  process_log?: string | null;
  last_run_id?: string | null;
  last_run?: MessageTaskRunnerRunSummary | null;
  schedule?: unknown;
  parent_task_id?: string | null;
  parent_task?: MessageTaskRunnerTaskSummary | null;
  source_run_id?: string | null;
  source_run?: MessageTaskRunnerRunSummary | null;
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
  prerequisite_task_ids?: string[];
  prerequisite_tasks?: MessageTaskRunnerTaskSummary[];
  project_task_id?: string | null;
  execution_client_ref?: string | null;
  dependency_context_refs?: string[];
  task_tool_state?: UnknownRecord | null;
  mcp_config?: UnknownRecord | null;
  input_payload?: unknown;
  created_at?: string | null;
  updated_at?: string | null;
  [key: string]: unknown;
}

export interface MessageTaskRunnerTasksResponse {
  items: MessageTaskRunnerTask[];
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
}

export interface MessageTaskRunnerGraphNode {
  task: MessageTaskRunnerTask;
  depth: number;
  is_root: boolean;
  is_current_message: boolean;
}

export interface MessageTaskRunnerGraphEdge {
  id: string;
  source: string;
  target: string;
  kind?: string | null;
}

export interface MessageTaskRunnerGraphResponse {
  root_task_ids: string[];
  nodes: MessageTaskRunnerGraphNode[];
  edges: MessageTaskRunnerGraphEdge[];
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
}

export interface MessageTaskRunnerRun {
  id: string;
  task_id: string;
  model_config_id?: string | null;
  memory_thread_id?: string | null;
  status?: string | null;
  model_phase_status?: string | null;
  workspace_execution?: MessageTaskRunnerWorkspaceExecution | null;
  started_at?: string | null;
  finished_at?: string | null;
  input_snapshot?: unknown;
  context_snapshot?: unknown;
  result_summary?: string | null;
  error_message?: string | null;
  usage?: unknown;
  report?: unknown;
  cancel_requested?: boolean;
  summary_job_run_id?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  [key: string]: unknown;
}

export interface MessageTaskRunnerRunEvent {
  id: string;
  run_id: string;
  event_type: string;
  message?: string | null;
  payload?: unknown;
  created_at?: string | null;
  [key: string]: unknown;
}

export interface MessageTaskRunnerRunDetailResponse {
  task: MessageTaskRunnerTask;
  run: MessageTaskRunnerRun;
  model_config?: MessageTaskRunnerModelConfigSummary | null;
  process_tasks?: MessageTaskRunnerTask[];
  events: MessageTaskRunnerRunEvent[];
  events_total?: number;
  events_limit?: number;
  events_offset?: number;
  events_has_more?: boolean;
}

export interface MessageTaskRunnerRetryRunResponse {
  success: boolean;
  run: MessageTaskRunnerRun;
}

export interface PluginUiWorkbenchHostContext {
  run_id: string;
  plugin_id: string;
  release_id: string;
  component_key: string;
  title: string;
  surface: string;
}

export interface PluginUiWorkbenchSessionResponse {
  session_id: string;
  expires_in: number;
  expires_at: string;
  iframe_path: string;
  bridge_protocol_version: number;
  adapter_session_id: string;
  host_session_nonce: string;
  bridge_capabilities: string[];
  host_context: PluginUiWorkbenchHostContext;
}

export interface PluginArtifactOwner {
  owner_user_id: string;
  run_id: string;
  device_id: string;
  workspace_id: string;
  plugin_id: string;
  release_id: string;
  artifact_sha256: string;
  component_key: string;
  adapter_session_id: string;
}

export interface PluginArtifactDescriptor {
  artifact_id: string;
  owner: PluginArtifactOwner;
  workspace_relative_path: string;
  display_name: string;
  media_type: string;
  size_bytes: number;
  sha256: string;
  created_at: string;
  producer_tool_name: string;
  downloadable: boolean;
  mutable: boolean;
}

export interface PluginArtifactUiAccess {
  run_id: string;
  plugin_id: string;
  release_id: string;
  artifact_sha256: string;
  component_key: string;
  adapter_session_id: string;
  ui_snapshot_sha256: string;
}

export interface PluginArtifactListResponse {
  access: PluginArtifactUiAccess;
  artifacts: PluginArtifactDescriptor[];
}

export interface PluginArtifactReadResponse {
  access: PluginArtifactUiAccess;
  artifact: PluginArtifactDescriptor;
  body_base64: string;
}

export interface PluginArtifactCreateRequest {
  display_name: string;
  media_type: string;
  body_base64: string;
}

export interface PluginArtifactUpdateRequest {
  expected_sha256: string;
  body_base64: string;
}

export type PluginArtifactWriteOperation = 'create' | 'update';

export interface PluginArtifactWriteResponse {
  access: PluginArtifactUiAccess;
  operation: PluginArtifactWriteOperation;
  artifact: PluginArtifactDescriptor;
}

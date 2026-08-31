// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type TaskStatus =
  | 'draft'
  | 'ready'
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'blocked'
  | 'cancelled'
  | 'archived';

export type TaskMcpInitMode = 'full' | 'disabled';
export type TaskBuiltinPromptMode = 'configured' | 'effective';
export type TaskScheduleMode = 'manual' | 'once' | 'interval' | 'contact_async';
export type TaskProcessLogOperation = 'append' | 'replace' | 'clear';
export type TaskProfile = 'default' | 'chatos_plan';
export type TaskProjectStatus = 'active' | 'archived';

export interface TaskMcpConfig {
  enabled: boolean;
  init_mode: TaskMcpInitMode;
  builtin_prompt_mode: TaskBuiltinPromptMode;
  builtin_prompt_locale: string;
  enabled_builtin_kinds: string[];
  requires_execution: boolean;
  workspace_changes_required: boolean;
  execution_service_id?: string | null;
  workspace_dir?: string | null;
  external_mcp_config_ids: string[];
  selected_skill_ids: string[];
  skill_policy_revision?: string | null;
}

export interface SelectedTaskPlugin {
  plugin_id: string;
  selected_skill_ids: string[];
  selected_command_ids: string[];
}

export interface TaskPluginCommandInvocation {
  plugin_id: string;
  command_id: string;
  arguments?: string | null;
}

export interface TaskPluginConfig {
  selected_plugins: SelectedTaskPlugin[];
  command_invocations: TaskPluginCommandInvocation[];
}

export interface TaskSelectedPluginSnapshot {
  plugin_id: string;
  plugin_key: string;
  display_name?: string;
  release_id: string;
  version: string;
  artifact_sha256: string;
  device_id: string;
  reason?: string | null;
}

export interface TaskPluginSelectionAudit {
  selection_source: string;
  policy_revision: string;
  selected_at: string;
  project_context_revision: string;
  plugins: TaskSelectedPluginSnapshot[];
}

export interface SelectableTaskPluginCommand {
  command_id: string;
  display_name: string;
  description?: string | null;
  argument_hint?: string | null;
  requires_confirmation: boolean;
  target_agent?: string | null;
  allowed_tools?: string[];
}

export interface SelectableTaskPlugin {
  id: string;
  plugin_key: string;
  display_name: string;
  description: string;
  version: string;
  release_id: string;
  artifact_sha256: string;
  device_id?: string | null;
  requires_device: boolean;
  component_keys: string[];
  commands: SelectableTaskPluginCommand[];
}

export interface TaskPluginConnectorDevice {
  id: string;
  display_name: string;
  client_version?: string | null;
  os?: string | null;
  status: string;
  last_seen_at?: string | null;
}

export interface TaskPluginConnectorWorkspace {
  id: string;
  device_id: string;
  display_name: string;
  local_path_alias: string;
  capabilities: string[];
  status: string;
}

export interface TaskPluginConnectorsResponse {
  devices: TaskPluginConnectorDevice[];
  workspaces: TaskPluginConnectorWorkspace[];
}

export interface TaskCapabilityCatalogResponse {
  agent_key:
    | 'task_runner_plan_phase'
    | 'task_runner_run_phase';
  policy_revision: string;
  selectable_plugins: SelectableTaskPlugin[];
}

export interface TaskMcpRequiredBuiltinCapability {
  kind: string;
  source: string;
}

export interface TaskMcpHostedBuiltinRoute {
  host: string;
  server_name: string;
  builtin_kinds: string[];
  public_server_names: string[];
}

export interface TaskMcpResolutionResponse {
  requested_builtin_kinds: string[];
  required_builtin_kinds: TaskMcpRequiredBuiltinCapability[];
  hosted_builtin_routes: TaskMcpHostedBuiltinRoute[];
  server_local_builtin_kinds: string[];
  external_mcp_config_ids: string[];
}

export interface TaskScheduleConfig {
  mode: TaskScheduleMode;
  run_at?: string | null;
  interval_seconds?: number | null;
  next_run_at?: string | null;
  last_scheduled_at?: string | null;
}

export interface TaskToolOutcomeItem {
  kind: string;
  text: string;
  importance?: string | null;
  refs: string[];
}

export interface TaskToolState {
  due_at?: string | null;
  outcome_items: TaskToolOutcomeItem[];
  resume_hint?: string | null;
  blocker_reason?: string | null;
  blocker_needs: string[];
  blocker_kind?: string | null;
  completed_at?: string | null;
  last_outcome_at?: string | null;
}

export interface TaskRecord {
  id: string;
  title: string;
  description?: string | null;
  objective: string;
  input_payload?: unknown;
  status: TaskStatus;
  priority: number;
  tags: string[];
  default_model_config_id?: string | null;
  memory_thread_id: string;
  tenant_id: string;
  subject_id: string;
  project_id: string;
  task_profile: TaskProfile;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  result_summary?: string | null;
  process_log?: string | null;
  last_run_id?: string | null;
  schedule: TaskScheduleConfig;
  parent_task_id?: string | null;
  source_run_id?: string | null;
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
  prerequisite_task_ids: string[];
  task_tool_state: TaskToolState;
  plugin_config: TaskPluginConfig;
  plugin_selection_audit?: TaskPluginSelectionAudit | null;
  mcp_config: TaskMcpConfig;
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
}

export interface TaskSummaryRecord {
  id: string;
  title: string;
  status: TaskStatus;
  default_model_config_id?: string | null;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  project_id: string;
  last_run_id?: string | null;
  updated_at: string;
}

export interface TaskIndexResponse {
  tasks: TaskSummaryRecord[];
  tags: string[];
}

export interface CreateTaskPayload {
  title: string;
  description?: string;
  objective: string;
  input_payload?: unknown;
  status?: TaskStatus;
  priority?: number;
  tags?: string[];
  default_model_config_id?: string;
  project_id?: string;
  task_profile?: TaskProfile;
  schedule?: TaskScheduleConfig;
  plugin_config?: TaskPluginConfig;
  mcp_config?: Pick<TaskMcpConfig, 'requires_execution'>;
  prerequisite_task_ids?: string[];
}

export interface UpdateTaskPayload extends Partial<CreateTaskPayload> {}

export interface RecordTaskProcessPayload {
  operation?: TaskProcessLogOperation;
  content?: string;
  heading?: string;
}

export interface TaskListFilters {
  status?: TaskStatus;
  keyword?: string;
  tag?: string;
  model_config_id?: string;
  project_id?: string;
  scheduled_only?: boolean;
  parent_task_id?: string;
  include_subtasks?: boolean;
  source_run_id?: string;
  task_profile?: TaskProfile;
  limit?: number;
  offset?: number;
}

export interface TaskProjectRecord {
  id: string;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  name: string;
  root_path?: string | null;
  git_url?: string | null;
  description?: string | null;
  status: TaskProjectStatus;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
}

export interface TaskStatsResponse {
  total: number;
  scheduled: number;
  follow_up: number;
  draft: number;
  ready: number;
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  blocked: number;
  cancelled: number;
  archived: number;
}

export interface BatchTaskStatusUpdatePayload {
  task_ids: string[];
  status: TaskStatus;
}

export interface BatchTaskDeletePayload {
  task_ids: string[];
}

export interface BatchTaskRunPayload {
  task_ids: string[];
  model_config_id?: string;
  prompt_override?: string;
}

export interface BatchTaskOperationItem {
  task_id: string;
  ok: boolean;
  message?: string | null;
  run_id?: string | null;
}

export interface BatchTaskOperationResponse {
  total: number;
  succeeded: number;
  failed: number;
  results: BatchTaskOperationItem[];
}

export interface TaskMemoryContextPayload {
  include_recent_records?: boolean;
  include_thread_summary?: boolean;
  include_subject_memory?: boolean;
  recent_record_limit?: number;
  summary_limit?: number;
}

export interface TaskMemoryRecordsPayload {
  role?: string;
  record_type?: string;
  summary_status?: string;
  limit?: number;
  offset?: number;
  order?: 'asc' | 'desc';
}

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

export type TaskRunStatus =
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'blocked';

export type UiPromptStatus =
  | 'pending'
  | 'submitted'
  | 'cancelled'
  | 'timed_out'
  | 'failed';

export type UserRole = 'admin' | 'agent';

export type TaskMcpInitMode = 'full' | 'builtin_only' | 'disabled';
export type TaskBuiltinPromptMode = 'configured' | 'effective';
export type TaskScheduleMode = 'manual' | 'once' | 'interval' | 'contact_async';
export type TaskProcessLogOperation = 'append' | 'replace' | 'clear';

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
}

export interface LoginPayload {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

export interface CurrentUserResponse {
  user: AuthUser;
}

export interface UserSummaryRecord {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string | null;
  principal_type?: 'human_user' | 'agent_account' | string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  agent_count?: number | null;
}

export interface CreateUserPayload {
  username: string;
  display_name?: string;
  password: string;
  role?: UserRole;
  enabled?: boolean;
}

export interface UpdateUserPayload {
  display_name?: string;
  password?: string;
  role?: UserRole;
  enabled?: boolean;
}

export interface TaskMcpConfig {
  enabled: boolean;
  init_mode: TaskMcpInitMode;
  builtin_prompt_mode: TaskBuiltinPromptMode;
  builtin_prompt_locale: string;
  enabled_builtin_kinds: string[];
  workspace_dir?: string | null;
  default_remote_server_id?: string | null;
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
  mcp_config: TaskMcpConfig;
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
}

export interface ModelConfigRecord {
  id: string;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  usage_scenario?: string | null;
  temperature?: number | null;
  max_output_tokens?: number | null;
  thinking_level?: string | null;
  supports_responses: boolean;
  instructions?: string | null;
  request_cwd?: string | null;
  include_prompt_cache_retention: boolean;
  request_body_limit_bytes?: number | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export type RemoteServerAuthType = 'password' | 'private_key' | 'private_key_cert';
export type RemoteServerHostKeyPolicy = 'accept_new' | 'strict';
export type RemoteServerTestStatus = 'success' | 'failed';

export interface RemoteServerRecord {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: RemoteServerAuthType | string;
  password?: string | null;
  private_key_path?: string | null;
  certificate_path?: string | null;
  default_remote_path?: string | null;
  host_key_policy: RemoteServerHostKeyPolicy | string;
  enabled: boolean;
  last_tested_at?: string | null;
  last_test_status?: RemoteServerTestStatus | string | null;
  last_test_message?: string | null;
  last_active_at?: string | null;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  task_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateRemoteServerPayload {
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_type: RemoteServerAuthType | string;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: RemoteServerHostKeyPolicy | string;
  enabled?: boolean;
}

export interface UpdateRemoteServerPayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: RemoteServerAuthType | string;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: RemoteServerHostKeyPolicy | string;
  enabled?: boolean;
}

export interface TestRemoteServerPayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: RemoteServerAuthType | string;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: RemoteServerHostKeyPolicy | string;
}

export interface RemoteServerTestResponse {
  ok: boolean;
  server_id?: string | null;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: RemoteServerAuthType | string;
  remote_host?: string | null;
  error?: string | null;
  tested_at: string;
}

export type ExternalMcpTransport = 'stdio' | 'http';

export interface ExternalMcpConfigRecord {
  id: string;
  name: string;
  transport: ExternalMcpTransport | string;
  command?: string | null;
  args: string[];
  url?: string | null;
  headers: Record<string, string>;
  env: Record<string, string>;
  cwd?: string | null;
  enabled: boolean;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateExternalMcpConfigPayload {
  name: string;
  transport: ExternalMcpTransport | string;
  command?: string;
  args?: string[];
  url?: string;
  headers?: Record<string, string>;
  env?: Record<string, string>;
  cwd?: string;
  enabled?: boolean;
}

export interface UpdateExternalMcpConfigPayload {
  name?: string;
  transport?: ExternalMcpTransport | string;
  command?: string;
  args?: string[];
  url?: string;
  headers?: Record<string, string>;
  env?: Record<string, string>;
  cwd?: string;
  enabled?: boolean;
}

export interface TaskRunRecord {
  id: string;
  task_id: string;
  model_config_id: string;
  memory_thread_id: string;
  status: TaskRunStatus;
  started_at?: string | null;
  finished_at?: string | null;
  input_snapshot: unknown;
  context_snapshot?: unknown;
  result_summary?: string | null;
  error_message?: string | null;
  usage?: unknown;
  report?: unknown;
  cancel_requested: boolean;
  summary_job_run_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface TaskRunEventRecord {
  id: string;
  run_id: string;
  event_type: string;
  message?: string | null;
  payload?: unknown;
  created_at: string;
}

export interface EngineThread {
  id: string;
  tenant_id: string;
  source_id: string;
  subject_id: string;
  thread_type: string;
  external_thread_id?: string | null;
  title?: string | null;
  labels?: string[] | null;
  metadata?: unknown;
  status: string;
  summary_status: string;
  summary_job_run_id?: string | null;
  summary_locked_at?: string | null;
  summary_lock_expires_at?: string | null;
  pending_record_count: number;
  pending_summary_tokens: number;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
}

export interface EngineRecord {
  id: string;
  thread_id: string;
  tenant_id: string;
  source_id: string;
  external_record_id?: string | null;
  role: string;
  record_type: string;
  content: string;
  structured_payload?: unknown;
  metadata?: unknown;
  summary_status: string;
  summary_id?: string | null;
  summarized_at?: string | null;
  created_at: string;
}

export interface ComposeContextBlock {
  block_type: string;
  text: string;
}

export interface ComposeContextMeta {
  summary_count: number;
  recent_record_count: number;
}

export interface ComposeContextResponse {
  thread_id: string;
  blocks: ComposeContextBlock[];
  recent_records: EngineRecord[];
  meta: ComposeContextMeta;
}

export interface TaskMemoryContextResponse {
  task_id: string;
  memory_thread_id: string;
  tenant_id: string;
  subject_id: string;
  thread?: EngineThread | null;
  context?: ComposeContextResponse | null;
  total_record_count: number;
}

export interface TaskMemoryRecordsResponse {
  task_id: string;
  memory_thread_id: string;
  tenant_id: string;
  subject_id: string;
  thread?: EngineThread | null;
  total: number;
  limit: number;
  offset: number;
  order: string;
  role?: string | null;
  record_type?: string | null;
  summary_status?: string | null;
  has_more: boolean;
  items: EngineRecord[];
}

export interface TaskMemorySummaryJobResult {
  thread_id: string;
  accepted: boolean;
  running: boolean;
  job_run_id?: string | null;
  generated: boolean;
  summary_id?: string | null;
  source_record_count: number;
}

export interface TaskMemorySummaryResponse {
  task_id: string;
  memory_thread_id: string;
  tenant_id: string;
  requested_at: string;
  result: TaskMemorySummaryJobResult;
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

export interface RunSummaryRecord {
  id: string;
  task_id: string;
  status: TaskRunStatus;
  model_config_id: string;
  updated_at: string;
}

export interface ModelConfigUsageRecord {
  model_config_id: string;
  task_count: number;
  run_count: number;
}

export interface UiPromptTaskCountRecord {
  task_id: string;
  count: number;
}

export interface UiPromptResponseSubmission {
  status: string;
  values?: unknown;
  selection?: unknown;
  reason?: string | null;
}

export interface UiPromptRecord {
  id: string;
  task_id?: string | null;
  run_id?: string | null;
  conversation_id: string;
  conversation_turn_id: string;
  tool_call_id?: string | null;
  kind: string;
  title: string;
  message: string;
  allow_cancel: boolean;
  timeout_ms: number;
  payload: unknown;
  response?: UiPromptResponseSubmission | null;
  status: UiPromptStatus;
  created_at: string;
  updated_at: string;
  expires_at?: string | null;
}

export interface McpUnavailableTool {
  name: string;
  reason: string;
}

export interface McpCatalogEntry {
  kind: string;
  server_name: string;
  config_id?: string | null;
  command?: string | null;
  description: string;
  use_cases: string[];
  capabilities: string[];
  implemented: boolean;
  runtime_default: boolean;
  default_allow_writes: boolean;
  available_tool_names: string[];
  unavailable_tools: McpUnavailableTool[];
  message?: string | null;
}

export interface McpServerInfo {
  server_name: string;
  transports: string[];
  http_endpoint_path?: string | null;
  stdio_command?: string | null;
  stdio_args: string[];
  tool_names: string[];
  tool_profiles?: McpServerToolProfileInfo[];
}

export interface McpServerToolProfileInfo {
  key: string;
  label: string;
  description: string;
  tool_names: string[];
}

export interface TaskRunnerSkillResponse {
  name: string;
  locale: string;
  content: string;
}

export interface TaskRunnerInternalPromptPreviewResponse {
  locale: string;
  task_prompt_template: string;
  process_log_system_prompt: string;
  notes: string[];
}

export interface McpPromptPreviewPayload {
  enabled?: boolean;
  init_mode?: TaskMcpInitMode;
  builtin_prompt_mode?: TaskBuiltinPromptMode;
  builtin_prompt_locale?: string;
  enabled_builtin_kinds?: string[];
  workspace_dir?: string;
  default_remote_server_id?: string;
}

export interface McpPromptBuildResult {
  prompt?: string | null;
  selected_section_ids: string[];
  omitted_section_ids: string[];
  requested_builtin_server_names: string[];
  active_builtin_server_names: string[];
  omitted_builtin_server_names: string[];
  runtime_limitations?: string | null;
}

export interface McpPromptPreviewResponse {
  enabled: boolean;
  init_mode: TaskMcpInitMode;
  builtin_prompt_mode: TaskBuiltinPromptMode;
  builtin_prompt_locale: string;
  selected_builtin_kinds: string[];
  build: McpPromptBuildResult;
}

export interface ToolingNoteSummary {
  id: string;
  title: string;
  folder: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  file: string;
}

export interface ToolingNotepadFoldersResponse {
  ok: boolean;
  folders: string[];
}

export interface ToolingNotepadNotesResponse {
  ok: boolean;
  notes: ToolingNoteSummary[];
}

export interface ToolingNotepadNoteResponse {
  ok: boolean;
  note: ToolingNoteSummary;
  content: string;
}

export interface ToolingTagCount {
  tag: string;
  count: number;
}

export interface ToolingNotepadTagsResponse {
  ok: boolean;
  tags: ToolingTagCount[];
}

export interface ToolingTerminalLogEntry {
  offset: number;
  kind: string;
  content: string;
  created_at: string;
}

export interface ToolingTerminalProcessRecord {
  terminal_id: string;
  process_id: string;
  terminal_name: string;
  status: string;
  process_status: string;
  busy: boolean;
  has_session: boolean;
  command: string;
  pid?: number | null;
  started_at: string;
  uptime_seconds?: number | null;
  cwd: string;
  project_id?: string | null;
  last_active_at: string;
  output_preview: string;
  output_tail: string;
  output_tail_chars: number;
  exit_code?: number | null;
}

export interface ToolingTerminalProcessListResponse {
  status: string;
  result_scope: string;
  is_multiple_terminals: boolean;
  terminal_count: number;
  process_count: number;
  visible_total: number;
  total_terminals: number;
  include_exited: boolean;
  limit: number;
  terminals: ToolingTerminalProcessRecord[];
  processes: ToolingTerminalProcessRecord[];
}

export interface ToolingTerminalProcessLogsResponse {
  terminal_id: string;
  process_id: string;
  terminal_name: string;
  status: string;
  process_status: string;
  busy: boolean;
  has_session: boolean;
  command: string;
  pid?: number | null;
  started_at: string;
  uptime_seconds?: number | null;
  cwd: string;
  project_id?: string | null;
  last_active_at: string;
  mode: string;
  requested_offset?: number | null;
  next_offset?: number | null;
  limit: number;
  fetched_log_count: number;
  returned_log_count: number;
  has_more: boolean;
  truncated: boolean;
  logs: ToolingTerminalLogEntry[];
  output_preview: string;
  output_tail: string;
  output_tail_chars: number;
  exit_code?: number | null;
}

export interface ToolingTerminalKillResponse {
  ok: boolean;
  terminal_id: string;
  killed: boolean;
}

export interface ToolingTerminalWriteResponse {
  ok: boolean;
  terminal_id: string;
  bytes_written: number;
  submit: boolean;
}

export interface HealthResponse {
  status: string;
  service: string;
  now: string;
}

export interface SystemConfigResponse {
  host: string;
  port: number;
  store_mode: string;
  database_url: string;
  memory_engine_base_url?: string | null;
  memory_engine_source_id: string;
  memory_engine_configured: boolean;
  default_tenant_id: string;
  default_subject_id: string;
  default_workspace_dir: string;
  memory_timeout_ms: number;
  default_execution_timeout_ms: number;
  execution_timeout_ms: number;
  scheduler_poll_interval_ms: number;
  auto_memory_summary: boolean;
  default_task_execution_max_iterations: number;
  task_execution_max_iterations: number;
  default_tool_result_model_max_chars: number;
  tool_result_model_max_chars: number;
  default_tool_results_model_total_max_chars: number;
  tool_results_model_total_max_chars: number;
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
  schedule?: TaskScheduleConfig;
  mcp_config?: TaskMcpConfig;
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
  limit?: number;
  offset?: number;
}

export type TaskProjectStatus = 'active' | 'archived';

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

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  has_more: boolean;
}

export interface CreateModelConfigPayload {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  usage_scenario?: string;
  temperature?: number;
  max_output_tokens?: number;
  thinking_level?: string;
  supports_responses?: boolean;
  instructions?: string;
  request_cwd?: string;
  include_prompt_cache_retention?: boolean;
  request_body_limit_bytes?: number;
  enabled?: boolean;
}

export interface UpdateModelConfigPayload extends Partial<CreateModelConfigPayload> {}

export interface RuntimeSettingsRecord {
  id: string;
  task_execution_max_iterations: number;
  execution_timeout_ms?: number | null;
  tool_result_model_max_chars: number;
  tool_results_model_total_max_chars: number;
  created_at: string;
  updated_at: string;
}

export interface UpdateRuntimeSettingsPayload {
  task_execution_max_iterations?: number;
  execution_timeout_ms?: number;
  tool_result_model_max_chars?: number;
  tool_results_model_total_max_chars?: number;
}

export interface PreviewModelCatalogPayload {
  provider: string;
  base_url?: string;
  api_key?: string;
  model?: string;
  supports_responses?: boolean;
}

export interface ProviderModelRecord {
  id: string;
  owned_by?: string | null;
  context_length?: number | null;
  supports_images: boolean;
  supports_video: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  raw?: unknown;
}

export interface ModelCatalogResponse {
  provider_config_id?: string | null;
  provider: string;
  base_url: string;
  source: string;
  fetched_at?: string | null;
  models: ProviderModelRecord[];
  error?: string | null;
}

export interface TestModelConfigPayload {
  prompt?: string;
}

export interface ModelConfigTestResponse {
  ok: boolean;
  model_config_id: string;
  provider: string;
  model: string;
  content?: string | null;
  reasoning?: string | null;
  usage?: unknown;
  response_id?: string | null;
  error?: string | null;
  tested_at: string;
}

export interface StartTaskRunPayload {
  model_config_id?: string;
  prompt_override?: string;
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

export interface RunListFilters {
  task_id?: string;
  status?: TaskRunStatus;
  model_config_id?: string;
  keyword?: string;
  limit?: number;
  offset?: number;
}

export interface PromptListFilters {
  taskId?: string;
  runId?: string;
  status?: UiPromptStatus;
  limit?: number;
  offset?: number;
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

export interface SubmitUiPromptPayload {
  values?: unknown;
  selection?: unknown;
  reason?: string;
}

export interface CancelUiPromptPayload {
  reason?: string;
}

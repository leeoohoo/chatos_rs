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
  workspace_dir?: string | null;
  default_remote_server_id?: string | null;
  external_mcp_config_ids: string[];
  skill_ids: string[];
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

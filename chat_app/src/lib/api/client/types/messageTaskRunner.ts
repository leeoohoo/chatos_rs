type UnknownRecord = Record<string, unknown>;

export interface MessageTaskRunnerTask {
  id: string;
  title: string;
  description?: string | null;
  objective?: string | null;
  status?: string | null;
  priority?: number | null;
  tags?: string[];
  default_model_config_id?: string | null;
  creator_user_id?: string | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  result_summary?: string | null;
  process_log?: string | null;
  last_run_id?: string | null;
  schedule?: unknown;
  parent_task_id?: string | null;
  source_run_id?: string | null;
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
  prerequisite_task_ids?: string[];
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

export interface MessageTaskRunnerRun {
  id: string;
  task_id: string;
  model_config_id?: string | null;
  memory_thread_id?: string | null;
  status?: string | null;
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
  events: MessageTaskRunnerRunEvent[];
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

type UnknownRecord = Record<string, unknown>;

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
  started_at?: string | null;
  finished_at?: string | null;
  result_summary?: string | null;
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
  events: MessageTaskRunnerRunEvent[];
  events_total?: number;
  events_limit?: number;
  events_offset?: number;
  events_has_more?: boolean;
}

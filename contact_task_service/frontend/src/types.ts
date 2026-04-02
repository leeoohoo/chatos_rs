export interface AuthUser {
  username: string;
  role: string;
}

export interface TaskContextAssetRef {
  asset_type: string;
  asset_id: string;
  display_name?: string | null;
  source_type?: string | null;
  source_path?: string | null;
}

export interface TaskExecutionResultContract {
  result_required: boolean;
  preferred_format?: string | null;
}

export interface TaskPlanningSnapshot {
  contact_authorized_builtin_mcp_ids: string[];
  selected_model_config_id?: string | null;
  planned_at?: string | null;
}

export interface ContactTask {
  id: string;
  user_id: string;
  contact_agent_id: string;
  project_id: string;
  project_root?: string | null;
  remote_connection_id?: string | null;
  session_id?: string | null;
  source_message_id?: string | null;
  model_config_id?: string | null;
  title: string;
  content: string;
  priority: string;
  status: string;
  confirm_note?: string | null;
  execution_note?: string | null;
  planned_builtin_mcp_ids: string[];
  planned_context_assets: TaskContextAssetRef[];
  execution_result_contract?: TaskExecutionResultContract | null;
  planning_snapshot?: TaskPlanningSnapshot | null;
  created_by?: string | null;
  created_at: string;
  updated_at: string;
  confirmed_at?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  last_error?: string | null;
  result_summary?: string | null;
  result_message_id?: string | null;
}

export interface TaskExecutionMessage {
  id: string;
  task_id?: string | null;
  source_session_id?: string | null;
  role: string;
  content: string;
  message_mode?: string | null;
  message_source?: string | null;
  tool_call_id?: string | null;
  reasoning?: string | null;
  metadata?: Record<string, unknown> | null;
  created_at: string;
}

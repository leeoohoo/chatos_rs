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
  source_user_goal_summary?: string | null;
  source_constraints_summary?: string | null;
  planned_at?: string | null;
}

export interface TaskHandoffPayload {
  task_id: string;
  task_plan_id?: string | null;
  handoff_kind: string;
  summary: string;
  result_summary?: string | null;
  key_changes: string[];
  changed_files: string[];
  executed_commands: string[];
  verification_suggestions: string[];
  open_risks: string[];
  artifact_refs: string[];
  checkpoint_message_ids: string[];
  result_brief_id?: string | null;
  generated_at: string;
}

export interface ContactTask {
  id: string;
  user_id: string;
  contact_agent_id: string;
  project_id: string;
  scope_key?: string;
  task_plan_id?: string | null;
  task_ref?: string | null;
  task_kind?: string | null;
  depends_on_task_ids: string[];
  verification_of_task_ids: string[];
  acceptance_criteria: string[];
  blocked_reason?: string | null;
  project_root?: string | null;
  remote_connection_id?: string | null;
  session_id?: string | null;
  conversation_turn_id?: string | null;
  source_message_id?: string | null;
  model_config_id?: string | null;
  title: string;
  content: string;
  priority: string;
  priority_rank?: number;
  queue_position?: number;
  status: string;
  confirm_note?: string | null;
  execution_note?: string | null;
  planned_builtin_mcp_ids: string[];
  planned_context_assets: TaskContextAssetRef[];
  execution_result_contract?: TaskExecutionResultContract | null;
  planning_snapshot?: TaskPlanningSnapshot | null;
  handoff_payload?: TaskHandoffPayload | null;
  created_by?: string | null;
  created_at: string;
  updated_at: string;
  confirmed_at?: string | null;
  started_at?: string | null;
  paused_at?: string | null;
  pause_reason?: string | null;
  last_checkpoint_summary?: string | null;
  last_checkpoint_message_id?: string | null;
  resume_note?: string | null;
  finished_at?: string | null;
  last_error?: string | null;
  result_summary?: string | null;
  result_message_id?: string | null;
}

export interface TaskPlanView {
  plan_id: string;
  user_id: string;
  contact_agent_id: string;
  project_id: string;
  title: string;
  task_count: number;
  blocked_task_count: number;
  latest_updated_at: string;
  active_task_id?: string | null;
  status_counts: Record<string, number>;
  tasks: ContactTask[];
}

export interface TaskPlanOperationResult {
  kind: string;
  task_id: string;
  affected_task_ids: string[];
  affected_count: number;
  replacement_task_id?: string | null;
}

export interface UpdateTaskPlanResponse {
  item: TaskPlanView;
  operation_results: TaskPlanOperationResult[];
}

export interface TaskExecutionMessage {
  id: string;
  task_id?: string | null;
  source_session_id?: string | null;
  role: string;
  content: string;
  message_mode?: string | null;
  message_source?: string | null;
  tool_calls?: unknown;
  tool_call_id?: string | null;
  reasoning?: string | null;
  metadata?: unknown;
  summary_status?: string | null;
  summary_id?: string | null;
  summarized_at?: string | null;
  created_at: string;
}

export interface TaskResultBrief {
  id: string;
  task_id: string;
  user_id: string;
  contact_agent_id: string;
  project_id: string;
  source_session_id?: string | null;
  source_turn_id?: string | null;
  task_title: string;
  task_status: string;
  result_summary: string;
  result_format?: string | null;
  result_message_id?: string | null;
  finished_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface MemoryContactSummary {
  id: string;
  user_id: string;
  agent_id: string;
  agent_name_snapshot?: string | null;
}

export interface MemoryProjectSummary {
  id: string;
  user_id: string;
  project_id: string;
  name: string;
}

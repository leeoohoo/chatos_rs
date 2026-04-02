export interface AuthUser {
  username: string;
  role: string;
}

export interface ContactTask {
  id: string;
  user_id: string;
  contact_agent_id: string;
  project_id: string;
  session_id?: string | null;
  source_message_id?: string | null;
  model_config_id?: string | null;
  title: string;
  content: string;
  priority: string;
  status: string;
  confirm_note?: string | null;
  execution_note?: string | null;
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

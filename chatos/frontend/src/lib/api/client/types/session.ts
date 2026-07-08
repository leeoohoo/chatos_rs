// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface SessionUpsertPayload {
  id: string;
  title: string;
  user_id: string;
  project_id?: string;
  metadata?: Record<string, unknown> | string | null;
}

export interface SessionUpdatePayload {
  title?: string;
  description?: string;
  metadata?: Record<string, unknown> | string | null;
}

export interface SessionRuntimeSettingsResponse {
  session_id: string;
  user_id: string;
  selected_model_id?: string | null;
  selected_model_name?: string | null;
  selected_thinking_level?: string | null;
  remote_connection_id?: string | null;
  workspace_root?: string | null;
  reasoning_enabled: boolean;
  plan_mode_enabled: boolean;
  mcp_enabled: boolean;
  enabled_mcp_ids: string[];
  auto_create_task: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface SessionRuntimeSettingsPayload {
  selected_model_id?: string | null;
  selected_model_name?: string | null;
  selected_thinking_level?: string | null;
  remote_connection_id?: string | null;
  workspace_root?: string | null;
  reasoning_enabled?: boolean;
  plan_mode_enabled?: boolean;
  mcp_enabled?: boolean;
  enabled_mcp_ids?: string[];
  auto_create_task?: boolean;
}

export interface SessionResponse {
  id: string;
  title: string;
  user_id?: string | null;
  userId?: string | null;
  project_id?: string | null;
  projectId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  message_count?: number;
  messageCount?: number;
  token_usage?: number;
  tokenUsage?: number;
  tags?: string | null;
  pinned?: boolean;
  archived?: boolean;
  status?: string;
  metadata?: Record<string, unknown> | string | null;
  selected_model_id?: string | null;
  selected_agent_id?: string | null;
  description?: string | null;
}

export interface ContactResponse {
  id: string;
  user_id: string;
  agent_id: string;
  agent_name_snapshot?: string | null;
  task_runner_enabled?: boolean | null;
  task_runner_base_url?: string | null;
  task_runner_agent_account_id?: string | null;
  task_runner_username?: string | null;
  task_runner_has_password?: boolean | null;
  status?: string | null;
  created_at?: string;
  updated_at?: string;
}

export type ContactCreateResponse = ContactResponse | { contact: ContactResponse };

export interface ContactTaskRunnerUpdatePayload {
  enabled: boolean;
  base_url?: string | null;
  task_runner_agent_account_id?: string | null;
  username?: string | null;
  password?: string | null;
  clear_password?: boolean;
}

export interface ContactProjectMemoryResponse {
  id: string;
  user_id?: string;
  contact_id?: string;
  agent_id?: string;
  project_id?: string;
  memory_text?: string;
  memory_version?: number;
  last_source_at?: string | null;
  updated_at?: string;
}

export interface ContactProjectLinkResponse {
  project_id?: string;
  project_name?: string;
  project_root?: string | null;
  status?: string;
  is_virtual?: number;
  has_memory?: boolean;
  memory_version?: number;
  recall_summarized?: number;
  last_source_at?: string | null;
  updated_at?: string;
}

export interface ContactAgentRecallResponse {
  id: string;
  user_id?: string;
  agent_id?: string;
  recall_key?: string;
  recall_text?: string;
  level?: number;
  confidence?: number | null;
  last_seen_at?: string | null;
  updated_at?: string;
}

export interface SessionMessageResponse {
  id: string;
  conversationId?: string;
  conversation_id?: string;
  role: string;
  content: string;
  message_mode?: string | null;
  message_source?: string | null;
  rawContent?: string;
  summary?: string;
  tokensUsed?: number;
  status?: string;
  metadata?: Record<string, unknown> | null;
  toolCalls?: unknown[];
  tool_calls?: unknown[];
  summary_status?: string | null;
  summary_id?: string | null;
  summarized_at?: string | null;
  createdAt?: string | Date;
  created_at?: string;
  updatedAt?: string | Date;
  updated_at?: string;
}

export interface CompactHistoryResponse {
  items: SessionMessageResponse[];
  has_more?: boolean;
  next_before?: string | null;
}

export interface UserMessageTurnResponse {
  turn_id: string;
  user_message: SessionMessageResponse;
  final_assistant_message?: SessionMessageResponse | null;
  has_process?: boolean;
  tool_call_count?: number;
  thinking_count?: number;
  process_message_count?: number;
}

export interface UserMessageTurnsResponse {
  items: UserMessageTurnResponse[];
  has_more?: boolean;
  next_before?: string | null;
}

export interface ConversationTaskRunnerActiveMessageTasksItem {
  source_user_message_id?: string | null;
  source_turn_id?: string | null;
  running_count?: number;
  active_count?: number;
}

export interface ConversationTaskRunnerActiveMessageTasksResponse {
  active_source_user_message_ids?: string[];
  running_source_user_message_ids?: string[];
  items?: ConversationTaskRunnerActiveMessageTasksItem[];
}

export interface ConversationMessageEnvelope {
  data: {
    message: SessionMessageResponse;
  };
}

export interface ConversationMessagesEnvelope {
  data: {
    messages: SessionMessageResponse[];
  };
}

export interface MessageCreatePayload {
  id: string;
  conversationId: string;
  role: string;
  content: string;
  metadata?: Record<string, unknown> | null;
  toolCalls?: unknown[];
  createdAt?: Date;
  status?: string;
}

export interface ConversationMessagePayload {
  id?: string;
  role: string;
  content: string;
  tool_calls?: unknown[] | null;
  tool_call_id?: string | null;
  reasoning?: unknown;
  metadata?: Record<string, unknown> | null;
}

export interface SessionSummaryResponse {
  id: string;
  conversation_id?: string;
  summary_text?: string;
  summary_model?: string;
  status?: string;
  level?: number;
  created_at?: string;
  updated_at?: string;
}

export interface SessionSummariesListResponse {
  items: SessionSummaryResponse[];
  total: number;
  has_summary: boolean;
}

export interface ReviewRepairSummaryResult {
  processed_sessions: number;
  summarized_sessions: number;
  generated_summaries: number;
  marked_messages: number;
  failed_sessions: number;
  pending_message_count: number;
  project_id: string;
  contact_id?: string | null;
  agent_id?: string | null;
  mode: string;
}

export interface ReviewRepairStatusResult {
  running: boolean;
  running_job_count: number;
  pending_message_count: number;
  scope_session_count: number;
  project_id: string;
  contact_id?: string | null;
  agent_id?: string | null;
  job_type: string;
}

export interface ReviewRepairResponse {
  success?: boolean;
  conversation_id?: string;
  conversationId?: string;
  project_id?: string;
  contact_id?: string | null;
  agent_id?: string | null;
  result?: ReviewRepairSummaryResult;
  error?: string;
  detail?: string;
}

export interface ReviewRepairStatusResponse {
  success?: boolean;
  conversation_id?: string;
  conversationId?: string;
  project_id?: string;
  contact_id?: string | null;
  agent_id?: string | null;
  result?: ReviewRepairStatusResult;
  error?: string;
  detail?: string;
}

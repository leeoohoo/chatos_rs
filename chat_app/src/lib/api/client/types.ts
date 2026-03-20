export interface PagingOptions {
  limit?: number;
  offset?: number;
}

export interface SessionPagingOptions extends PagingOptions {
  includeArchived?: boolean;
  includeArchiving?: boolean;
}

export interface MemoryAgentsQueryOptions extends PagingOptions {
  enabled?: boolean;
}

export interface SessionUpsertPayload {
  id: string;
  title: string;
  user_id: string;
  project_id?: string;
  metadata?: any;
}

export interface SessionUpdatePayload {
  title?: string;
  description?: string;
  metadata?: any;
}

export interface RemoteConnectionDraftPayload {
  name?: string;
  host: string;
  port?: number;
  username: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_password?: string;
  user_id?: string;
}

export interface RemoteConnectionUpdatePayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_password?: string;
}

export interface SftpTransferStartPayload {
  direction: 'upload' | 'download';
  local_path: string;
  remote_path: string;
}

export interface FsMoveOptions {
  targetName?: string;
  replaceExisting?: boolean;
}

export interface MessageCreatePayload {
  id: string;
  sessionId: string;
  role: string;
  content: string;
  metadata?: any;
  toolCalls?: any[];
  createdAt?: Date;
  status?: string;
}

export interface McpConfigCreatePayload {
  id: string;
  name: string;
  command: string;
  type: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled: boolean;
  user_id?: string;
}

export interface McpConfigUpdatePayload {
  id?: string;
  name?: string;
  command?: string;
  type?: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled?: boolean;
  userId?: string;
}

export interface AiModelConfigCreatePayload {
  id: string;
  name: string;
  provider: string;
  model: string;
  thinking_level?: string;
  api_key: string;
  base_url: string;
  user_id?: string;
  enabled: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface SystemContextCreatePayload {
  name: string;
  content: string;
  user_id: string;
  app_ids?: string[];
}

export interface SystemContextUpdatePayload {
  name: string;
  content: string;
  app_ids?: string[];
}

export interface SystemContextDraftGeneratePayload {
  user_id: string;
  scene: string;
  style?: string;
  language?: string;
  output_format?: string;
  constraints?: string[];
  forbidden?: string[];
  candidate_count?: number;
  ai_model_config?: any;
}

export interface SystemContextDraftOptimizePayload {
  user_id: string;
  content: string;
  goal?: string;
  keep_intent?: boolean;
  ai_model_config?: any;
}

export interface SystemContextDraftEvaluatePayload {
  content: string;
}

export interface ApplicationCreatePayload {
  name: string;
  url: string;
  icon_url?: string | null;
  user_id?: string;
}

export interface ApplicationUpdatePayload {
  name?: string;
  url?: string;
  icon_url?: string | null;
}

export interface StreamChatOptions {
  turnId?: string;
  contactAgentId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
}

export interface TaskManagerUpdatePayload {
  title?: string;
  details?: string;
  priority?: 'high' | 'medium' | 'low';
  status?: 'todo' | 'doing' | 'blocked' | 'done';
  tags?: string[];
  due_at?: string | null;
}

export interface TaskReviewDecisionPayload {
  action: 'confirm' | 'cancel';
  tasks?: any[];
  reason?: string;
}

export interface UiPromptResponsePayload {
  status: 'ok' | 'canceled' | 'timeout';
  values?: Record<string, string>;
  selection?: string | string[];
  reason?: string;
}

export interface NotepadListOptions {
  folder?: string;
  recursive?: boolean;
  tags?: string[];
  match?: 'all' | 'any';
  query?: string;
  limit?: number;
}

export interface NotepadCreatePayload {
  folder?: string;
  title?: string;
  content?: string;
  tags?: string[];
}

export interface NotepadUpdatePayload {
  title?: string;
  content?: string;
  folder?: string;
  tags?: string[];
}

export interface NotepadSearchOptions {
  query: string;
  folder?: string;
  recursive?: boolean;
  tags?: string[];
  match?: 'all' | 'any';
  include_content?: boolean;
  limit?: number;
}

export interface SessionSummaryJobConfigPayload {
  user_id?: string;
  enabled?: boolean;
  summary_model_config_id?: string | null;
  token_limit?: number;
  message_count_limit?: number;
  round_limit?: number;
  target_summary_tokens?: number;
  job_interval_seconds?: number;
}

export interface RegisterPayload {
  username: string;
  password: string;
}

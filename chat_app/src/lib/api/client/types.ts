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

export interface DeleteSuccessResponse {
  success?: boolean;
  deleted?: boolean;
  message?: string;
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
  status?: string | null;
  created_at?: string;
  updated_at?: string;
}

export type ContactCreateResponse = ContactResponse | { contact: ContactResponse };

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
  sessionId?: string;
  session_id?: string;
  role: string;
  content: string;
  rawContent?: string;
  summary?: string;
  tokensUsed?: number;
  status?: string;
  metadata?: Record<string, unknown> | null;
  toolCalls?: unknown[];
  tool_calls?: unknown[];
  createdAt?: string | Date;
  created_at?: string;
  updatedAt?: string | Date;
  updated_at?: string;
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

export interface ProjectResponse {
  id: string;
  name: string;
  root_path?: string;
  rootPath?: string;
  description?: string | null;
  user_id?: string | null;
  userId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface ProjectRunTargetResponse {
  id: string;
  label?: string;
  kind?: string;
  cwd?: string;
  command?: string;
  source?: string;
  confidence?: number;
  is_default?: boolean;
  isDefault?: boolean;
}

export interface ProjectRunCatalogResponse {
  project_id?: string;
  projectId?: string;
  status?: string;
  default_target_id?: string | null;
  defaultTargetId?: string | null;
  targets?: ProjectRunTargetResponse[];
  error_message?: string | null;
  errorMessage?: string | null;
  analyzed_at?: string | null;
  analyzedAt?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectRunExecuteResponse {
  success?: boolean;
  status?: string;
  run_id?: string;
  runId?: string;
  terminal_id?: string;
  terminalId?: string;
  target_id?: string;
  targetId?: string;
  command?: string;
  cwd?: string;
  message?: string;
  error?: string;
}

export interface ProjectContactLinkResponse {
  contact_id?: string;
  contactId?: string;
  agent_id?: string;
  agentId?: string;
  agent_name_snapshot?: string | null;
  agentNameSnapshot?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}

export interface ProjectChangeLogResponse {
  id: string;
  server_name?: string;
  serverName?: string;
  path?: string;
  action?: string;
  change_kind?: 'create' | 'edit' | 'delete' | string;
  changeKind?: 'create' | 'edit' | 'delete' | string;
  bytes?: number;
  sha256?: string | null;
  diff?: string | null;
  session_id?: string | null;
  sessionId?: string | null;
  run_id?: string | null;
  runId?: string | null;
  confirmed?: boolean;
  confirmed_at?: string | null;
  confirmedAt?: string | null;
  confirmed_by?: string | null;
  confirmedBy?: string | null;
  created_at?: string;
  createdAt?: string;
  session_title?: string | null;
  sessionTitle?: string | null;
}

export interface ProjectChangeMarkResponse {
  path?: string;
  relative_path?: string;
  relativePath?: string;
  kind?: 'create' | 'edit' | 'delete' | string;
  last_change_id?: string;
  lastChangeId?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface ProjectChangeSummaryResponse {
  file_marks?: ProjectChangeMarkResponse[];
  fileMarks?: ProjectChangeMarkResponse[];
  deleted_marks?: ProjectChangeMarkResponse[];
  deletedMarks?: ProjectChangeMarkResponse[];
  counts?: {
    create?: number;
    edit?: number;
    delete?: number;
    total?: number;
  };
}

export interface TerminalResponse {
  id: string;
  name?: string;
  cwd?: string;
  user_id?: string | null;
  userId?: string | null;
  project_id?: string | null;
  projectId?: string | null;
  status?: string;
  busy?: boolean;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  last_active_at?: string;
  lastActiveAt?: string;
}

export interface TerminalLogResponse {
  id: string;
  terminal_id?: string;
  terminalId?: string;
  log_type?: string;
  logType?: string;
  type?: string;
  content?: string;
  created_at?: string;
  createdAt?: string;
}

export interface TerminalDispatchResponse {
  success?: boolean;
  terminal_id?: string;
  terminalId?: string;
  terminal_name?: string;
  terminalName?: string;
  terminal_reused?: boolean;
  terminalReused?: boolean;
  status?: string;
  interrupted?: boolean;
  signal?: string;
  reason?: string;
  cwd?: string;
  executed_command?: string;
  executedCommand?: string;
  project_id?: string;
  projectId?: string;
  message?: string;
}

export interface RemoteConnectionResponse {
  id: string;
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  authType?: 'private_key' | 'private_key_cert' | 'password';
  password?: string | null;
  private_key_path?: string | null;
  privateKeyPath?: string | null;
  certificate_path?: string | null;
  certificatePath?: string | null;
  default_remote_path?: string | null;
  defaultRemotePath?: string | null;
  host_key_policy?: 'strict' | 'accept_new';
  hostKeyPolicy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jumpEnabled?: boolean;
  jump_host?: string | null;
  jumpHost?: string | null;
  jump_port?: number | null;
  jumpPort?: number | null;
  jump_username?: string | null;
  jumpUsername?: string | null;
  jump_private_key_path?: string | null;
  jumpPrivateKeyPath?: string | null;
  jump_password?: string | null;
  jumpPassword?: string | null;
  user_id?: string | null;
  userId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  last_active_at?: string;
  lastActiveAt?: string;
}

export interface RemoteConnectionTestResponse {
  success?: boolean;
  status?: string;
  message?: string;
  error?: string;
}

export interface RemoteSftpEntryResponse {
  name?: string;
  path?: string;
  is_dir?: boolean;
  isDir?: boolean;
  size?: number | null;
  modified_at?: string | null;
  modifiedAt?: string | null;
}

export interface RemoteSftpEntriesResponse {
  path?: string | null;
  parent?: string | null;
  entries?: RemoteSftpEntryResponse[];
}

export interface RemoteSftpTransferStatusResponse {
  id?: string;
  direction?: 'upload' | 'download';
  state?: 'pending' | 'running' | 'cancelling' | 'success' | 'error' | 'cancelled' | string;
  total_bytes?: number | null;
  totalBytes?: number | null;
  transferred_bytes?: number;
  transferredBytes?: number;
  percent?: number | null;
  current_path?: string | null;
  currentPath?: string | null;
  message?: string | null;
  error?: string | null;
}

export interface FsEntryResponse {
  name?: string;
  path?: string;
  is_dir?: boolean;
  isDir?: boolean;
  size?: number | null;
  modified_at?: string | null;
  modifiedAt?: string | null;
}

export interface FsEntriesResponse {
  path?: string | null;
  parent?: string | null;
  entries?: FsEntryResponse[];
  roots?: FsEntryResponse[];
  truncated?: boolean;
}

export interface FsReadFileResponse {
  path?: string;
  name?: string;
  size?: number;
  content_type?: string;
  contentType?: string;
  is_binary?: boolean;
  isBinary?: boolean;
  modified_at?: string | null;
  modifiedAt?: string | null;
  content?: string;
}

export interface FsMutationResponse {
  success?: boolean;
  path?: string;
  name?: string;
  message?: string;
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

export interface ConversationMessagePayload {
  id?: string;
  role: string;
  content: string;
  tool_calls?: unknown[] | null;
  tool_call_id?: string | null;
  reasoning?: unknown;
  metadata?: Record<string, unknown> | null;
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

export interface McpConfigResponse {
  id: string;
  name: string;
  display_name?: string | null;
  displayName?: string | null;
  command: string;
  type: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled?: boolean;
  readonly?: boolean;
  builtin?: boolean;
  config?: Record<string, unknown> | null;
  user_id?: string | null;
  userId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
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

export interface AiModelConfigUpdatePayload {
  id?: string;
  name?: string;
  provider?: string;
  model?: string;
  model_name?: string;
  thinking_level?: string;
  api_key?: string;
  base_url?: string;
  user_id?: string;
  userId?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface AiModelConfigResponse {
  id: string;
  name: string;
  provider: string;
  model?: string;
  model_name?: string;
  thinking_level?: string;
  api_key?: string;
  base_url?: string;
  user_id?: string | null;
  userId?: string | null;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
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

export interface SystemContextModelConfigPayload {
  model_name?: string;
  model?: string;
  provider?: string;
  api_key?: string;
  base_url?: string;
  temperature?: number;
}

export interface SystemContextResponse {
  id: string;
  name: string;
  content: string;
  user_id?: string;
  userId?: string;
  is_active?: boolean;
  isActive?: boolean;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  app_ids?: string[];
}

export interface ActiveSystemContextResponse {
  content: string;
  context: SystemContextResponse | null;
}

export interface PromptQualityReportResponse {
  clarity?: number;
  constraint_completeness?: number;
  conflict_risk?: number;
  verbosity?: number;
  overall?: number;
  warnings?: string[];
}

export interface PromptCandidateResponse {
  title?: string;
  content: string;
  score?: number;
  report?: PromptQualityReportResponse;
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
  ai_model_config?: SystemContextModelConfigPayload;
}

export interface SystemContextDraftGenerateResponse {
  candidates?: PromptCandidateResponse[];
}

export interface SystemContextDraftOptimizePayload {
  user_id: string;
  content: string;
  goal?: string;
  keep_intent?: boolean;
  ai_model_config?: SystemContextModelConfigPayload;
}

export interface SystemContextDraftOptimizeResponse {
  optimized_content?: string;
  score_after?: number;
  report_after?: PromptQualityReportResponse;
}

export interface SystemContextDraftEvaluatePayload {
  content: string;
}

export interface SystemContextDraftEvaluateResponse {
  report?: PromptQualityReportResponse;
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

export interface ApplicationResponse {
  id: string;
  name: string;
  url: string;
  icon_url?: string | null;
  iconUrl?: string | null;
  description?: string | null;
  user_id?: string | null;
  enabled?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface MemoryAgentSkillResponse {
  id: string;
  name: string;
  content: string;
}

export interface MemoryAgentRuntimePluginSummaryResponse {
  source: string;
  name: string;
  category?: string | null;
  description?: string | null;
  content_summary?: string | null;
  updated_at?: string | null;
}

export interface MemoryAgentRuntimeCommandSummaryResponse {
  command_ref: string;
  name: string;
  description?: string | null;
  argument_hint?: string | null;
  plugin_source: string;
  source_path: string;
  content: string;
  updated_at?: string | null;
}

export interface MemoryAgentRuntimeSkillSummaryResponse {
  id: string;
  name: string;
  description?: string | null;
  plugin_source?: string | null;
  source_type: string;
  source_path?: string | null;
  updated_at?: string | null;
}

export interface MemoryAgentResponse {
  id: string;
  user_id?: string;
  name: string;
  description?: string | null;
  category?: string | null;
  role_definition: string;
  plugin_sources?: string[];
  skills?: MemoryAgentSkillResponse[];
  skill_ids?: string[];
  default_skill_ids?: string[];
  mcp_policy?: Record<string, unknown> | null;
  project_policy?: Record<string, unknown> | null;
  enabled?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface MemoryAgentRuntimeContextResponse {
  agent_id: string;
  name: string;
  description?: string | null;
  category?: string | null;
  role_definition: string;
  plugin_sources?: string[];
  runtime_plugins?: MemoryAgentRuntimePluginSummaryResponse[];
  skills?: MemoryAgentSkillResponse[];
  skill_ids?: string[];
  runtime_skills?: MemoryAgentRuntimeSkillSummaryResponse[];
  runtime_commands?: MemoryAgentRuntimeCommandSummaryResponse[];
  mcp_policy?: Record<string, unknown> | null;
  project_policy?: Record<string, unknown> | null;
  updated_at?: string;
}

export interface StreamChatOptions {
  turnId?: string;
  contactAgentId?: string | null;
  remoteConnectionId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
}

export interface StreamChatModelConfigPayload {
  provider: string;
  model_name: string;
  temperature?: number;
  thinking_level?: string | null;
  api_key?: string;
  base_url?: string;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface StreamChatAttachmentPayload {
  name: string;
  mimeType: string;
  size: number;
  type: 'image' | 'file';
  dataUrl?: string;
  text?: string;
}

export interface ConversationDetailsResponse {
  data: {
    conversation: {
      id: string;
      title: string;
      created_at: string;
      updated_at: string;
    };
  };
}

export interface ConversationAssistantResponse {
  data: {
    assistant: {
      id: string;
      name: string;
      model_config: {
        model_name?: string;
        temperature?: number;
        api_key?: string;
        base_url?: string;
      };
    };
  };
}

export interface ConversationMcpServersResponse {
  data: {
    mcp_servers: Array<{
      name: string;
      url: string;
    }>;
  };
}

export interface McpConfigResourceResponse {
  success: boolean;
  config: Record<string, unknown> | null;
  alias?: string;
}

export interface RuntimeGuidanceSubmitPayload {
  sessionId: string;
  turnId: string;
  content: string;
  projectId?: string | null;
}

export interface RuntimeGuidanceSubmitResponse {
  success: boolean;
  guidance_id?: string;
  status?: 'queued' | 'applied' | 'dropped';
  pending_count?: number;
  turn_id?: string;
  error?: string;
  code?: string;
}

export interface TurnRuntimeSnapshotSystemMessage {
  id: string;
  source: string;
  content: string;
}

export interface TurnRuntimeSnapshotTool {
  name: string;
  server_name: string;
  server_type: string;
  description?: string | null;
}

export interface TurnRuntimeSnapshotSelectedCommand {
  command_ref?: string | null;
  name?: string | null;
  plugin_source: string;
  source_path: string;
  trigger?: string | null;
  arguments?: string | null;
}

export interface TurnRuntimeSnapshotRuntime {
  model?: string | null;
  provider?: string | null;
  contact_agent_id?: string | null;
  remote_connection_id?: string | null;
  project_id?: string | null;
  project_root?: string | null;
  workspace_root?: string | null;
  mcp_enabled?: boolean | null;
  enabled_mcp_ids?: string[];
  selected_commands?: TurnRuntimeSnapshotSelectedCommand[];
}

export interface TurnRuntimeSnapshot {
  id: string;
  session_id: string;
  user_id: string;
  turn_id: string;
  user_message_id?: string | null;
  status: string;
  snapshot_source: string;
  snapshot_version: number;
  captured_at: string;
  updated_at: string;
  system_messages?: TurnRuntimeSnapshotSystemMessage[];
  tools?: TurnRuntimeSnapshotTool[];
  runtime?: TurnRuntimeSnapshotRuntime | null;
}

export interface TurnRuntimeSnapshotLookupResponse {
  session_id: string;
  turn_id?: string | null;
  status: string;
  snapshot_source: string;
  snapshot?: TurnRuntimeSnapshot | null;
}

export interface TaskManagerUpdatePayload {
  title?: string;
  details?: string;
  priority?: 'high' | 'medium' | 'low';
  status?: 'todo' | 'doing' | 'blocked' | 'done';
  tags?: string[];
  due_at?: string | null;
}

export interface TaskManagerTaskResponse {
  id: string;
  title?: string;
  details?: string | null;
  priority?: 'high' | 'medium' | 'low' | null;
  status?: 'todo' | 'doing' | 'blocked' | 'done' | null;
  tags?: string[];
  due_at?: string | null;
  created_at?: string;
  updated_at?: string;
  conversation_turn_id?: string | null;
}

export interface TaskReviewTaskDraft {
  id?: string;
  title?: string;
  details?: string;
  priority?: 'high' | 'medium' | 'low';
  status?: 'todo' | 'doing' | 'blocked' | 'done';
  tags?: string[];
  due_at?: string | null;
  [key: string]: unknown;
}

export interface TaskReviewDecisionPayload {
  action: 'confirm' | 'cancel';
  tasks?: TaskReviewTaskDraft[];
  reason?: string;
}

export interface UiPromptResponsePayload {
  status: 'ok' | 'canceled' | 'timeout';
  values?: Record<string, string>;
  selection?: string | string[];
  reason?: string;
}

export interface UiPromptItemResponse {
  id: string;
  session_id?: string;
  conversation_turn_id?: string | null;
  status?: string;
  title?: string | null;
  message?: string | null;
  schema?: Record<string, unknown> | null;
  values?: Record<string, unknown> | null;
  created_at?: string;
  updated_at?: string;
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

export interface NotepadFolderMutationResponse {
  ok?: boolean;
  folder?: string;
  from?: string;
  to?: string;
  moved_notes?: number;
  deleted_notes?: number;
}

export interface NotepadNoteResponse {
  id: string;
  title: string;
  folder: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  file: string;
}

export interface NotepadInitResponse {
  ok?: boolean;
  [key: string]: unknown;
}

export interface NotepadFoldersResponse {
  ok?: boolean;
  folders?: string[];
}

export interface NotepadNotesResponse {
  ok?: boolean;
  notes?: NotepadNoteResponse[];
}

export interface NotepadNoteDetailResponse {
  ok?: boolean;
  note?: NotepadNoteResponse | null;
  content?: string;
}

export interface NotepadDeleteNoteResponse {
  ok?: boolean;
  id?: string;
}

export interface NotepadTagResponse {
  tag: string;
  count: number;
}

export interface NotepadTagsResponse {
  ok?: boolean;
  tags?: NotepadTagResponse[];
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

export interface SessionSummaryJobConfigResponse extends SessionSummaryJobConfigPayload {
  id?: string;
  created_at?: string;
  updated_at?: string;
}

export interface SessionSummaryResponse {
  id: string;
  session_id?: string;
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

export interface RegisterPayload {
  username: string;
  password: string;
}

export interface AuthResponse {
  token?: string;
  access_token?: string;
  user?: {
    id?: string;
    username?: string;
    role?: string;
  } | null;
  username?: string;
  role?: string;
}

export interface MeResponse {
  user?: {
    id?: string;
    username?: string;
    role?: string;
  } | null;
  id?: string;
  username?: string;
  role?: string;
}

export interface UserSettingsResponse {
  user_id?: string;
  settings?: Record<string, unknown>;
  effective?: Record<string, unknown>;
}

export interface UserSettingsUpdatePayload {
  user_id: string;
  settings: Record<string, unknown>;
}

export interface StopChatResponse {
  success?: boolean;
  message?: string;
}

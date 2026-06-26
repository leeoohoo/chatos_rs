export type RealtimeConnectionState = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'error';

export type RealtimeTopicScope =
  | 'contacts'
  | 'notepad'
  | 'projects'
  | 'sessions'
  | 'remote_connections'
  | 'conversation'
  | 'project'
  | 'terminal'
  | 'remote_connection';

export interface RealtimeTopic {
  scope: RealtimeTopicScope;
  id?: string | null;
}

export interface RealtimeAckMessage {
  type: 'ack';
  acked: 'subscribe' | 'unsubscribe';
  topics: RealtimeTopic[];
}

export interface RealtimePongMessage {
  type: 'pong';
  ts: string;
}

export interface RealtimeErrorMessage {
  type: 'error';
  code: string;
  message: string;
}

export interface RealtimeDebugEventRecord {
  event: string;
  ts: string;
  conversation_id?: string | null;
  project_id?: string | null;
  payloadKind?: string | null;
  payloadReason?: string | null;
  payloadAction?: string | null;
  streamType?: string | null;
}

export interface RealtimeDebugSnapshot {
  connectionState: RealtimeConnectionState;
  activeTopics: RealtimeTopic[];
  lastAck?: RealtimeAckMessage | null;
  lastError?: RealtimeErrorMessage | null;
  lastPongTs?: string | null;
  lastControlMessageAt?: string | null;
  lastEventAt?: string | null;
  recentEvents: RealtimeDebugEventRecord[];
}

export interface ReviewRepairRealtimePayload {
  conversation_id: string;
  project_id: string;
  contact_id?: string | null;
  agent_id?: string | null;
  running: boolean;
  pending_message_count?: number | null;
  running_job_count?: number | null;
  scope_session_count?: number | null;
  processed_sessions?: number | null;
  summarized_sessions?: number | null;
  generated_summaries?: number | null;
  marked_messages?: number | null;
  failed_sessions?: number | null;
  job_type?: string | null;
  mode?: string | null;
  error?: string | null;
}

export interface RealtimeConversationSummariesUpdatedPayloadWrapper {
  kind: 'conversation_summaries_updated';
  conversation_id: string;
  project_id: string;
  contact_id?: string | null;
  agent_id?: string | null;
  items: Array<{
    id: string;
    conversation_id?: string | null;
    summary_text?: string;
    summary_model?: string;
    trigger_type?: string;
    source_message_count?: number;
    source_estimated_tokens?: number;
    status?: string;
    error_message?: string | null;
    level?: number;
    created_at?: string;
    updated_at?: string;
  }>;
  total: number;
  has_summary: boolean;
  reason: string;
}

export interface RealtimeReviewRepairPayloadWrapper {
  kind: 'review_repair';
  conversation_id: string;
  project_id: string;
  contact_id?: string | null;
  agent_id?: string | null;
  running: boolean;
  pending_message_count?: number | null;
  running_job_count?: number | null;
  scope_session_count?: number | null;
  processed_sessions?: number | null;
  summarized_sessions?: number | null;
  generated_summaries?: number | null;
  marked_messages?: number | null;
  failed_sessions?: number | null;
  job_type?: string | null;
  mode?: string | null;
  error?: string | null;
}

export interface RealtimeProjectChangeSummaryPayloadWrapper {
  kind: 'project_change_summary';
  project_id: string;
  reason: string;
  conversation_id?: string | null;
  path?: string | null;
}

export interface RealtimeContactsUpdatedPayloadWrapper {
  kind: 'contacts_updated';
  reason: string;
  contact_id?: string | null;
  contact?: {
    id: string;
    user_id?: string | null;
    agent_id?: string | null;
    agent_name_snapshot?: string | null;
    status?: string | null;
    created_at?: string;
    updated_at?: string;
  } | null;
}

export interface RealtimeNotepadUpdatedPayloadWrapper {
  kind: 'notepad_updated';
  reason: string;
  note_id?: string | null;
  folder?: string | null;
  from?: string | null;
  to?: string | null;
}

export interface RealtimeProjectsUpdatedPayloadWrapper {
  kind: 'projects_updated';
  reason: string;
  project_id?: string | null;
  project?: {
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
  } | null;
}

export interface RealtimeRemoteConnectionsUpdatedPayloadWrapper {
  kind: 'remote_connections_updated';
  reason: string;
  connection_id?: string | null;
  connection?: {
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
    jump_connection_id?: string | null;
    jumpConnectionId?: string | null;
    jump_host?: string | null;
    jumpHost?: string | null;
    jump_port?: number | null;
    jumpPort?: number | null;
    jump_username?: string | null;
    jumpUsername?: string | null;
    jump_private_key_path?: string | null;
    jumpPrivateKeyPath?: string | null;
    jump_certificate_path?: string | null;
    jumpCertificatePath?: string | null;
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
  } | null;
}

export interface RealtimeSessionsUpdatedPayloadWrapper {
  kind: 'sessions_updated';
  reason: string;
  session_id?: string | null;
  project_id?: string | null;
  session?: {
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
  } | null;
}

export interface RealtimeTerminalStatePayloadWrapper {
  kind: 'terminal_state';
  terminal_id: string;
  project_id?: string | null;
  terminal_name?: string | null;
  cwd?: string | null;
  status: string;
  busy: boolean;
  reason: string;
  exit_code?: number | null;
}

export interface RealtimeTerminalListInvalidatedPayloadWrapper {
  kind: 'terminal_list_invalidated';
  terminal_id?: string | null;
  project_id?: string | null;
  reason: string;
  terminal?: {
    id: string;
    name?: string | null;
    cwd?: string | null;
    user_id?: string | null;
    userId?: string | null;
    project_id?: string | null;
    projectId?: string | null;
    status?: string | null;
    busy?: boolean | null;
    created_at?: string;
    createdAt?: string;
    updated_at?: string;
    updatedAt?: string;
    last_active_at?: string;
    lastActiveAt?: string;
  } | null;
}

export interface RealtimeProjectRunStatePayloadWrapper {
  kind: 'project_run_state';
  project_id: string;
  terminal_id?: string | null;
  terminal_name?: string | null;
  cwd?: string | null;
  status: string;
  busy: boolean;
  running: boolean;
  reason: string;
  exit_code?: number | null;
}

export interface RealtimeProjectRunInstancePayloadWrapper {
  kind: 'project_run_instance';
  project_id: string;
  terminal_id: string;
  terminal_name: string;
  cwd: string;
  status: string;
  busy: boolean;
  running: boolean;
  reason: string;
  exit_code?: number | null;
}

export interface RealtimeProjectRunCatalogPayloadWrapper {
  kind: 'project_run_catalog';
  project_id: string;
  reason: string;
  path?: string | null;
}

export interface RealtimeProjectMembersUpdatedPayloadWrapper {
  kind: 'project_members_updated';
  project_id: string;
  reason: string;
  contact_id?: string | null;
}

export interface RealtimeTaskBoardPayloadWrapper {
  kind: 'task_board';
  conversation_id: string;
  conversation_turn_id?: string | null;
  review_id?: string | null;
  task_id?: string | null;
  action: string;
  task?: {
    id: string;
    conversation_id?: string;
    conversation_turn_id?: string | null;
    title?: string;
    details?: string;
    priority?: string | null;
    status?: string | null;
    tags?: string[];
    due_at?: string | null;
    outcome_summary?: string | null;
    outcome_items?: Array<{
      kind?: string;
      text?: string;
      importance?: string;
      refs?: string[];
    }>;
    resume_hint?: string | null;
    blocker_reason?: string | null;
    blocker_needs?: string[];
    blocker_kind?: string | null;
    completed_at?: string | null;
    last_outcome_at?: string | null;
    created_at?: string;
    updated_at?: string;
  } | null;
  draft_tasks?: Array<{
    id?: string;
    title?: string;
    details?: string;
    priority?: string;
    status?: string;
    tags?: string[];
    due_at?: string | null;
  }> | null;
  timeout_ms?: number | null;
}

export interface RealtimeAskUserPromptPayloadWrapper {
  kind: 'ask_user_prompt';
  conversation_id: string;
  conversation_turn_id?: string | null;
  project_id?: string | null;
  prompt_id: string;
  action: string;
  status?: string | null;
  tool_call_id?: string | null;
  prompt_kind?: string | null;
  title?: string | null;
  message?: string | null;
  allow_cancel?: boolean | null;
  timeout_ms?: number | null;
  payload?: Record<string, unknown> | null;
}

export interface RealtimeChatStreamPayloadWrapper {
  kind: 'chat_stream';
  conversation_id: string;
  conversation_turn_id?: string | null;
  project_id?: string | null;
  user_message_id?: string | null;
  stream_type: string;
  raw: {
    type?: string;
    timestamp?: string;
    content?: unknown;
    data?: unknown;
    task_turn_review?: {
      attempted?: boolean;
      outcome?: string;
      rounds?: number;
      [key: string]: unknown;
    } | null;
    success?: boolean;
    is_error?: boolean;
    code?: string;
    message?: string;
    result?: {
      content?: unknown;
      task_turn_review?: {
        attempted?: boolean;
        outcome?: string;
        rounds?: number;
        [key: string]: unknown;
      } | null;
      persisted_user_message?: unknown;
      persisted_user_message_id?: string | null;
      persisted_assistant_message?: unknown;
      persisted_assistant_message_id?: string | null;
      [key: string]: unknown;
    } | null;
    [key: string]: unknown;
  };
}

export interface RealtimeRemoteSftpTransferPayloadWrapper {
  kind: 'remote_sftp_transfer';
  id: string;
  connection_id: string;
  direction: 'upload' | 'download';
  state: 'pending' | 'running' | 'cancelling' | 'success' | 'error' | 'cancelled';
  total_bytes?: number | null;
  transferred_bytes: number;
  percent?: number | null;
  current_path?: string | null;
  message?: string | null;
  error?: string | null;
  created_at: string;
  updated_at: string;
}

export type RealtimeProjectScopedPayload =
  | RealtimeReviewRepairPayloadWrapper
  | RealtimeConversationSummariesUpdatedPayloadWrapper
  | RealtimeProjectChangeSummaryPayloadWrapper
  | RealtimeTerminalStatePayloadWrapper
  | RealtimeTerminalListInvalidatedPayloadWrapper
  | RealtimeProjectRunStatePayloadWrapper
  | RealtimeProjectRunInstancePayloadWrapper
  | RealtimeProjectRunCatalogPayloadWrapper
  | RealtimeProjectMembersUpdatedPayloadWrapper;

export interface RealtimeEventEnvelope {
  type: 'event';
  event: string;
  user_id: string;
  conversation_id?: string | null;
  project_id?: string | null;
  payload?:
    | RealtimeContactsUpdatedPayloadWrapper
    | RealtimeNotepadUpdatedPayloadWrapper
    | RealtimeProjectsUpdatedPayloadWrapper
    | RealtimeRemoteConnectionsUpdatedPayloadWrapper
    | RealtimeSessionsUpdatedPayloadWrapper
    | RealtimeProjectScopedPayload
    | RealtimeTaskBoardPayloadWrapper
    | RealtimeAskUserPromptPayloadWrapper
    | RealtimeChatStreamPayloadWrapper
    | RealtimeRemoteSftpTransferPayloadWrapper;
  ts: string;
}

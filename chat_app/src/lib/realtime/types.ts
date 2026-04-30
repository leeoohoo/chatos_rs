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
}

export interface RealtimeRemoteConnectionsUpdatedPayloadWrapper {
  kind: 'remote_connections_updated';
  reason: string;
  connection_id?: string | null;
}

export interface RealtimeSessionsUpdatedPayloadWrapper {
  kind: 'sessions_updated';
  reason: string;
  session_id?: string | null;
  project_id?: string | null;
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
}

export interface RealtimeTerminalListInvalidatedPayloadWrapper {
  kind: 'terminal_list_invalidated';
  terminal_id?: string | null;
  project_id?: string | null;
  reason: string;
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

export interface RealtimeUiPromptPayloadWrapper {
  kind: 'ui_prompt';
  conversation_id: string;
  conversation_turn_id?: string | null;
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
    result?: {
      content?: unknown;
    } | null;
    success?: boolean;
    is_error?: boolean;
    code?: string;
    message?: string;
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
  | RealtimeProjectChangeSummaryPayloadWrapper
  | RealtimeTerminalStatePayloadWrapper
  | RealtimeTerminalListInvalidatedPayloadWrapper
  | RealtimeProjectRunStatePayloadWrapper
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
    | RealtimeUiPromptPayloadWrapper
    | RealtimeChatStreamPayloadWrapper
    | RealtimeRemoteSftpTransferPayloadWrapper;
  ts: string;
}

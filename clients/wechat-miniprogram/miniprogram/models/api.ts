export type AuthUser = {
  id: string
  username: string
  display_name: string
  role: string
  principal_type: string
}

export type WeChatLoginResponse =
  | { status: 'binding_required' }
  | {
      status: 'authenticated'
      token: string
      user: AuthUser
      client_session_id: string
    }

export type BindClaimResponse = {
  status: 'claimed'
  claim_id: string
  claim_secret: string
  expires_at_unix: number
}

export type BindClaimResult =
  | { status: 'pending_desktop_confirmation' }
  | {
      status: 'authenticated'
      token: string
      user: AuthUser
      client_session_id: string
    }

export type DeviceSummary = {
  id: string
  display_name: string
  client_version?: string
  os?: string
  status: 'online' | 'offline' | 'revoked'
  is_online: boolean
  last_seen_at?: string
  updated_at: string
}

export type Conversation = {
  id: string
  title: string
  status: string
  message_count: number
  updated_at: string
  metadata?: Record<string, unknown>
}

export type CompanionResource = {
  id: string
  kind: 'contact' | 'project'
  title: string
  subtitle?: string
  conversation_id?: string
  message_count: number
  updated_at?: string
}

export type ConversationMessage = {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  revision?: number
  sequence_no?: number
  message_mode?: string
  message_source?: string
  task_id?: string
  created_at?: string
  metadata?: Record<string, unknown>
}

export type CompactHistoryPage = {
  items: ConversationMessage[]
  has_more: boolean
  next_before?: string
  snapshot_revision: number
}

export type SendMessageResponse = {
  accepted: boolean
  conversation_id: string
  turn_id?: string
  user_message_id: string
}

export type GuidanceResponse = {
  accepted: boolean
  message_id?: string
}

export type ConversationRuntimeContext = {
  turn_id?: string
  conversation_turn_id?: string
  status?: string
  active_in_runtime?: boolean
  [key: string]: unknown
}

export type CompanionTaskLastRun = {
  id: string
  status?: string
  model_phase_status?: string
  result_summary?: string
  report?: { content?: string }
  error_message?: string
  started_at?: string
  finished_at?: string
}

export type CompanionTask = {
  id: string
  title: string
  description?: string
  objective?: string
  status?: string
  priority?: number
  tags?: string[]
  result_summary?: string
  process_log?: string
  last_run?: CompanionTaskLastRun
  created_at?: string
  updated_at?: string
}

export type CompanionTaskListResponse = {
  items: CompanionTask[]
}

export type CompanionApproval = {
  id: string
  command: string
  context?: string
  source: string
  risk: string
  reason?: string
  created_at: string
  available_decisions: Array<'accept' | 'acceptForSession' | 'decline'>
}

export type CompanionApprovalResolution = {
  success: boolean
  approval_id: string
}

export type AskUserField = {
  key: string
  label: string
  description?: string
  placeholder?: string
  defaultValue: string
  required: boolean
  multiline: boolean
  secret: boolean
}

export type AskUserChoiceOption = {
  value: string
  label: string
  description?: string
  selected?: boolean
}

export type AskUserChoice = {
  multiple: boolean
  options: AskUserChoiceOption[]
  defaults: string[]
  minimum: number
  maximum: number
}

export type AskUserPromptRecord = {
  id: string
  conversation_id: string
  conversation_turn_id: string
  kind: string
  status: 'pending' | 'ok' | 'canceled' | 'timeout' | 'failed'
  prompt: Record<string, unknown>
  created_at: string
  updated_at: string
}

export type AskUserPromptListResponse = {
  success: boolean
  prompts: AskUserPromptRecord[]
}

export type AskUserPromptMutationResponse = {
  success: boolean
  prompt: AskUserPromptRecord
}

export type ClientSession = {
  id: string
  client_type: string
  created_at: string
  last_seen_at: string
  expires_at_unix: number
  revoked_at?: string
}

export type WsTicketResponse = {
  ticket: string
  expires_in: number
  expires_at: string
}

export type RealtimeEnvelope = {
  topic?: string
  event?: string
  type?: string
  conversation_id?: string
  turn_id?: string
  payload?: unknown
  [key: string]: unknown
}

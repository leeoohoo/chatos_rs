export interface MemoryAgentSkillResponse {
  id: string;
  name: string;
  content: string;
}

export interface MemorySkillResponse {
  id: string;
  user_id: string;
  plugin_source: string;
  name: string;
  description?: string | null;
  content: string;
  source_path: string;
  version?: string | null;
  updated_at: string;
}

export interface MemorySkillPluginCommandResponse {
  name: string;
  source_path: string;
  description?: string | null;
  argument_hint?: string | null;
  content: string;
}

export interface MemorySkillPluginResponse {
  id: string;
  user_id: string;
  source: string;
  name: string;
  category?: string | null;
  description?: string | null;
  version?: string | null;
  repository?: string | null;
  branch?: string | null;
  cache_path?: string | null;
  content?: string | null;
  commands?: MemorySkillPluginCommandResponse[];
  command_count?: number;
  installed: boolean;
  discoverable_skills: number;
  installed_skill_count: number;
  updated_at: string;
}

export interface AiCreateAgentResponse {
  created: boolean;
  agent: MemoryAgentResponse;
  source?: string;
  model?: string;
  provider?: string;
  content?: string | null;
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

export interface MemoryAgentSessionResponse {
  id: string;
  user_id: string;
  project_id?: string | null;
  title?: string | null;
  metadata?: Record<string, unknown> | string | null;
  status: string;
  created_at: string;
  updated_at: string;
  archived_at?: string | null;
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

export interface CreateAgentPayload {
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
}

export interface UpdateAgentPayload {
  name?: string;
  description?: string | null;
  category?: string | null;
  role_definition?: string;
  plugin_sources?: string[];
  skills?: MemoryAgentSkillResponse[];
  skill_ids?: string[];
  default_skill_ids?: string[];
  mcp_policy?: Record<string, unknown> | null;
  project_policy?: Record<string, unknown> | null;
  enabled?: boolean;
}

export interface AiCreateAgentPayload {
  user_id?: string;
  model_config_id?: string;
  requirement: string;
  name?: string;
  category?: string;
  description?: string;
  role_definition?: string;
  skill_ids?: string[];
  skill_prompts?: string[];
  enabled?: boolean;
  mcp_enabled?: boolean;
  enabled_mcp_ids?: string[];
  project_id?: string;
  project_root?: string;
}

export interface StreamChatOptions {
  turnId?: string;
  contactAgentId?: string | null;
  remoteConnectionId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
  skillsEnabled?: boolean;
  selectedSkillIds?: string[];
}

export interface StreamChatCommandResponse {
  accepted?: boolean;
  conversation_id?: string;
  turn_id?: string | null;
}

export interface StreamChatModelConfigPayload {
  id?: string;
  provider: string;
  model_name: string;
  temperature?: number;
  thinking_level?: string | null;
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

export interface RuntimeGuidanceSubmitPayload {
  conversationId: string;
  turnId: string;
  content: string;
  projectId?: string | null;
}

export interface AgentToolDefinition {
  name: string;
  description?: string | null;
  parameters?: Record<string, unknown> | null;
  server_name?: string | null;
  display_group?: string | null;
}

export interface AgentToolsResponse {
  tools?: AgentToolDefinition[];
  unavailable_tools?: Array<{
    server_name?: string | null;
    tool_name?: string | null;
    reason?: string | null;
  }>;
  owner?: string | null;
  service?: string | null;
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

export interface TurnRuntimeSnapshotContextItem {
  role?: string | null;
  type?: string | null;
  source?: string | null;
  content: string;
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
  unavailable_builtin_tools?: Array<{
    server_name: string;
    tool_name: string;
    reason?: string | null;
  }>;
  builtin_mcp_prompt?: {
    prompt_source_path?: string | null;
    all_section_ids?: string[];
    selected_section_ids?: string[];
    omitted_section_ids?: string[];
    requested_builtin_server_names?: string[];
    active_builtin_server_names?: string[];
    omitted_builtin_server_names?: string[];
    runtime_limitations?: string | null;
  } | null;
  actual_context_mode?: string | null;
  actual_context_items?: TurnRuntimeSnapshotContextItem[];
  last_model_request_payload?: Record<string, unknown> | null;
}

export interface TurnRuntimeSnapshot {
  id: string;
  conversation_id: string;
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
  conversation_id: string;
  turn_id?: string | null;
  status: string;
  snapshot_source: string;
  active_in_runtime?: boolean;
  snapshot?: TurnRuntimeSnapshot | null;
}

export interface TaskManagerUpdatePayload {
  title?: string;
  details?: string;
  priority?: 'high' | 'medium' | 'low';
  status?: 'todo' | 'doing' | 'blocked' | 'done';
  tags?: string[];
  due_at?: string | null;
  outcome_summary?: string;
  outcome_items?: Array<{
    kind?: string;
    text?: string;
    importance?: 'high' | 'medium' | 'low';
    refs?: string[];
  }>;
  resume_hint?: string;
  blocker_reason?: string;
  blocker_needs?: string[];
  blocker_kind?: string;
  completed_at?: string | null;
  last_outcome_at?: string | null;
}

export interface TaskManagerTaskResponse {
  id: string;
  title?: string;
  details?: string | null;
  priority?: 'high' | 'medium' | 'low' | null;
  status?: 'todo' | 'doing' | 'blocked' | 'done' | null;
  tags?: string[];
  due_at?: string | null;
  outcome_summary?: string | null;
  outcome_items?: Array<{
    kind?: string;
    text?: string;
    importance?: 'high' | 'medium' | 'low';
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
  outcome_summary?: string;
  outcome_items?: Array<{
    kind?: string;
    text?: string;
    importance?: 'high' | 'medium' | 'low';
    refs?: string[];
  }>;
  resume_hint?: string;
  blocker_reason?: string;
  blocker_needs?: string[];
  blocker_kind?: string;
  [key: string]: unknown;
}

export interface TaskReviewItemResponse {
  review_id: string;
  conversation_id?: string;
  conversation_turn_id?: string | null;
  draft_tasks?: TaskReviewTaskDraft[] | null;
  timeout_ms?: number | null;
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
  conversation_id?: string;
  conversation_turn_id?: string | null;
  status?: string;
  title?: string | null;
  message?: string | null;
  schema?: Record<string, unknown> | null;
  values?: Record<string, unknown> | null;
  created_at?: string;
  updated_at?: string;
}

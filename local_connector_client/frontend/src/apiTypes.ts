// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  role: string;
}

export interface WorkspaceRecord {
  id: string;
  alias: string;
  absolute_root: string;
  fingerprint: string;
}

export interface DockerStatus {
  installed: boolean;
  running: boolean;
  version?: string | null;
  error?: string | null;
}

export interface SandboxState {
  enabled: boolean;
  backend?: string | null;
  isolation?: string | null;
  selected_image_ref?: string | null;
}

export interface ConnectorStatus {
  configured: boolean;
  connector_running: boolean;
  developer_mode?: boolean;
  developer_cloud_base_url?: string | null;
  developer_user_service_base_url?: string | null;
  developer_chatos_web_url?: string | null;
  cloud_base_url?: string | null;
  user_service_base_url?: string | null;
  device_id?: string | null;
  device_name?: string | null;
  user?: AuthUser | null;
  workspaces: WorkspaceRecord[];
  sandbox: SandboxState;
  docker: DockerStatus;
}

export interface FsEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface FsListResponse {
  path: string;
  parent?: string | null;
  entries: FsEntry[];
}

export interface TerminalExecResponse {
  command: string;
  args: string[];
  cwd: string;
  success: boolean;
  exit_code?: number | null;
  timed_out: boolean;
  stdout: string;
  stderr: string;
  error?: string;
}

export interface CommandHistoryEntry {
  id: string;
  source: string;
  workspace_id?: string | null;
  workspace_alias?: string | null;
  cwd?: string | null;
  command: string;
  args: string[];
  display: string;
  status: string;
  exit_code?: number | null;
  stdout_preview?: string | null;
  stderr_preview?: string | null;
  error?: string | null;
  started_at: string;
  finished_at?: string | null;
  request_id?: string | null;
  terminal_session_id?: string | null;
  sandbox_id?: string | null;
  tool_name?: string | null;
}

export interface CommandHistoryResponse {
  entries: CommandHistoryEntry[];
}

export type ApprovalMode = 'request_approval' | 'auto_approval' | 'full_control';

export interface ApprovalProjectKey {
  owner_user_id: string;
  device_id: string;
  workspace_id: string;
  project_id?: string | null;
  project_root_relative_path: string;
  project_anchor_relative_path?: string | null;
}

export interface ApprovalAiSettings {
  enabled: boolean;
  base_url?: string | null;
  api_key?: string | null;
  has_api_key?: boolean;
  model?: string | null;
  provider: string;
  supports_responses: boolean;
  temperature?: number | null;
  max_output_tokens?: number | null;
  thinking_level?: string | null;
  request_body_limit_bytes?: number | null;
}

export interface ApprovalMemorySettings {
  source_id: string;
  timeout_ms: number;
}

export interface ProjectApprovalState {
  project_key: ApprovalProjectKey;
  mode?: ApprovalMode | null;
  ai_enabled: boolean;
  updated_at: string;
}

export interface CommandWhitelistEntry {
  id: string;
  project_key: ApprovalProjectKey;
  command_fingerprint: string;
  command_display: string;
  normalized_command: string;
  cwd_scope: string;
  created_by: string;
  created_at: string;
  enabled: boolean;
}

export interface ApprovalHistoryEntry {
  id: string;
  request_id: string;
  project_key: ApprovalProjectKey;
  command: string;
  normalized_command: string;
  cwd: string;
  source: string;
  mode: ApprovalMode;
  decision: string;
  decision_source: string;
  risk: string;
  reason?: string | null;
  whitelist_entry_id?: string | null;
  created_at: string;
}

export interface PendingApprovalItem {
  id: string;
  request_id: string;
  project_key: ApprovalProjectKey;
  command: string;
  cwd: string;
  source: string;
  risk: string;
  reason?: string | null;
  created_at: string;
}

export interface ApprovalSettings {
  default_mode: ApprovalMode;
  projects: ProjectApprovalState[];
  whitelist: CommandWhitelistEntry[];
  history: ApprovalHistoryEntry[];
  ai: ApprovalAiSettings;
  memory: ApprovalMemorySettings;
}

export interface PendingApprovalsResponse {
  items: PendingApprovalItem[];
  reviewing?: PendingApprovalItem[];
}

export interface LocalModelConfig {
  id: string;
  server_model_config_id?: string | null;
  name: string;
  provider: string;
  model: string;
  model_name: string;
  base_url?: string | null;
  has_api_key: boolean;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  thinking_level?: string | null;
  task_usage_scenario?: string | null;
  task_thinking_level?: string | null;
  temperature?: number | null;
  max_output_tokens?: number | null;
  created_at: string;
  updated_at: string;
}

export interface LocalModelSettings {
  memory_summary_model_config_id?: string | null;
  memory_summary_thinking_level?: string | null;
  project_management_agent_model_config_id?: string | null;
  project_management_agent_thinking_level?: string | null;
  command_approval_model_config_id?: string | null;
  command_approval_thinking_level?: string | null;
  updated_at?: string | null;
}

export interface LocalModelConfigListResponse {
  items: LocalModelConfig[];
  settings: LocalModelSettings;
}

export interface LocalRuntimeSettings {
  ai_agent_max_iterations: number;
  developer_mode: boolean;
  developer_cloud_base_url: string;
  developer_user_service_base_url: string;
  developer_chatos_web_url: string;
}

export type SystemPermissionStatus =
  | 'ready'
  | 'needs_attention'
  | 'missing_dependency'
  | 'not_applicable'
  | 'unknown';

export interface SystemPermissionItem {
  id: string;
  label: string;
  summary: string;
  status: SystemPermissionStatus | string;
  status_label: string;
  required: boolean;
  can_request: boolean;
  request_label: string;
  settings_target?: string | null;
  builtin_kinds: string[];
  skill_ids: string[];
  note: string;
  last_error?: string | null;
}

export interface SystemPermissionsResponse {
  platform: string;
  platform_label: string;
  items: SystemPermissionItem[];
}

export interface LocalProviderModel {
  id: string;
  owned_by?: string | null;
  context_length?: number | null;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
}

export interface LocalModelCatalogResponse {
  provider: string;
  base_url: string;
  source: string;
  fetched_at?: string | null;
  models: LocalProviderModel[];
  error?: string | null;
}

export interface LocalModelConfigDraft {
  id?: string | null;
  server_model_config_id?: string | null;
  name: string;
  provider?: string | null;
  model?: string | null;
  base_url?: string | null;
  api_key?: string | null;
  copy_api_key_from_id?: string | null;
  clear_api_key?: boolean | null;
  enabled?: boolean | null;
  supports_images?: boolean | null;
  supports_reasoning?: boolean | null;
  supports_responses?: boolean | null;
  thinking_level?: string | null;
  task_usage_scenario?: string | null;
  task_thinking_level?: string | null;
  temperature?: number | null;
  clear_temperature?: boolean | null;
  max_output_tokens?: number | null;
  clear_max_output_tokens?: boolean | null;
}

export type LocalMcpTransport = 'stdio' | 'http';

export interface LocalMcpConfig {
  manifest_id: string;
  plugin_mcp_id?: string | null;
  internal_name: string;
  display_name: string;
  description?: string | null;
  transport: LocalMcpTransport;
  command?: string | null;
  args: string[];
  env: Record<string, string>;
  url?: string | null;
  headers: Record<string, string>;
  timeout_ms?: number | null;
  enabled: boolean;
  sync_status: string;
  last_check_status: string;
  last_checked_at?: string | null;
  last_error?: string | null;
  tool_count: number;
  created_at: string;
  updated_at: string;
}

export interface LocalMcpConfigDraft {
  manifest_id?: string | null;
  display_name: string;
  description?: string | null;
  transport: LocalMcpTransport;
  enabled?: boolean | null;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  url?: string | null;
  headers?: Record<string, string>;
  timeout_ms?: number | null;
}

export interface LocalSkillInstallation {
  id: string;
  owner_user_id: string;
  device_id: string;
  skill_id: string;
  bundle_id: string;
  version: string;
  bundle_hash: string;
  platform: string;
  status: string;
  dependency_status: string;
  last_error?: string | null;
  last_checked_at: string;
}

export interface LocalSkillRecord {
  id: string;
  name: string;
  display_name: string;
  description?: string | null;
  enabled: boolean;
  content: {
    kind: string;
    bundle_id?: string | null;
    bundle_version?: string | null;
    bundle_hash?: string | null;
    entrypoint_kind?: string | null;
  };
  metadata: {
    version?: string | null;
    category?: string | null;
    tags: string[];
    extra: Record<string, unknown>;
  };
}

export interface LocalSkillCatalogItem {
  skill: LocalSkillRecord;
  user_enabled: boolean;
  available: boolean;
  status: string;
  reason?: string | null;
  installation?: LocalSkillInstallation | null;
}

export interface LocalSkillCatalogResponse {
  items: LocalSkillCatalogItem[];
  total: number;
}

export interface SandboxImageFeature {
  id: string;
  label: string;
  description: string;
  default_version: string;
  versions: Array<{
    id: string;
    label: string;
    description: string;
    default: boolean;
  }>;
}

export interface SandboxImageCatalog {
  image_tag_prefix: string;
  features: SandboxImageFeature[];
  images: Array<{
    id: string;
    image_ref: string;
    features: string[];
    created_at?: string;
  }>;
}

export interface SandboxImageJob {
  id: string;
  image_id: string;
  image_name: string;
  status: string;
  features: string[];
  output?: string | null;
  error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface SandboxLease {
  id: string;
  sandbox_id: string;
  tenant_id: string;
  user_id: string;
  project_id: string;
  run_id: string;
  workspace_root: string;
  run_workspace: string;
  backend: string;
  backend_id?: string | null;
  image_id?: string | null;
  image_ref?: string | null;
  status: string;
  agent_endpoint?: string | null;
  tools: string[];
  created_at: string;
  updated_at: string;
  expires_at: string;
  destroyed_at?: string | null;
  last_error?: string | null;
}

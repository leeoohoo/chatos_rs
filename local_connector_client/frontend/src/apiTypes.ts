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
  project_config_trusted?: boolean;
  project_config_trust_stale?: boolean;
  project_config_trusted_at?: string | null;
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
  default_backend?: SandboxBackendKind | null;
  isolation?: string | null;
  filesystem_isolation?: boolean | null;
  network_isolation?: boolean | null;
  process_tree_control?: boolean | null;
  isolation_note?: string | null;
  default_permission_profile_id?: PermissionProfileId | null;
  default_permission_profile_name?: string | null;
  default_permission_profile_provenance?: PermissionProfileProvenance | null;
  permission_configuration_error?: string | null;
  default_approval_policy?: SandboxApprovalPolicy | null;
  default_approval_reviewer?: SandboxApprovalReviewer | null;
  default_network_requirements?: SandboxNetworkRequirements | null;
  allowed_permission_profiles?: Record<string, boolean> | null;
  configured_allowed_permission_profiles?: Record<string, boolean> | null;
  permission_profiles?: PermissionProfileSummary[] | null;
  custom_permission_profiles?: Record<string, CustomPermissionProfile> | null;
  effective_custom_permission_profiles?: Record<string, CustomPermissionProfile> | null;
  managed_permission_profiles?: string[] | null;
  policy_revision?: string | null;
  effective_policy?: SandboxEffectivePolicy | null;
  effective_permissions?: EffectivePermissionSnapshot | null;
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
  permission_scope?: 'turn' | 'session' | null;
  created_at: string;
}

export type FileSystemAccessMode = 'read' | 'write' | 'deny';

export type FileSystemPermissionPath =
  | { type: 'path'; path: string }
  | { type: 'glob_pattern'; pattern: string }
  | {
      type: 'special';
      value: {
        kind: 'root' | 'minimal' | 'project_roots' | 'tmpdir' | 'slash_tmp' | 'unknown';
        path?: string | null;
        subpath?: string | null;
      };
    };

export interface RequestPermissionProfile {
  fileSystem?: {
    entries?: Array<{ access: FileSystemAccessMode; path: FileSystemPermissionPath }> | null;
    globScanMaxDepth?: number | null;
    read?: string[] | null;
    write?: string[] | null;
  } | null;
  network?: { enabled?: boolean | null } | null;
}

export type CommandExecutionApprovalDecision =
  | 'accept'
  | 'acceptForSession'
  | 'decline'
  | 'cancel';

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
  requested_permissions?: RequestPermissionProfile | null;
  available_decisions?: CommandExecutionApprovalDecision[];
}

export interface ApprovalSettings {
  default_mode: ApprovalMode;
  settings_revision?: string | null;
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
  environment_initialization_model_config_id?: string | null;
  environment_initialization_thinking_level?: string | null;
  command_approval_model_config_id?: string | null;
  command_approval_thinking_level?: string | null;
  updated_at?: string | null;
}

export interface LocalModelConfigListResponse {
  items: LocalModelConfig[];
  settings: LocalModelSettings;
}

export interface LocalRuntimeSettings {
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
    status: string;
    rebuildable?: boolean;
    created_at?: string;
  }>;
}

export type SandboxBackendKind = 'docker' | 'local_process';
export type PermissionProfileId = 'read_only' | 'workspace_write' | 'full_access';
export type SandboxApprovalPolicy = 'on_request' | 'never';
export type SandboxApprovalReviewer = 'user' | 'auto_review';

export interface PermissionProfileSummary {
  id: string;
  allowed: boolean;
  description?: string | null;
}

export interface CustomPermissionProfile {
  description?: string | null;
  extends?: string | null;
  workspaceRoots?: Record<string, boolean>;
  fileSystem?: {
    entries?: Array<{ access: FileSystemAccessMode; path: FileSystemPermissionPath }> | null;
    globScanMaxDepth?: number | null;
    read?: string[] | null;
    write?: string[] | null;
  } | null;
  network?: SandboxNetworkRequirements | null;
}
export type SandboxNetworkProxyMode = 'limited' | 'full';
export type SandboxNetworkDomainPermission = 'allow' | 'deny';
export type SandboxBackendReadinessStatus =
  | 'ready'
  | 'setup_required'
  | 'unsupported'
  | 'under_development';

export interface SandboxEffectivePolicy {
  sandbox_mode: SandboxBackendKind;
  permission_profile_id: PermissionProfileId;
  approval_policy: SandboxApprovalPolicy;
  approval_reviewer: SandboxApprovalReviewer;
  policy_revision?: string | null;
  additional_writable_roots?: string[];
}

export interface SandboxNetworkRequirements {
  enabled?: boolean | null;
  domains?: Record<string, SandboxNetworkDomainPermission> | null;
  unixSockets?: Record<string, SandboxNetworkDomainPermission> | null;
  allowLocalBinding?: boolean | null;
  allowUpstreamProxy?: boolean | null;
  dangerouslyAllowAllUnixSockets?: boolean | null;
  dangerouslyAllowNonLoopbackProxy?: boolean | null;
  managedAllowedDomainsOnly?: boolean | null;
  httpPort?: number | null;
  socksPort?: number | null;
  allowedDomains?: string[] | null;
  deniedDomains?: string[] | null;
  allowUnixSockets?: string[] | null;
  mode?: SandboxNetworkProxyMode | null;
  enableSocks5?: boolean | null;
  enableSocks5Udp?: boolean | null;
}

export type PermissionProfileProvenance =
  | 'built_in'
  | 'user'
  | 'project'
  | 'managed'
  | 'external'
  | 'disabled';

export interface EffectivePermissionSnapshot {
  activeProfile: { id: string; extends?: string | null };
  provenance: PermissionProfileProvenance;
  fileSystem:
    | {
        type: 'restricted';
        entries: Array<{ access: FileSystemAccessMode; path: FileSystemPermissionPath }>;
        glob_scan_max_depth?: number | null;
      }
    | { type: 'unrestricted' };
  network:
    | { type: 'restricted'; requirements: SandboxNetworkRequirements }
    | { type: 'unrestricted' };
  runtimeWorkspaceRoots: string[];
  policyRevision?: string | null;
}

export interface SandboxBackendCapability {
  backend: SandboxBackendKind;
  status: SandboxBackendReadinessStatus;
  selectable: boolean;
  filesystem_isolation: boolean;
  network_isolation: boolean;
  process_tree_control: boolean;
  message: string;
}

export interface SandboxCapabilities {
  backends: SandboxBackendCapability[];
}

export interface SandboxSettings {
  enabled: boolean;
  default_backend: SandboxBackendKind;
  default_permission_profile_id: PermissionProfileId;
  default_permission_profile_name: string;
  default_permission_profile_provenance?: PermissionProfileProvenance | null;
  permission_configuration_error?: string | null;
  default_approval_policy: SandboxApprovalPolicy;
  default_approval_reviewer: SandboxApprovalReviewer;
  default_network_requirements: SandboxNetworkRequirements;
  allowed_permission_profiles?: Record<string, boolean> | null;
  configured_allowed_permission_profiles?: Record<string, boolean> | null;
  permission_profiles: PermissionProfileSummary[];
  custom_permission_profiles: Record<string, CustomPermissionProfile>;
  effective_custom_permission_profiles?: Record<string, CustomPermissionProfile> | null;
  managed_permission_profiles?: string[] | null;
  policy_revision?: string | null;
  selected_image_ref?: string | null;
  effective_policy: SandboxEffectivePolicy;
  effective_permissions: EffectivePermissionSnapshot;
}

export type SandboxSettingsUpdate = Partial<
  Omit<
    SandboxSettings,
    | 'permission_profiles'
    | 'custom_permission_profiles'
    | 'effective_custom_permission_profiles'
    | 'managed_permission_profiles'
    | 'configured_allowed_permission_profiles'
    | 'default_permission_profile_provenance'
    | 'permission_configuration_error'
    | 'effective_policy'
    | 'effective_permissions'
  >
> & {
  permission_profiles?: Record<string, CustomPermissionProfile>;
  permission_profiles_toml?: string;
  risk_acknowledged?: boolean;
};

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
  effective_policy: SandboxEffectivePolicy;
  effective_permissions: EffectivePermissionSnapshot;
}

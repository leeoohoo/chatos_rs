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
}

export interface ConnectorStatus {
  configured: boolean;
  connector_running: boolean;
  developer_mode?: boolean;
  browser_full_cdp_access_enabled?: boolean;
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

export interface ProjectApprovalState {
  project_key: ApprovalProjectKey;
  mode?: ApprovalMode | null;
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

export interface ApprovalActionAuditDetail {
  key: string;
  value: string;
}

export interface ApprovalActionAudit {
  kind: string;
  operation: string;
  details?: ApprovalActionAuditDetail[];
  privacy?: string | null;
  safety?: string | null;
  recovery?: string | null;
}

export interface ApprovalConfirmationRequirement {
  kind: string;
  risk: string;
  challenge: string;
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
  action_audit?: ApprovalActionAudit | null;
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
  action_audit?: ApprovalActionAudit | null;
  confirmation?: ApprovalConfirmationRequirement | null;
  available_decisions?: CommandExecutionApprovalDecision[];
}

export interface ApprovalSettings {
  default_mode: ApprovalMode;
  settings_revision?: string | null;
  projects: ProjectApprovalState[];
  whitelist: CommandWhitelistEntry[];
  history: ApprovalHistoryEntry[];
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
  prompt_vendor?: 'glm' | 'deepseek' | 'gpt' | 'kimi' | null;
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
  model_request_max_retries?: number;
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
  browser_full_cdp_access_enabled: boolean;
  developer_cloud_base_url: string;
  developer_user_service_base_url: string;
  developer_chatos_web_url: string;
}

export type PluginInstallStatus =
  | 'not_installed'
  | 'downloading'
  | 'verifying'
  | 'rejected'
  | 'installing'
  | 'installed'
  | 'updating'
  | 'rolling_back'
  | 'uninstalling';

export type PluginTransactionOperation = 'install' | 'update' | 'rollback' | 'uninstall';

export interface LocalPluginPermissionRequirement {
  permission: string;
  required: boolean;
  reason?: string | null;
  components?: string[];
}

export interface LocalPluginComponentDescriptor {
  component_key: string;
  kind:
    | 'skill_collection'
    | 'mcp_server'
    | 'connected_app'
    | 'command'
    | 'agent'
    | 'hook_set'
    | 'ui_contribution';
  display_name: string;
  runtime_kind: string;
  entrypoint?: { path: string } | null;
  required: boolean;
  permissions: LocalPluginPermissionRequirement[];
  metadata: Record<string, unknown>;
}

export interface LocalInstalledPluginVersion {
  release_id: string;
  version: string;
  artifact_sha256: string;
  manifest_sha256: string;
  signature_key_id: string;
  relative_installation_path: string;
  installed_at: string;
  package_file_sha256: Record<string, string>;
  inventory: {
    dependencies: Record<string, unknown>;
    permissions: LocalPluginPermissionRequirement[];
    auth_component_keys: string[];
    components: LocalPluginComponentDescriptor[];
  };
}

export interface LocalInstalledPlugin {
  plugin_id: string;
  marketplace_id: string;
  plugin_name: string;
  active_version?: string | null;
  previous_version?: string | null;
  versions: Record<string, LocalInstalledPluginVersion>;
}

export interface LocalPluginTransactionRecord {
  transaction_id: string;
  operation: PluginTransactionOperation;
  status: PluginInstallStatus;
  plugin_id: string;
  release_id?: string | null;
  from_version?: string | null;
  target_version?: string | null;
  relative_staging_path?: string | null;
  relative_final_path?: string | null;
  relative_storage_path?: string | null;
  relative_trash_path?: string | null;
  downloaded_bytes: number;
  total_bytes?: number | null;
  started_at: string;
  updated_at: string;
  completed_at?: string | null;
  recovered_after_restart: boolean;
  last_error?: string | null;
}

export interface LocalPluginStatusSnapshot {
  registry: {
    schema_version: number;
    plugins: Record<string, LocalInstalledPlugin>;
  };
  transactions: {
    schema_version: number;
    active: Record<string, LocalPluginTransactionRecord>;
    history: LocalPluginTransactionRecord[];
  };
  runtime: PluginRuntimeTelemetrySnapshot;
}

export type PluginRuntimeSessionStatus =
  | 'ready'
  | 'executing'
  | 'degraded'
  | 'failed'
  | 'cancelled'
  | 'expired';

export type PluginRuntimeTelemetryPhase =
  | 'prepare'
  | 'execute'
  | 'health'
  | 'cancel'
  | 'lifecycle';
export type PluginRuntimeTelemetryEventStatus =
  | 'started'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'expired';

export interface PluginRuntimeSessionTelemetry {
  run_id: string;
  adapter_session_id: string;
  plugin_id: string;
  release_id: string;
  component_key: string;
  status: PluginRuntimeSessionStatus;
  active_executions: number;
  execution_count: number;
  last_operation?: string | null;
  last_tool_name?: string | null;
  health_status?: string | null;
  started_at: string;
  updated_at: string;
  completed_at?: string | null;
  expires_at: number;
  last_error?: string | null;
}

export interface PluginRuntimeTelemetryEvent {
  sequence: number;
  run_id: string;
  adapter_session_id?: string | null;
  plugin_id: string;
  release_id: string;
  component_key: string;
  phase: PluginRuntimeTelemetryPhase;
  status: PluginRuntimeTelemetryEventStatus;
  operation?: string | null;
  tool_name?: string | null;
  timestamp: string;
  duration_ms?: number | null;
  health_status?: string | null;
  error?: string | null;
}

export interface PluginRuntimeTelemetrySnapshot {
  schema_version: number;
  revision: number;
  sessions: PluginRuntimeSessionTelemetry[];
  recent_events: PluginRuntimeTelemetryEvent[];
}

export interface LocalPluginStatusEvent {
  schema_version: number;
  cursor: string;
  changed: boolean;
  snapshot?: LocalPluginStatusSnapshot | null;
}

export interface LocalPluginStoreItem {
  plugin_id: string;
  marketplace_id: string;
  name: string;
  display_name: string;
  description: string;
  category: string;
  publisher: string;
  visibility: 'public' | 'personal';
  featured: boolean;
  latest_version: string;
  latest_release_id: string;
  published_at: string;
  artifact_revision: string;
  skill_ids: string[];
  install_source: 'bundled' | 'network' | 'installed';
  install_available: boolean;
  lifecycle_status: string;
  update_available: boolean;
  rollback_available: boolean;
  preference?: UserPluginPreferenceRecord | null;
  auto_update_state?: LocalPluginAutoUpdateRecord | null;
  installation?: LocalInstalledPlugin | null;
  active_transaction?: LocalPluginTransactionRecord | null;
  latest_transaction?: LocalPluginTransactionRecord | null;
}

export interface LocalPluginStoreSnapshot {
  schema_version: number;
  catalog_revision: string;
  marketplace_id: string;
  marketplace_name: string;
  bundled_install_available: boolean;
  network_install_available: boolean;
  network_catalog_error?: string | null;
  auto_update_error?: string | null;
  runtime: PluginRuntimeTelemetrySnapshot;
  items: LocalPluginStoreItem[];
}

export interface UserPluginPreferenceRecord {
  owner_user_id: string;
  plugin_id: string;
  enabled: boolean;
  auto_update: boolean;
  release_channel: string;
  enabled_components: string[];
  updated_at: string;
}

export interface LocalPluginAutoUpdateRecord {
  plugin_id: string;
  target_release_id?: string | null;
  last_checked_at?: string | null;
  last_attempted_at?: string | null;
  last_succeeded_at?: string | null;
  next_retry_at?: string | null;
  consecutive_failures: number;
  last_error?: string | null;
}

export interface LocalPluginAutoUpdateReport {
  schema_version: number;
  catalog_items: number;
  eligible: number;
  attempted: number;
  updated: number;
  deferred: number;
  busy: number;
  failures: number;
  skipped_reason?: string | null;
  errors: string[];
}

export interface LocalPluginOAuthConnection {
  id: string;
  owner_user_id: string;
  device_id: string;
  plugin_id: string;
  release_id: string;
  component_key: string;
  provider: string;
  resource: string;
  scopes: string[];
  connected: boolean;
  needs_auth: boolean;
  expires_at?: string | null;
  account_display?: string | null;
  updated_at: string;
}

export interface PluginOAuthAuthorizationStart {
  transaction_id: string;
  authorization_url: string;
  expires_at: string;
  browser_opened: boolean;
  browser_error?: string | null;
}

export interface UpdateLocalRuntimeSettingsPayload extends Partial<LocalRuntimeSettings> {
  acknowledge_browser_full_cdp_risk?: boolean;
}

export interface ChromeBridgeStatus {
  connected: boolean;
  extension_id: string;
  extension_version?: string | null;
  extension_compatible: boolean;
  connected_at_ms?: number | null;
  last_seen_at_ms?: number | null;
  claimed_tab_count: number;
  authorized_origin_count: number;
  pending_command_count: number;
}

export interface ChromeIntegrationStatus {
  platform_supported: boolean;
  enabled: boolean;
  native_host_available: boolean;
  native_host_manifest_path?: string | null;
  extension_available: boolean;
  extension_directory?: string | null;
  extension_id: string;
  bridge: ChromeBridgeStatus;
  setup_note: string;
  last_error?: string | null;
}

export interface AgentPromptUpdateStatus {
  configured: boolean;
  initialized: boolean;
  source_instance_id?: string | null;
  installed_bundle_version: number;
  remote_bundle_version: number;
  update_available: boolean;
  required: boolean;
  prompt_count: number;
  expected_prompt_count: number;
  capability_count: number;
  expected_capability_count: number;
  last_checked_at?: string | null;
  last_synced_at?: string | null;
  last_error?: string | null;
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

export type SandboxBackendKind = 'local_process';
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
  status: string;
  tools: string[];
  created_at: string;
  updated_at: string;
  expires_at: string;
  destroyed_at?: string | null;
  last_error?: string | null;
  effective_policy: SandboxEffectivePolicy;
  effective_permissions: EffectivePermissionSnapshot;
}

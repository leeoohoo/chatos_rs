// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type Visibility = 'private' | 'public' | 'system_private';
export type RuntimeKind =
  | 'builtin'
  | 'system_routed'
  | 'http'
  | 'stdio_cloud'
  | 'local_connector_stdio'
  | 'local_connector_http'
  | 'local_connector_builtin_proxy';
export type ResourceKind = 'mcp' | 'skill' | 'skill_package';
export type BindingScope = 'global_default' | 'user_override' | 'system_required';
export type McpBindingMode = 'disabled' | 'optional' | 'required';
export type AgentPromptVendor = 'glm' | 'deepseek' | 'gpt' | 'kimi';

export interface CurrentUser {
  principal_type: string;
  user_id: string;
  username: string;
  display_name: string;
  role: string;
  owner_user_id?: string | null;
}

export interface LoginPayload {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: CurrentUser;
}

export interface ListResponse<T> {
  items: T[];
  total: number;
}

export interface LocalConnectorRef {
  device_id?: string | null;
  workspace_id?: string | null;
  manifest_id?: string | null;
  relative_path?: string | null;
  requires_online?: boolean;
}

export interface McpRuntime {
  kind: RuntimeKind;
  builtin_kind?: string | null;
  server_name?: string | null;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string | null;
  url?: string | null;
  headers?: Record<string, string>;
  local_connector?: LocalConnectorRef | null;
}

export interface ResourceSecurity {
  allow_writes?: boolean | null;
  max_file_bytes?: number | null;
  max_write_bytes?: number | null;
  search_limit?: number | null;
  allowed_tool_names?: string[];
  blocked_tool_names?: string[];
}

export interface ResourceMetadata {
  tags?: string[];
  version?: string | null;
  homepage?: string | null;
  category?: string | null;
  argument_hint?: string | null;
  extra?: Record<string, unknown>;
}

export interface McpRecord {
  id: string;
  owner_user_id: string;
  owner_kind: string;
  visibility: Visibility;
  source_kind: string;
  name: string;
  display_name: string;
  description?: string | null;
  enabled: boolean;
  runtime: McpRuntime;
  security: ResourceSecurity;
  metadata: ResourceMetadata;
  created_by: string;
  updated_by: string;
  created_at: string;
  updated_at: string;
}

export interface McpProviderSkill {
  id: string;
  name: string;
  description: string;
  instructions: string;
  locale?: string | null;
}

export interface AdminAiModelConfig {
  id: string;
  name: string;
  provider: string;
  model: string;
  model_name?: string;
  enabled: boolean;
  has_api_key: boolean;
  supports_responses: boolean;
}

export interface OptimizeProviderSkillResponse {
  mcp_id: string;
  skill_id: string;
  model_config_id: string;
  provider: string;
  model: string;
  optimized_instructions: string;
}

export type OptimizeProviderSkillStreamEvent =
  | { type: 'started'; provider: string; model: string }
  | { type: 'thinking'; delta: string }
  | { type: 'chunk'; delta: string }
  | { type: 'done'; optimized_instructions: string }
  | { type: 'error'; message: string };

export interface McpToolDescriptor extends Record<string, unknown> {
  name?: string;
  description?: string;
  inputSchema?: unknown;
  outputSchema?: unknown;
  input_schema?: unknown;
  output_schema?: unknown;
}

export interface McpDescriptorResponse {
  mcp_id: string;
  server_name: string;
  provider_skills: McpProviderSkill[];
  tools: McpToolDescriptor[];
  tools_status: 'ready' | 'degraded' | 'unavailable' | 'not_declared' | string;
  tools_error?: string | null;
}

export interface SkillContent {
  kind:
    | 'inline_content'
    | 'cloud_package'
    | 'git_package'
    | 'local_connector_file'
    | 'local_connector_package';
  inline?: string | null;
  package_id?: string | null;
  source_path?: string | null;
  repository?: string | null;
  branch?: string | null;
  local_connector?: LocalConnectorRef | null;
}

export interface SkillRecord {
  id: string;
  owner_user_id: string;
  owner_kind: string;
  visibility: Visibility;
  source_kind: string;
  name: string;
  display_name: string;
  description?: string | null;
  enabled: boolean;
  content: SkillContent;
  metadata: ResourceMetadata;
  created_by: string;
  updated_by: string;
  created_at: string;
  updated_at: string;
}

export interface SkillPackageRecord {
  id: string;
  owner_user_id: string;
  visibility: Visibility;
  source_kind: string;
  name: string;
  description?: string | null;
  repository?: string | null;
  branch?: string | null;
  cache_ref?: string | null;
  local_connector?: LocalConnectorRef | null;
  skill_ids: string[];
  installed: boolean;
  created_at: string;
  updated_at: string;
}

export interface SystemAgentRecord {
  id: string;
  agent_key: string;
  display_name: string;
  service_name: string;
  scope: string;
  description?: string | null;
  enabled: boolean;
  managed_by: string;
  include_user_resources: boolean;
  created_at: string;
  updated_at: string;
}

export interface AgentProviderPromptRecord {
  id: string;
  agent_key: string;
  vendor: AgentPromptVendor;
  draft_content?: string | null;
  published_content?: string | null;
  published_revision: number;
  published_checksum?: string | null;
  enabled: boolean;
  source_kind: string;
  generated_by_model_config_id?: string | null;
  created_by: string;
  updated_by: string;
  published_by?: string | null;
  created_at: string;
  updated_at: string;
  published_at?: string | null;
}

export interface AgentPromptVersionPrompt {
  vendor: AgentPromptVendor;
  content: string;
  revision: number;
  checksum: string;
  published_at: string;
}

export interface AgentPromptVersionRecord {
  id: string;
  agent_key: string;
  bundle_version: number;
  changed_vendor?: AgentPromptVendor | null;
  prompts: AgentPromptVersionPrompt[];
  published_by: string;
  published_at: string;
}

export interface AgentPromptVersionVendorSummary {
  vendor: AgentPromptVendor;
  revision: number;
  checksum: string;
}

export interface AgentPromptVersionSummary {
  id: string;
  agent_key: string;
  bundle_version: number;
  changed_vendor?: AgentPromptVendor | null;
  vendor_revisions: AgentPromptVersionVendorSummary[];
  published_by: string;
  published_at: string;
}

export interface GenerateAgentPromptResponse {
  agent_key: string;
  vendor: AgentPromptVendor;
  model_config_id: string;
  provider: string;
  model: string;
  content: string;
}

export interface AgentPromptCompleteness {
  agent_key: string;
  required_vendors: AgentPromptVendor[];
  published_vendors: AgentPromptVendor[];
  missing_vendors: AgentPromptVendor[];
  ready: boolean;
}

export interface BindingConditions {
  task_profile?: string | null;
  project_source_type?: string | null;
  runtime_provider?: string | null;
  schedule_mode?: string | null;
}

export interface AgentBindingRecord {
  id: string;
  agent_key: string;
  binding_scope: BindingScope;
  owner_user_id?: string | null;
  resource_kind: ResourceKind;
  resource_id: string;
  enabled: boolean;
  required: boolean;
  priority: number;
  conditions: BindingConditions;
  created_by: string;
  updated_by: string;
  created_at: string;
  updated_at: string;
}

export interface AgentMcpBindingView {
  mcp: McpRecord;
  mode: McpBindingMode;
}

export interface AgentMcpBindingsResponse {
  agent: SystemAgentRecord;
  items: AgentMcpBindingView[];
}

export interface ResourceCheckRecord {
  id: string;
  resource_kind: ResourceKind;
  resource_id: string;
  owner_user_id: string;
  status: string;
  last_checked_at: string;
  last_error?: string | null;
  tool_snapshot: unknown[];
  manifest_hash?: string | null;
}

export interface ResolvedMcp {
  resource: McpRecord;
  binding: AgentBindingRecord;
  available: boolean;
  status: string;
  reason?: string | null;
}

export interface ResolvedSkill {
  resource: SkillRecord;
  binding: AgentBindingRecord;
  available: boolean;
  status: string;
  reason?: string | null;
}

export interface LocalConnectorRequirement {
  resource_kind: ResourceKind;
  resource_id: string;
  device_id?: string | null;
  workspace_id?: string | null;
  required: boolean;
  available: boolean;
  reason?: string | null;
}

export interface RuntimeCapabilitiesResponse {
  agent_key: string;
  owner_user_id: string;
  mcps: ResolvedMcp[];
  skills: ResolvedSkill[];
  local_connector_requirements: LocalConnectorRequirement[];
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type UserRole = 'super_admin' | 'user';
export type PrincipalType = 'human_user' | 'agent_account';

export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  principal_type: PrincipalType;
}

export interface LoginPayload {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

export interface CurrentUserResponse {
  user: AuthUser;
}

export interface UserSummaryRecord {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string | null;
  agent_count: number;
  harness_provisioning?: HarnessProvisioningSummaryRecord | null;
}

export interface InviteCodeRecord {
  id: string;
  label?: string | null;
  created_by_user_id: string;
  max_uses: number;
  used_count: number;
  expires_at_unix?: number | null;
  revoked_at?: string | null;
  last_used_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateInviteCodePayload {
  label?: string;
  max_uses?: number;
  expires_in_days?: number;
}

export interface CreateInviteCodeResponse {
  code: string;
  invite: InviteCodeRecord;
}

export interface HarnessProvisioningSummaryRecord {
  status: 'pending' | 'provisioned' | 'failed' | string;
  harness_uid: string;
  harness_email: string;
  space_identifier: string;
  attempts: number;
  last_error?: string | null;
  last_attempt_at?: string | null;
  provisioned_at?: string | null;
  updated_at: string;
}

export interface CreateUserPayload {
  username: string;
  display_name?: string;
  password: string;
  role?: UserRole;
  enabled?: boolean;
}

export interface UpdateUserPayload {
  display_name?: string;
  password?: string;
  role?: UserRole;
  enabled?: boolean;
}

export interface ProvisionHarnessPayload {
  password: string;
}

export interface AgentAccountListItem {
  id: string;
  username: string;
  display_name: string;
  owner_user_id: string;
  owner_username: string;
  owner_display_name: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_login_at?: string | null;
}

export interface CreateAgentAccountPayload {
  username: string;
  display_name?: string;
  password: string;
  owner_user_id?: string;
  enabled?: boolean;
}

export interface UpdateAgentAccountPayload {
  display_name?: string;
  owner_user_id?: string;
  enabled?: boolean;
}

export interface ResetAgentPasswordPayload {
  password: string;
}

export interface UserModelConfigRecord {
  id: string;
  owner_user_id: string;
  name: string;
  provider: string;
  prompt_vendor?: AgentPromptVendor | null;
  model: string;
  model_name: string;
  thinking_level?: string | null;
  api_key?: string;
  has_api_key: boolean;
  base_url?: string | null;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  created_at: string;
  updated_at: string;
  sync_warnings?: string[];
}

export interface UserModelProviderRecord {
  id: string;
  owner_user_id: string;
  name: string;
  provider: string;
  prompt_vendor?: AgentPromptVendor | null;
  api_key?: string;
  has_api_key: boolean;
  base_url?: string | null;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  last_sync_status?: string | null;
  last_sync_error?: string | null;
  last_synced_at?: string | null;
  imported_model_count: number;
  created_at: string;
  updated_at: string;
  sync_warnings?: string[];
}

export interface CreateUserModelConfigPayload {
  id?: string;
  owner_user_id?: string;
  name: string;
  provider?: string;
  prompt_vendor?: AgentPromptVendor;
  model?: string;
  thinking_level?: string;
  api_key?: string;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface CreateUserModelProviderPayload {
  id?: string;
  owner_user_id?: string;
  name: string;
  provider?: string;
  prompt_vendor?: AgentPromptVendor;
  api_key?: string;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface UpdateUserModelConfigPayload {
  name?: string;
  provider?: string;
  prompt_vendor?: AgentPromptVendor;
  model?: string;
  thinking_level?: string;
  api_key?: string;
  clear_api_key?: boolean;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface UpdateUserModelProviderPayload {
  name?: string;
  provider?: string;
  prompt_vendor?: AgentPromptVendor;
  api_key?: string;
  clear_api_key?: boolean;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export type AgentPromptVendor = 'glm' | 'deepseek' | 'gpt' | 'kimi';

export interface UserModelSettingsRecord {
  user_id: string;
  memory_summary_model_config_id?: string | null;
  memory_summary_thinking_level?: string | null;
  project_management_agent_model_config_id?: string | null;
  project_management_agent_thinking_level?: string | null;
  updated_at: string;
  sync_warnings?: string[];
}

export interface UpdateUserModelSettingsPayload {
  user_id?: string;
  memory_summary_model_config_id?: string | null;
  memory_summary_thinking_level?: string | null;
  project_management_agent_model_config_id?: string | null;
  project_management_agent_thinking_level?: string | null;
}

export interface HealthResponse {
  status: string;
  service: string;
  now: string;
}

export interface SystemConfigResponse {
  service: string;
  issuer: string;
  user_service_audience: string;
  task_runner_audience: string;
  database_url: string;
  user_access_ttl_seconds: number;
  task_runner_access_ttl_seconds: number;
}

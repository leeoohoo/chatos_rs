// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type * from './apiTypes';

import { request } from './apiTransport';
import type {
  AgentPromptUpdateStatus,
  ApprovalSettings,
  CommandExecutionApprovalDecision,
  CommandHistoryResponse,
  ConnectorStatus,
  DockerStatus,
  FsListResponse,
  LocalMcpConfig,
  LocalMcpConfigDraft,
  LocalModelCatalogResponse,
  LocalModelConfig,
  LocalModelConfigDraft,
  LocalModelConfigListResponse,
  LocalModelSettings,
  LocalRuntimeSettings,
  LocalPluginStatusSnapshot,
  LocalPluginStatusEvent,
  LocalPluginStoreSnapshot,
  LocalPluginAutoUpdateReport,
  LocalPluginOAuthConnection,
  PluginOAuthAuthorizationStart,
  UserPluginPreferenceRecord,
  ChromeIntegrationStatus,
  UpdateLocalRuntimeSettingsPayload,
  LocalSkillCatalogItem,
  LocalSkillCatalogResponse,
  LocalSkillInstallation,
  PendingApprovalsResponse,
  SandboxCapabilities,
  SandboxImageCatalog,
  SandboxImageJob,
  SandboxLease,
  SandboxSettings,
  SandboxSettingsUpdate,
  SystemPermissionsResponse,
  TerminalExecResponse,
} from './apiTypes';

export const api = {
  status: () => request<ConnectorStatus>('/api/local/status'),
  login: (payload: {
    cloud_base_url: string;
    user_service_base_url: string;
    username: string;
    password: string;
    device_name?: string;
  }) =>
    request<ConnectorStatus>('/api/local/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  register: (payload: {
    cloud_base_url: string;
    user_service_base_url: string;
    username: string;
    display_name?: string;
    password: string;
    device_name?: string;
    invite_code?: string;
    verification_code?: string;
  }) =>
    request<ConnectorStatus>('/api/local/auth/register', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  sendRegisterEmailCode: (payload: { cloud_base_url: string; email: string; invite_code: string }) =>
    request<{ ok: boolean; expires_in_seconds?: number; resend_after_seconds?: number }>(
      '/api/local/auth/register/send-code',
      {
        method: 'POST',
        body: JSON.stringify(payload),
      },
    ),
  logout: () =>
    request<ConnectorStatus>('/api/local/auth/logout', {
      method: 'POST',
    }),
  fsList: (path?: string | null) => {
    const query = path ? `?path=${encodeURIComponent(path)}` : '';
    return request<FsListResponse>(`/api/local/fs/list${query}`);
  },
  addWorkspace: (payload: { path: string; alias?: string }) =>
    request<ConnectorStatus>('/api/local/workspaces', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  removeWorkspace: (workspaceId: string) =>
    request<ConnectorStatus>(`/api/local/workspaces/${encodeURIComponent(workspaceId)}`, {
      method: 'DELETE',
    }),
  setWorkspaceProjectConfigTrust: (
    workspaceId: string,
    payload: { trusted: boolean; risk_acknowledged?: boolean },
  ) =>
    request<ConnectorStatus>(
      `/api/local/workspaces/${encodeURIComponent(workspaceId)}/project-config-trust`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      },
    ),
  dockerStatus: () => request<DockerStatus>('/api/local/docker/status'),
  setSandboxEnabled: (payload: { enabled: boolean }) =>
    request<ConnectorStatus>('/api/local/sandbox/toggle', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  sandboxCapabilities: () => request<SandboxCapabilities>('/api/local/sandbox/capabilities'),
  sandboxSettings: () => request<SandboxSettings>('/api/local/sandbox/settings'),
  updateSandboxSettings: (payload: SandboxSettingsUpdate) =>
    request<SandboxSettings>('/api/local/sandbox/settings', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  sandboxImages: () => request<SandboxImageCatalog>('/api/local/sandbox/images'),
  sandboxImageJobs: () => request<SandboxImageJob[]>('/api/local/sandbox/images/jobs'),
  sandboxLeases: () => request<SandboxLease[]>('/api/local/sandbox/leases'),
  initializeSandboxImage: (payload: { features: string[]; custom_build_script?: string }) =>
    request<SandboxImageJob>('/api/local/sandbox/images/initialize', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  deleteSandboxImage: (imageId: string) =>
    request<{ ok: boolean; image_id: string; image_ref: string }>(
      `/api/local/sandbox/images/${encodeURIComponent(imageId)}`,
      { method: 'DELETE' },
    ),
  reinitializeSandboxImage: (imageId: string) =>
    request<SandboxImageJob>(
      `/api/local/sandbox/images/${encodeURIComponent(imageId)}/reinitialize`,
      { method: 'POST' },
    ),
  terminalExec: (payload: {
    workspace_id: string;
    command: string;
    args?: string[];
    cwd?: string;
    timeout_ms?: number;
  }) =>
    request<TerminalExecResponse>('/api/local/terminal/exec', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  commandHistory: (payload: { limit?: number; source?: string } = {}) => {
    const query = new URLSearchParams();
    if (payload.limit) {
      query.set('limit', String(payload.limit));
    }
    if (payload.source) {
      query.set('source', payload.source);
    }
    const suffix = query.toString() ? `?${query.toString()}` : '';
    return request<CommandHistoryResponse>(`/api/local/commands${suffix}`);
  },
  clearCommandHistory: () =>
    request<CommandHistoryResponse>('/api/local/commands', {
      method: 'DELETE',
    }),
  runtimeSettings: async () => {
    try {
      return await request<LocalRuntimeSettings>('/api/local/runtime-settings');
    } catch (error) {
      const fallback = window.chatosLocalConnector?.runtimeSettings;
      if (fallback && isCoreBridgeUnavailable(error)) {
        return fallback();
      }
      throw error;
    }
  },
  updateRuntimeSettings: async (payload: UpdateLocalRuntimeSettingsPayload) => {
    try {
      return await request<LocalRuntimeSettings>('/api/local/runtime-settings', {
        method: 'POST',
        body: JSON.stringify(payload),
      });
    } catch (error) {
      const fallback = window.chatosLocalConnector?.updateRuntimeSettings;
      if (fallback && isCoreBridgeUnavailable(error)) {
        return fallback(payload);
      }
      throw error;
    }
  },
  chromeIntegration: () => request<ChromeIntegrationStatus>('/api/local/chrome-integration'),
  enableChromeIntegration: () => request<ChromeIntegrationStatus>(
    '/api/local/chrome-integration/enable',
    {
      method: 'POST',
      body: JSON.stringify({ acknowledge_sensitive_browser_access: true }),
    },
  ),
  disableChromeIntegration: () => request<ChromeIntegrationStatus>(
    '/api/local/chrome-integration/disable',
    { method: 'POST' },
  ),
  agentPromptStatus: () =>
    request<AgentPromptUpdateStatus>('/api/local/agent-prompts/status'),
  checkAgentPromptUpdates: () =>
    request<AgentPromptUpdateStatus>('/api/local/agent-prompts/check', { method: 'POST' }),
  updateAgentPrompts: () =>
    request<AgentPromptUpdateStatus>('/api/local/agent-prompts/update', { method: 'POST' }),
  systemPermissions: () => request<SystemPermissionsResponse>('/api/local/system-permissions'),
  requestSystemPermission: (permissionId: string) =>
    request<SystemPermissionsResponse>(
      `/api/local/system-permissions/${encodeURIComponent(permissionId)}/request`,
      {
        method: 'POST',
      },
    ),
  approvalSettings: () => request<ApprovalSettings>('/api/local/approval/settings'),
  updateApprovalSettings: (
    payload: Partial<Pick<ApprovalSettings, 'default_mode' | 'projects'>> & {
      risk_acknowledged?: boolean;
    },
  ) =>
    request<ApprovalSettings>('/api/local/approval/settings', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  pendingApprovals: () => request<PendingApprovalsResponse>('/api/local/approval/pending'),
  approvePendingApproval: (
    id: string,
    payload: {
      remember_allow?: boolean;
      decision?: CommandExecutionApprovalDecision;
      risk_acknowledged?: boolean;
      confirmation_response?: string;
    } = {},
  ) =>
    request<{ ok: boolean }>(`/api/local/approval/pending/${encodeURIComponent(id)}/approve`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  denyPendingApproval: (id: string, payload: { reason?: string } = {}) =>
    request<{ ok: boolean }>(`/api/local/approval/pending/${encodeURIComponent(id)}/deny`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  modelConfigs: () => request<LocalModelConfigListResponse>('/api/local/model-configs'),
  previewModelCatalog: (draft: LocalModelConfigDraft) =>
    request<LocalModelCatalogResponse>('/api/local/model-configs/catalog/preview', {
      method: 'POST',
      body: JSON.stringify(draft),
    }),
  saveModelConfig: (draft: LocalModelConfigDraft, sync = true) =>
    request<LocalModelConfig>('/api/local/model-configs', {
      method: 'POST',
      body: JSON.stringify({ ...draft, sync }),
    }),
  updateModelConfig: (id: string, draft: LocalModelConfigDraft, sync = true) =>
    request<LocalModelConfig>(`/api/local/model-configs/${encodeURIComponent(id)}`, {
      method: 'POST',
      body: JSON.stringify({ ...draft, sync }),
    }),
  deleteModelConfig: (id: string) =>
    request<{ ok: boolean }>(`/api/local/model-configs/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    }),
  syncModelConfig: (id: string) =>
    request<LocalModelConfig>(`/api/local/model-configs/${encodeURIComponent(id)}/sync`, {
      method: 'POST',
    }),
  saveModelSettings: (payload: LocalModelSettings, sync = false) =>
    request<LocalModelSettings>('/api/local/model-settings', {
      method: 'POST',
      body: JSON.stringify({ ...payload, sync }),
    }),
  mcpConfigs: () => request<LocalMcpConfig[]>('/api/local/mcp-configs'),
  plugins: () => request<LocalPluginStoreSnapshot>('/api/local/plugins/catalog'),
  pluginStatus: () => request<LocalPluginStatusSnapshot>('/api/local/plugins'),
  pluginEvents: (cursor?: string) => {
    const query = new URLSearchParams({ timeout_ms: '25000' });
    if (cursor) query.set('cursor', cursor);
    return request<LocalPluginStatusEvent>(`/api/local/plugins/events?${query.toString()}`);
  },
  recoverPlugins: () => request<{
    completed_transactions: number;
    rolled_back_transactions: number;
    cleaned_paths: number;
    errors: string[];
  }>('/api/local/plugins/recover', { method: 'POST' }),
  checkPluginUpdates: () => request<LocalPluginAutoUpdateReport>(
    '/api/local/plugins/check-updates',
    { method: 'POST' },
  ),
  updatePluginPreference: (
    pluginId: string,
    payload: {
      enabled: boolean;
      auto_update?: boolean;
      release_channel?: string;
      enabled_components?: string[];
    },
  ) => request<UserPluginPreferenceRecord>(
    `/api/local/plugins/${encodeURIComponent(pluginId)}/preference`,
    { method: 'PUT', body: JSON.stringify(payload) },
  ),
  rollbackPlugin: (pluginId: string) =>
    request<LocalPluginStatusSnapshot>(
      `/api/local/plugins/${encodeURIComponent(pluginId)}/rollback`,
      { method: 'POST' },
    ),
  installPlugin: (pluginId: string) =>
    request<LocalPluginStatusSnapshot>(
      `/api/local/plugins/${encodeURIComponent(pluginId)}/install`,
      { method: 'POST' },
    ),
  uninstallPlugin: (pluginId: string) =>
    request<LocalPluginStatusSnapshot>(`/api/local/plugins/${encodeURIComponent(pluginId)}`, {
      method: 'DELETE',
      body: JSON.stringify({ acknowledge_plugin_data_removal: true }),
    }),
  pluginOAuthConnections: (pluginId: string) =>
    request<LocalPluginOAuthConnection[]>(
      `/api/local/plugins/${encodeURIComponent(pluginId)}/oauth`,
    ),
  beginPluginOAuth: (pluginId: string, componentKey: string) =>
    request<PluginOAuthAuthorizationStart>(
      `/api/local/plugins/${encodeURIComponent(pluginId)}/components/${encodeURIComponent(componentKey)}/oauth/start`,
      {
        method: 'POST',
        body: JSON.stringify({ open_browser: true }),
      },
    ),
  disconnectPluginOAuth: (pluginId: string, componentKey: string, provider: string) =>
    request<{ disconnected: boolean }>(
      `/api/local/plugins/${encodeURIComponent(pluginId)}/components/${encodeURIComponent(componentKey)}/oauth/${encodeURIComponent(provider)}`,
      { method: 'DELETE' },
    ),
  skills: () => request<LocalSkillCatalogResponse>('/api/local/skills'),
  syncSkills: () => request<LocalSkillInstallation[]>('/api/local/skills/sync', { method: 'POST' }),
  setSkillEnabled: (skillId: string, enabled: boolean) =>
    request<LocalSkillCatalogItem>(
      `/api/local/skills/${encodeURIComponent(skillId)}/preference`,
      {
        method: 'POST',
        body: JSON.stringify({ enabled }),
      },
    ),
  saveMcpConfig: (draft: LocalMcpConfigDraft) =>
    request<LocalMcpConfig>('/api/local/mcp-configs', {
      method: 'POST',
      body: JSON.stringify(draft),
    }),
  updateMcpConfig: (manifestId: string, draft: LocalMcpConfigDraft) =>
    request<LocalMcpConfig>(`/api/local/mcp-configs/${encodeURIComponent(manifestId)}`, {
      method: 'POST',
      body: JSON.stringify(draft),
    }),
  testMcpConfig: (manifestId: string) =>
    request<LocalMcpConfig>(`/api/local/mcp-configs/${encodeURIComponent(manifestId)}/test`, {
      method: 'POST',
    }),
  setMcpConfigEnabled: (manifestId: string, enabled: boolean) =>
    request<LocalMcpConfig>(
      `/api/local/mcp-configs/${encodeURIComponent(manifestId)}/${enabled ? 'enable' : 'disable'}`,
      { method: 'POST' },
    ),
  syncMcpConfig: (manifestId: string) =>
    request<LocalMcpConfig>(`/api/local/mcp-configs/${encodeURIComponent(manifestId)}/sync`, {
      method: 'POST',
    }),
  deleteMcpConfig: (manifestId: string) =>
    request<{ ok: boolean }>(`/api/local/mcp-configs/${encodeURIComponent(manifestId)}`, {
      method: 'DELETE',
    }),
};

function isCoreBridgeUnavailable(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }
  return /ENOENT|ECONNREFUSED|ECONNRESET|socket|pipe|Local Connector Core/i.test(error.message);
}

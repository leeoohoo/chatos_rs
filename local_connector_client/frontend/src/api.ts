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

async function request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
  const headers = new Headers(options.headers || {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const response = await fetch(endpoint, {
    ...options,
    headers,
  });
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;
  if (!response.ok) {
    const message =
      typeof body?.error === 'string'
        ? body.error
        : typeof body?.message === 'string'
          ? body.message
          : `HTTP ${response.status}`;
    throw new Error(message);
  }
  return body as T;
}

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
  }) =>
    request<ConnectorStatus>('/api/local/auth/register', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
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
  dockerStatus: () => request<DockerStatus>('/api/local/docker/status'),
  setSandboxEnabled: (payload: { enabled: boolean }) =>
    request<ConnectorStatus>('/api/local/sandbox/toggle', {
      method: 'POST',
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
};

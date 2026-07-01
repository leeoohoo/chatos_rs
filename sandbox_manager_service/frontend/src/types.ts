export type SandboxStatus =
  | 'pending'
  | 'leasing'
  | 'starting'
  | 'ready'
  | 'running'
  | 'releasing'
  | 'destroying'
  | 'destroyed'
  | 'failed'
  | 'expired';

export interface ResourceLimits {
  cpu: number;
  memory_mb: number;
  disk_mb: number;
  max_processes: number;
}

export interface NetworkPolicy {
  mode: string;
}

export interface SandboxLeaseRecord {
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
  status: SandboxStatus;
  agent_endpoint?: string | null;
  resource_limits: ResourceLimits;
  network: NetworkPolicy;
  tools: string[];
  created_at: string;
  updated_at: string;
  expires_at: string;
  destroyed_at?: string | null;
  last_error?: string | null;
}

export interface SandboxEventRecord {
  id: string;
  sandbox_id: string;
  lease_id: string;
  event_type: string;
  message?: string | null;
  payload?: unknown;
  created_at: string;
}

export interface SandboxHealthCheck {
  name: string;
  ok: boolean;
  message: string;
}

export interface SandboxHealthResponse {
  ok: boolean;
  sandbox_id: string;
  lease_id: string;
  status: SandboxStatus;
  backend: string;
  backend_id?: string | null;
  backend_alive: boolean;
  agent_endpoint?: string | null;
  agent_alive?: boolean | null;
  workspace_alive: boolean;
  checked_at: string;
  message: string;
  checks: SandboxHealthCheck[];
}

export interface SandboxMcpToolsResponse {
  ok: boolean;
  sandbox_id: string;
  agent_endpoint: string;
  tools: unknown[];
}

export interface SandboxMcpCallPayload {
  name: string;
  arguments: unknown;
}

export interface SandboxMcpCallResponse {
  ok: boolean;
  sandbox_id: string;
  agent_endpoint: string;
  result: unknown;
}

export interface CreateSandboxLeasePayload {
  tenant_id: string;
  user_id: string;
  project_id: string;
  run_id: string;
  workspace_root: string;
  tools: string[];
  ttl_seconds?: number;
  resource_limits?: ResourceLimits;
  network?: NetworkPolicy;
}

export interface CreateSandboxLeaseResponse {
  lease_id: string;
  sandbox_id: string;
  backend_id?: string | null;
  status: SandboxStatus;
  agent_endpoint?: string | null;
  run_workspace: string;
  expires_at: string;
}

export interface PoolStatusResponse {
  backend: string;
  max_active: number;
  active: number;
  max_pending: number;
  pending: number;
  lease_ttl_seconds: number;
  cleanup_interval_seconds: number;
}

export interface SystemConfigResponse {
  host: string;
  port: number;
  backend: string;
  work_root: string;
  pool_max_active: number;
  pool_max_pending: number;
  lease_ttl_seconds: number;
  cleanup_interval_seconds: number;
  agent_port: number;
  docker_image: string;
  docker_network_mode: string;
  kata_container_cli: string;
  kata_runtime: string;
  kata_image: string;
  kata_network_mode: string;
}

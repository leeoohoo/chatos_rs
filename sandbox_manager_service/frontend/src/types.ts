// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  image_id?: string | null;
  image_ref?: string | null;
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
  image_id?: string;
  tools: string[];
  ttl_seconds?: number;
  resource_limits?: ResourceLimits;
  network?: NetworkPolicy;
}

export interface CreateSandboxLeaseResponse {
  lease_id: string;
  sandbox_id: string;
  backend_id?: string | null;
  image_id?: string | null;
  image_ref?: string | null;
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

export interface UpdatePoolConfigPayload {
  max_active?: number;
  max_pending?: number;
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
  image_tag_prefix: string;
  image_build_context: string;
  image_dockerfile: string;
}

export interface SandboxAccessClient {
  id: string;
  name: string;
  client_id: string;
  enabled: boolean;
  scopes: string[];
  allowed_tenant_ids: string[];
  allowed_project_ids: string[];
  allowed_tools: string[];
  max_lease_ttl_seconds: number;
  created_at: string;
  updated_at: string;
  last_used_at?: string | null;
}

export interface SandboxAccessClientPayload {
  name: string;
  client_id?: string | null;
  scopes: string[];
  allowed_tenant_ids: string[];
  allowed_project_ids: string[];
  allowed_tools: string[];
  max_lease_ttl_seconds?: number | null;
}

export interface SandboxAccessClientUpdatePayload {
  name?: string;
  enabled?: boolean;
  scopes?: string[];
  allowed_tenant_ids?: string[];
  allowed_project_ids?: string[];
  allowed_tools?: string[];
  max_lease_ttl_seconds?: number;
}

export interface SandboxAccessClientSecretResponse {
  client: SandboxAccessClient;
  client_key: string;
}

export interface SandboxImageRuntimeVersionRecord {
  id: string;
  label: string;
  description: string;
  default: boolean;
}

export interface SandboxImageFeatureRecord {
  id: string;
  label: string;
  description: string;
  default_version: string;
  versions: SandboxImageRuntimeVersionRecord[];
}

export interface SandboxImageRecord {
  id: string;
  name: string;
  description: string;
  image_ref: string;
  features: string[];
  backend: string;
  initialized: boolean;
  status: string;
  buildable: boolean;
  is_default: boolean;
}

export interface SandboxImageCatalogResponse {
  backend: string;
  default_image_id: string;
  image_tag_prefix: string;
  features: SandboxImageFeatureRecord[];
  images: SandboxImageRecord[];
}

export interface InitializeSandboxImagePayload {
  features: string[];
  custom_build_script?: string;
}

export interface SandboxImageJobRecord {
  id: string;
  image_id: string;
  image_name: string;
  image_ref: string;
  features: string[];
  backend: string;
  status: 'running' | 'succeeded' | 'failed' | string;
  created_at: string;
  updated_at: string;
  started_at?: string | null;
  finished_at?: string | null;
  output: string;
  error?: string | null;
}

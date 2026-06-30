import type {
  CreateSandboxLeasePayload,
  CreateSandboxLeaseResponse,
  PoolStatusResponse,
  SandboxEventRecord,
  SandboxHealthResponse,
  SandboxLeaseRecord,
  SandboxMcpCallPayload,
  SandboxMcpCallResponse,
  SandboxMcpToolsResponse,
} from '../types';
import { request, withQuery } from './client';

export interface SandboxListFilters {
  tenant_id?: string;
  user_id?: string;
  project_id?: string;
  run_id?: string;
  status?: string;
}

export const sandboxesApi = {
  list: (filters?: SandboxListFilters) =>
    request<SandboxLeaseRecord[]>(
      withQuery('/api/sandboxes', {
        ...filters,
        limit: 200,
      }),
    ),
  create: (payload: CreateSandboxLeasePayload) =>
    request<CreateSandboxLeaseResponse>('/api/sandboxes/leases', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  get: (sandboxId: string) => request<SandboxLeaseRecord>(`/api/sandboxes/${sandboxId}`),
  events: (sandboxId: string) =>
    request<SandboxEventRecord[]>(`/api/sandboxes/${sandboxId}/events`),
  health: (sandboxId: string) =>
    request<SandboxHealthResponse>(`/api/sandboxes/${sandboxId}/health`),
  mcpTools: (sandboxId: string) =>
    request<SandboxMcpToolsResponse>(`/api/sandboxes/${sandboxId}/mcp/tools`),
  mcpCall: (sandboxId: string, payload: SandboxMcpCallPayload) =>
    request<SandboxMcpCallResponse>(`/api/sandboxes/${sandboxId}/mcp/call`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  release: (sandboxId: string, leaseId: string) =>
    request<{ ok: boolean; status: string }>(`/api/sandboxes/${sandboxId}/release`, {
      method: 'POST',
      body: JSON.stringify({ lease_id: leaseId, export_result: false, destroy: true }),
    }),
  destroy: (sandboxId: string) =>
    request<{ ok: boolean; status: string }>(`/api/sandboxes/${sandboxId}`, {
      method: 'DELETE',
    }),
  poolStatus: () => request<PoolStatusResponse>('/api/sandbox-pool/status'),
};

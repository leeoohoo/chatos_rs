// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CreateSandboxLeasePayload,
  CreateSandboxLeaseResponse,
  InitializeSandboxImagePayload,
  PoolStatusResponse,
  SandboxImageCatalogResponse,
  SandboxImageJobRecord,
  SandboxEventRecord,
  SandboxHealthResponse,
  SandboxLeaseRecord,
  SandboxMcpJsonRpcRequest,
  SandboxMcpJsonRpcResponse,
  UpdatePoolConfigPayload,
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
  images: () => request<SandboxImageCatalogResponse>('/api/sandbox-images'),
  initializeImage: (payload: InitializeSandboxImagePayload) =>
    request<SandboxImageJobRecord>('/api/sandbox-images/initialize', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  imageJobs: () => request<SandboxImageJobRecord[]>('/api/sandbox-images/jobs'),
  get: (sandboxId: string) => request<SandboxLeaseRecord>(`/api/sandboxes/${sandboxId}`),
  events: (sandboxId: string) =>
    request<SandboxEventRecord[]>(`/api/sandboxes/${sandboxId}/events`),
  health: (sandboxId: string) =>
    request<SandboxHealthResponse>(`/api/sandboxes/${sandboxId}/health`),
  mcpProxy: (sandboxId: string, payload: SandboxMcpJsonRpcRequest) =>
    request<SandboxMcpJsonRpcResponse>(`/api/sandboxes/${sandboxId}/mcp`, {
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
  updatePoolConfig: (payload: UpdatePoolConfigPayload) =>
    request<PoolStatusResponse>('/api/sandbox-pool/config', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
};

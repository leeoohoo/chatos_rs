// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  SandboxAccessClient,
  SandboxAccessClientPayload,
  SandboxAccessClientSecretResponse,
  SandboxAccessClientUpdatePayload,
} from '../types';
import { request } from './client';

export const accessClientsApi = {
  list: () => request<SandboxAccessClient[]>('/api/access-clients'),
  create: (payload: SandboxAccessClientPayload) =>
    request<SandboxAccessClientSecretResponse>('/api/access-clients', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  update: (id: string, payload: SandboxAccessClientUpdatePayload) =>
    request<SandboxAccessClient>(`/api/access-clients/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  rotateKey: (id: string) =>
    request<SandboxAccessClientSecretResponse>(`/api/access-clients/${id}/rotate-key`, {
      method: 'POST',
    }),
  remove: (id: string) =>
    request<{ ok: boolean }>(`/api/access-clients/${id}`, {
      method: 'DELETE',
    }),
};

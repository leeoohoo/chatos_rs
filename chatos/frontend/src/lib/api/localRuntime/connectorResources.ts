// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CreateLocalConnectorDirectoryRequest,
  CreateLocalConnectorDirectoryResponse,
  LocalConnectorDeviceResponse,
  LocalConnectorDirectoryListResponse,
  LocalConnectorWorkspaceResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

export const listLocalRuntimeDevices = (): Promise<LocalConnectorDeviceResponse[]> =>
  requestLocalRuntime<LocalConnectorDeviceResponse[]>('/api/local/runtime/devices');

export const listLocalRuntimeWorkspaces = (): Promise<LocalConnectorWorkspaceResponse[]> =>
  requestLocalRuntime<LocalConnectorWorkspaceResponse[]>('/api/local/runtime/workspaces');

export const listLocalRuntimeDirectory = (
  workspaceId: string,
  path?: string,
): Promise<LocalConnectorDirectoryListResponse> => {
  const query = path ? `?path=${encodeURIComponent(path)}` : '';
  return requestLocalRuntime<LocalConnectorDirectoryListResponse>(
    `/api/local/runtime/workspaces/${encodeURIComponent(workspaceId)}/directories${query}`,
  );
};

export const createLocalRuntimeDirectory = (
  data: CreateLocalConnectorDirectoryRequest,
): Promise<CreateLocalConnectorDirectoryResponse> =>
  requestLocalRuntime<CreateLocalConnectorDirectoryResponse>(
    `/api/local/runtime/workspaces/${encodeURIComponent(data.workspace_id)}/directories`,
    {
      method: 'POST',
      body: JSON.stringify({ path: data.path }),
    },
  );

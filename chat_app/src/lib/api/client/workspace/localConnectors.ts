// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildQuery } from '../shared';
import type {
  CreateLocalConnectorDirectoryRequest,
  CreateLocalConnectorDirectoryResponse,
  CreateLocalConnectorProjectRequest,
  LocalConnectorDirectoryListResponse,
  LocalConnectorDeviceResponse,
  LocalConnectorProjectResponse,
  LocalConnectorTerminalExecRequest,
  LocalConnectorTerminalExecResponse,
  LocalConnectorWorkspaceResponse,
} from '../types';
import type { ApiRequestFn } from './common';

export const listLocalConnectorDevices = (
  request: ApiRequestFn,
  userId?: string,
): Promise<LocalConnectorDeviceResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<LocalConnectorDeviceResponse[]>(`/local-connectors/devices${query}`);
};

export const listLocalConnectorWorkspaces = (
  request: ApiRequestFn,
  deviceId?: string,
): Promise<LocalConnectorWorkspaceResponse[]> => {
  const query = buildQuery({ device_id: deviceId });
  return request<LocalConnectorWorkspaceResponse[]>(`/local-connectors/workspaces${query}`);
};

export const listLocalConnectorDirectory = (
  request: ApiRequestFn,
  data: {
    device_id: string;
    workspace_id: string;
    path?: string;
    user_id?: string;
  },
): Promise<LocalConnectorDirectoryListResponse> => {
  const query = buildQuery({
    device_id: data.device_id,
    workspace_id: data.workspace_id,
    path: data.path,
    user_id: data.user_id,
  });
  return request<LocalConnectorDirectoryListResponse>(`/local-connectors/fs/list${query}`);
};

export const createLocalConnectorDirectory = (
  request: ApiRequestFn,
  data: CreateLocalConnectorDirectoryRequest,
): Promise<CreateLocalConnectorDirectoryResponse> => {
  return request<CreateLocalConnectorDirectoryResponse>('/local-connectors/fs/mkdir', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const createLocalConnectorProject = (
  request: ApiRequestFn,
  data: CreateLocalConnectorProjectRequest,
): Promise<LocalConnectorProjectResponse> => {
  return request<LocalConnectorProjectResponse>('/local-connectors/projects', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const execLocalConnectorTerminalCommand = (
  request: ApiRequestFn,
  data: LocalConnectorTerminalExecRequest,
): Promise<LocalConnectorTerminalExecResponse> => {
  return request<LocalConnectorTerminalExecResponse>('/local-connectors/terminal/exec', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

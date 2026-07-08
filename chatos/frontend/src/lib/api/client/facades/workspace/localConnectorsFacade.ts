// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as workspaceApi from '../../workspace';
import type ApiClient from '../../../client';
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
} from '../../types';

export interface WorkspaceLocalConnectorFacade {
  listLocalConnectorDevices(userId?: string): Promise<LocalConnectorDeviceResponse[]>;
  listLocalConnectorWorkspaces(deviceId?: string): Promise<LocalConnectorWorkspaceResponse[]>;
  listLocalConnectorDirectory(data: {
    device_id: string;
    workspace_id: string;
    path?: string;
    user_id?: string;
  }): Promise<LocalConnectorDirectoryListResponse>;
  createLocalConnectorDirectory(data: CreateLocalConnectorDirectoryRequest): Promise<CreateLocalConnectorDirectoryResponse>;
  createLocalConnectorProject(data: CreateLocalConnectorProjectRequest): Promise<LocalConnectorProjectResponse>;
  execLocalConnectorTerminalCommand(data: LocalConnectorTerminalExecRequest): Promise<LocalConnectorTerminalExecResponse>;
}

export const workspaceLocalConnectorFacade: WorkspaceLocalConnectorFacade & ThisType<ApiClient> = {
  async listLocalConnectorDevices(userId) {
    return workspaceApi.listLocalConnectorDevices(this.getRequestFn(), userId);
  },
  async listLocalConnectorWorkspaces(deviceId) {
    return workspaceApi.listLocalConnectorWorkspaces(this.getRequestFn(), deviceId);
  },
  async listLocalConnectorDirectory(data) {
    return workspaceApi.listLocalConnectorDirectory(this.getRequestFn(), data);
  },
  async createLocalConnectorDirectory(data) {
    return workspaceApi.createLocalConnectorDirectory(this.getRequestFn(), data);
  },
  async createLocalConnectorProject(data) {
    return workspaceApi.createLocalConnectorProject(this.getRequestFn(), data);
  },
  async execLocalConnectorTerminalCommand(data) {
    return workspaceApi.execLocalConnectorTerminalCommand(this.getRequestFn(), data);
  },
};

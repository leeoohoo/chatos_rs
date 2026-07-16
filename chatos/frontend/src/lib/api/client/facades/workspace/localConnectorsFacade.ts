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
import { localRuntimeBridgeAvailable } from '../../../localRuntime';

const requireLocalConnectorDesktop = (): void => {
  if (!localRuntimeBridgeAvailable()) {
    throw new Error('Local Connector 功能只能在 Chat OS 桌面客户端中使用');
  }
};

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
    requireLocalConnectorDesktop();
    void userId;
    return this.getLocalRuntimeClient().listConnectorDevices();
  },
  async listLocalConnectorWorkspaces(deviceId) {
    requireLocalConnectorDesktop();
    void deviceId;
    return this.getLocalRuntimeClient().listConnectorWorkspaces();
  },
  async listLocalConnectorDirectory(data) {
    requireLocalConnectorDesktop();
    return this.getLocalRuntimeClient().listConnectorDirectory(data.workspace_id, data.path);
  },
  async createLocalConnectorDirectory(data) {
    requireLocalConnectorDesktop();
    return this.getLocalRuntimeClient().createConnectorDirectory(data);
  },
  async createLocalConnectorProject(data) {
    requireLocalConnectorDesktop();
    return this.getLocalRuntimeClient().createProject(data);
  },
  async execLocalConnectorTerminalCommand(data) {
    requireLocalConnectorDesktop();
    return workspaceApi.execLocalConnectorTerminalCommand(this.getRequestFn(), data);
  },
};

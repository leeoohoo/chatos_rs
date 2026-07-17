// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { workspaceLocalConnectorFacade } from './localConnectorsFacade';

describe('workspaceLocalConnectorFacade desktop routing', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads workspace resources directly from the local runtime', async () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const listConnectorDevices = vi.fn().mockResolvedValue([{ id: 'device-1' }]);
    const listConnectorWorkspaces = vi.fn().mockResolvedValue([{ id: 'workspace-1' }]);
    const listConnectorDirectory = vi.fn().mockResolvedValue({ path: '.', entries: [] });
    const createConnectorDirectory = vi.fn().mockResolvedValue({ path: 'apps', created: true });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud request must not run');
    });
    const context = {
      getLocalRuntimeClient: () => ({
        listConnectorDevices,
        listConnectorWorkspaces,
        listConnectorDirectory,
        createConnectorDirectory,
      }),
      getRequestFn: () => cloudRequest,
    };

    await workspaceLocalConnectorFacade.listLocalConnectorDevices.call(context as never);
    await workspaceLocalConnectorFacade.listLocalConnectorWorkspaces.call(context as never);
    await workspaceLocalConnectorFacade.listLocalConnectorDirectory.call(context as never, {
      device_id: 'device-1',
      workspace_id: 'workspace-1',
      path: 'apps',
    });
    await workspaceLocalConnectorFacade.createLocalConnectorDirectory.call(context as never, {
      device_id: 'device-1',
      workspace_id: 'workspace-1',
      path: 'apps/new',
    });

    expect(listConnectorDirectory).toHaveBeenCalledWith('workspace-1', 'apps');
    expect(createConnectorDirectory).toHaveBeenCalledWith({
      device_id: 'device-1',
      workspace_id: 'workspace-1',
      path: 'apps/new',
    });
    expect(cloudRequest).not.toHaveBeenCalled();
  });

  it('keeps local resources unavailable in a normal browser', async () => {
    vi.stubGlobal('window', {});
    await expect(
      workspaceLocalConnectorFacade.listLocalConnectorWorkspaces.call({} as never),
    ).rejects.toThrow('Local Connector 功能只能在 Chat OS 桌面客户端中使用');
  });

  it('registers a newly created local project before it is selected', async () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const createProject = vi.fn().mockResolvedValue({
      id: 'local-project-1',
      name: 'Local project',
      root_path: 'local://connector/device/workspace/app',
    });
    const registerLocalProjectExecution = vi.fn();
    const context = {
      getLocalRuntimeClient: () => ({ createProject }),
      registerLocalProjectExecution,
    };

    await workspaceLocalConnectorFacade.createLocalConnectorProject.call(context as never, {
      name: 'Local project',
      device_id: 'device',
      workspace_id: 'workspace',
      relative_path: 'app',
    });

    expect(registerLocalProjectExecution).toHaveBeenCalledWith('local-project-1');
  });
});

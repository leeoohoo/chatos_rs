// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { workspaceLocalConnectorFacade } from './localConnectorsFacade';

describe('workspaceLocalConnectorFacade desktop routing', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('loads local connector resources through the cloud gateway API', async () => {
    vi.stubGlobal('window', {});
    const cloudRequest = vi.fn()
      .mockResolvedValueOnce([{ id: 'device-1' }])
      .mockResolvedValueOnce([{ id: 'workspace-1' }])
      .mockResolvedValueOnce({ path: '.', entries: [] })
      .mockResolvedValueOnce({ path: 'apps/new', created: true });
    const context = {
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

    expect(cloudRequest).toHaveBeenNthCalledWith(1, '/local-connectors/devices');
    expect(cloudRequest).toHaveBeenNthCalledWith(2, '/local-connectors/workspaces');
    expect(cloudRequest).toHaveBeenNthCalledWith(
      3,
      '/local-connectors/fs/list?device_id=device-1&workspace_id=workspace-1&path=apps',
    );
    expect(cloudRequest).toHaveBeenNthCalledWith(4, '/local-connectors/fs/mkdir', {
      method: 'POST',
      body: JSON.stringify({
        device_id: 'device-1',
        workspace_id: 'workspace-1',
        path: 'apps/new',
      }),
    });
  });

  it('allows local connector resources in a normal browser session', async () => {
    vi.stubGlobal('window', {});
    const request = vi.fn().mockResolvedValue([{ id: 'workspace-1' }]);
    const context = { getRequestFn: () => request };

    await expect(
      workspaceLocalConnectorFacade.listLocalConnectorWorkspaces.call(context as never),
    ).resolves.toEqual([{ id: 'workspace-1' }]);
    expect(request).toHaveBeenCalledWith('/local-connectors/workspaces');
  });

  it('loads the Task Runner executable Plugin catalog for the exact project', async () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const request = vi.fn().mockResolvedValue({ selectable_plugins: [] });
    const context = { getRequestFn: () => request };

    await workspaceLocalConnectorFacade.listTaskRunnerAvailablePlugins.call(
      context as never,
      'project-1',
    );

    expect(request).toHaveBeenCalledWith(
      '/task-runner/available-plugins?project_id=project-1',
    );

    await workspaceLocalConnectorFacade.listTaskRunnerAvailablePlugins.call(
      context as never,
      'project-1',
      true,
    );
    expect(request).toHaveBeenLastCalledWith(
      '/task-runner/available-plugins?project_id=project-1&plan_mode=true',
    );
  });

  it('creates a cloud-managed project for the selected local workspace', async () => {
    vi.stubGlobal('window', {});
    const request = vi.fn().mockResolvedValue({
      id: 'local-project-1',
      name: 'Local project',
      root_path: 'local://connector/device/workspace/app',
    });
    const context = { getRequestFn: () => request };

    const data = {
      name: 'Local project',
      device_id: 'device',
      workspace_id: 'workspace',
      relative_path: 'app',
    };
    await workspaceLocalConnectorFacade.createLocalConnectorProject.call(context as never, data);

    expect(request).toHaveBeenCalledWith('/local-connectors/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  });
});

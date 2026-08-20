// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type ApiClient from '../../lib/api/client';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import type { Project } from '../../types';
import { useTaskPluginPicker } from './useTaskPluginPicker';

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});

const project = (overrides: Partial<Project> = {}): Project => ({
  id: 'project-1',
  name: 'Project',
  rootPath: 'local://connector/device-1/workspace-1/project',
  createdAt: new Date(0),
  updatedAt: new Date(0),
  ...overrides,
});

const plugin = (
  id: string,
  hosts: Array<'local' | 'portable'>,
): TaskRunnerSelectablePluginResponse => ({
  id,
  plugin_key: id,
  display_name: id,
  description: id,
  version: '1.0.0',
  release_id: `release-${id}`,
  artifact_sha256: 'a'.repeat(64),
  execution_type: hosts.every((host) => host === 'portable') ? 'portable' : 'local',
  requires_device: false,
  component_hosts: {},
  component_keys: hosts.map((_, index) => `component-${index}`),
  components: hosts.map((host, index) => ({
    component_key: `component-${index}`,
    kind: 'mcp_server',
    execution_host: host,
    available: true,
    status: 'ready',
    prepare_provider: 'mcp_management',
    requires_workspace: host === 'local',
  })),
  commands: [],
});

const renderPicker = (
  client: ApiClient,
  selectedProject: Project | null,
  projectId = selectedProject?.id || null,
) => renderHook(() => (
  useTaskPluginPicker({
    client,
    conversationId: 'conversation-1',
    project: selectedProject,
    projectId,
    disabled: false,
    planMode: false,
  })
));

describe('useTaskPluginPicker project runtime gating', () => {
  it('hides the picker for a project without a Local Connector root', async () => {
    const local = plugin('local', ['local']);
    const portable = plugin('portable', ['portable']);
    const listLocalConnectorDevices = vi.fn();
    const listTaskRunnerAvailablePlugins = vi.fn().mockResolvedValue({
      selectable_plugins: [local, portable],
    });
    const client = {
      listLocalConnectorDevices,
      listTaskRunnerAvailablePlugins,
    } as unknown as ApiClient;

    const { result } = renderPicker(client, project({ rootPath: '/unmanaged/project' }));

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.visible).toBe(false);
    expect(result.current.filteredPlugins).toEqual([]);
    expect(listLocalConnectorDevices).not.toHaveBeenCalled();
    expect(listTaskRunnerAvailablePlugins).not.toHaveBeenCalled();
  });

  it('hides the picker when the local project device is offline', async () => {
    const listLocalConnectorDevices = vi.fn().mockResolvedValue([
      { id: 'device-1', status: 'offline' },
    ]);
    const listTaskRunnerAvailablePlugins = vi.fn();
    const client = {
      listLocalConnectorDevices,
      listTaskRunnerAvailablePlugins,
    } as unknown as ApiClient;

    const { result } = renderPicker(client, project({
      rootPath: 'local://connector/device-1/workspace-1/apps/backend',
    }));

    await waitFor(() => expect(listLocalConnectorDevices).toHaveBeenCalled());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.visible).toBe(false);
    expect(result.current.filteredPlugins).toEqual([]);
    expect(listTaskRunnerAvailablePlugins).not.toHaveBeenCalled();
  });

  it('shows local-capable Plugins when the bound local device is online', async () => {
    const local = plugin('local', ['local']);
    const portable = plugin('portable', ['portable']);
    const listLocalConnectorDevices = vi.fn().mockResolvedValue([
      { id: 'device-1', status: 'online' },
    ]);
    const listTaskRunnerAvailablePlugins = vi.fn().mockResolvedValue({
      selectable_plugins: [local, portable],
    });
    const client = {
      listLocalConnectorDevices,
      listTaskRunnerAvailablePlugins,
    } as unknown as ApiClient;

    const { result } = renderPicker(client, project({
      rootPath: 'local://connector/device-1/workspace-1/apps/backend',
    }));

    await waitFor(() => expect(result.current.visible).toBe(true));
    expect(result.current.filteredPlugins).toEqual([local, portable]);
    expect(listTaskRunnerAvailablePlugins).toHaveBeenCalledWith('project-1', false);
  });

  it('loads missing local project details before checking its device and Plugins', async () => {
    const local = plugin('local', ['local']);
    const getProject = vi.fn().mockResolvedValue({
      id: 'project-1',
      name: 'Local project',
      root_path: 'local://connector/device-1/workspace-1/apps/backend',
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    });
    const listLocalConnectorDevices = vi.fn().mockResolvedValue([
      { id: 'device-1', status: 'online' },
    ]);
    const listTaskRunnerAvailablePlugins = vi.fn().mockResolvedValue({
      selectable_plugins: [local],
    });
    const client = {
      getProject,
      listLocalConnectorDevices,
      listTaskRunnerAvailablePlugins,
    } as unknown as ApiClient;

    const { result } = renderPicker(client, null, 'project-1');

    await waitFor(() => expect(result.current.visible).toBe(true));
    expect(getProject).toHaveBeenCalledWith('project-1');
    expect(listLocalConnectorDevices).toHaveBeenCalledTimes(1);
    expect(listTaskRunnerAvailablePlugins).toHaveBeenCalledWith('project-1', false);
    expect(result.current.filteredPlugins).toEqual([local]);
  });

  it('ignores stale project details after switching projects', async () => {
    let resolveFirstProject!: (value: unknown) => void;
    const firstProject = new Promise<unknown>((resolve) => {
      resolveFirstProject = resolve;
    });
    const local = plugin('local', ['local']);
    const getProject = vi.fn()
      .mockReturnValueOnce(firstProject)
      .mockResolvedValueOnce({
        id: 'project-2',
        name: 'Current project',
        root_path: 'local://connector/device-2/workspace-2/project',
        created_at: '2026-01-01T00:00:00Z',
        updated_at: '2026-01-01T00:00:00Z',
      });
    const listTaskRunnerAvailablePlugins = vi.fn().mockResolvedValue({
      selectable_plugins: [local],
    });
    const client = {
      getProject,
      listLocalConnectorDevices: vi.fn().mockResolvedValue([
        { id: 'device-2', status: 'online' },
      ]),
      listTaskRunnerAvailablePlugins,
    } as unknown as ApiClient;

    const { result, rerender } = renderHook(
      ({ selectedProjectId }: { selectedProjectId: string }) => useTaskPluginPicker({
        client,
        conversationId: 'conversation-1',
        project: null,
        projectId: selectedProjectId,
        disabled: false,
        planMode: false,
      }),
      { initialProps: { selectedProjectId: 'project-1' } },
    );

    rerender({ selectedProjectId: 'project-2' });
    await waitFor(() => expect(result.current.visible).toBe(true));

    resolveFirstProject({
      id: 'project-1',
      name: 'Old local project',
      root_path: 'local://connector/device-1/workspace-1/apps/backend',
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    });
    await Promise.resolve();

    expect(result.current.filteredPlugins).toEqual([local]);
    expect(client.listLocalConnectorDevices).toHaveBeenCalledTimes(1);
    expect(listTaskRunnerAvailablePlugins).toHaveBeenCalledTimes(1);
    expect(listTaskRunnerAvailablePlugins).toHaveBeenCalledWith('project-2', false);
  });

  it('hides the picker when the executable catalog is empty', async () => {
    const client = {
      listLocalConnectorDevices: vi.fn().mockResolvedValue([
        { id: 'device-1', status: 'online' },
      ]),
      listTaskRunnerAvailablePlugins: vi.fn().mockResolvedValue({ selectable_plugins: [] }),
    } as unknown as ApiClient;

    const { result } = renderPicker(client, project());

    await waitFor(() => expect(client.listTaskRunnerAvailablePlugins).toHaveBeenCalled());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.visible).toBe(false);
  });
});

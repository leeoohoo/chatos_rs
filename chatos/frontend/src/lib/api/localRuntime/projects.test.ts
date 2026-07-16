// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { createLocalRuntimeProject, listLocalRuntimeProjects } from './projects';

const record = {
  project_id: 'project-1',
  owner_user_id: 'user-1',
  device_id: 'device 1',
  workspace_id: 'workspace-1',
  project_name: 'Local Project',
  root_relative_path: 'apps/my project',
  execution_plane: 'local_connector' as const,
  runtime_schema_version: 1,
  created_at: '2026-07-16T00:00:00Z',
  updated_at: '2026-07-16T00:00:00Z',
};

describe('local runtime projects', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('maps SQLite records to Chat OS project responses', async () => {
    const apiRequest = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      body: JSON.stringify([record]),
    });
    vi.stubGlobal('window', { chatosLocalRuntime: { apiRequest } });

    const projects = await listLocalRuntimeProjects();

    expect(projects).toEqual([expect.objectContaining({
      id: 'project-1',
      name: 'Local Project',
      execution_plane: 'local_connector',
      root_path: 'local://connector/device%201/workspace-1/apps/my%20project',
    })]);
  });

  it('creates local project metadata through the Core bridge only', async () => {
    const apiRequest = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      body: JSON.stringify(record),
    });
    vi.stubGlobal('window', { chatosLocalRuntime: { apiRequest } });

    await createLocalRuntimeProject({
      name: 'Local Project',
      device_id: 'device 1',
      workspace_id: 'workspace-1',
      relative_path: 'apps/my project',
    });

    expect(apiRequest).toHaveBeenCalledWith(expect.objectContaining({
      endpoint: '/api/local/runtime/projects',
      method: 'POST',
    }));
  });
});

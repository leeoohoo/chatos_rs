// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { PUBLIC_PROJECT_ID } from '../../lib/domain/contactSessions';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import type { Project } from '../../types';
import {
  filterPluginsForProjectRuntime,
  resolveTaskPluginRuntimeScope,
} from './pluginRuntimeScope';

const project = (overrides: Partial<Project> = {}): Project => ({
  id: 'project-1',
  name: 'Project',
  rootPath: '/project',
  sourceType: 'cloud',
  createdAt: new Date(0),
  updatedAt: new Date(0),
  ...overrides,
});

const plugin = (
  id: string,
  providers: Array<'task_runner_cloud' | 'local_connector'>,
): TaskRunnerSelectablePluginResponse => ({
  id,
  plugin_key: id,
  display_name: id,
  description: id,
  version: '1.0.0',
  release_id: `release-${id}`,
  artifact_sha256: 'a'.repeat(64),
  execution_type: providers.includes('local_connector') ? 'local' : 'cloud',
  requires_device: providers.includes('local_connector'),
  component_hosts: {},
  component_keys: providers.map((_, index) => `component-${index}`),
  components: providers.map((provider, index) => ({
    component_key: `component-${index}`,
    kind: 'mcp_server',
    execution_host: provider === 'local_connector' ? 'local' : 'cloud',
    available: true,
    status: 'ready',
    prepare_provider: provider,
    requires_workspace: provider === 'local_connector',
  })),
  commands: [],
});

describe('Plugin runtime scope', () => {
  it('resolves cloud projects without requiring a Local Connector device', () => {
    expect(resolveTaskPluginRuntimeScope('conversation-1', project())).toEqual({
      projectId: 'project-1',
      sourceKind: 'cloud',
      localDeviceId: null,
    });
  });

  it('requires the managed Local Connector device for a local project', () => {
    expect(resolveTaskPluginRuntimeScope('conversation-1', project({
      sourceType: 'local_connector',
      rootPath: 'local://connector/device-1/workspace-1/apps/backend',
    }))).toEqual({
      projectId: 'project-1',
      sourceKind: 'local_connector',
      localDeviceId: 'device-1',
    });
    expect(resolveTaskPluginRuntimeScope('conversation-1', project({
      sourceType: 'local_connector',
      rootPath: '/unmanaged/local/path',
    }))).toBeNull();
  });

  it('rejects missing conversations and non-concrete projects', () => {
    expect(resolveTaskPluginRuntimeScope(null, project())).toBeNull();
    expect(resolveTaskPluginRuntimeScope('conversation-1', null)).toBeNull();
    expect(resolveTaskPluginRuntimeScope('conversation-1', project({ id: '0' }))).toBeNull();
    expect(resolveTaskPluginRuntimeScope(
      'conversation-1',
      project({ id: PUBLIC_PROJECT_ID }),
    )).toBeNull();
  });

  it('keeps cloud projects cloud-only and local projects local-capable', () => {
    const cloud = plugin('cloud', ['task_runner_cloud']);
    const local = plugin('local', ['local_connector']);
    const hybrid = plugin('hybrid', ['task_runner_cloud', 'local_connector']);

    expect(filterPluginsForProjectRuntime([cloud, local, hybrid], 'cloud'))
      .toEqual([cloud]);
    expect(filterPluginsForProjectRuntime([cloud, local, hybrid], 'local_connector'))
      .toEqual([local, hybrid]);
  });
});

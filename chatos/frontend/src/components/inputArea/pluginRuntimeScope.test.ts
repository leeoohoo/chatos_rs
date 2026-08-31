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
  rootPath: 'local://connector/device-1/workspace-1/project',
  createdAt: new Date(0),
  updatedAt: new Date(0),
  ...overrides,
});

const plugin = (id: string): TaskRunnerSelectablePluginResponse => ({
  id,
  plugin_key: id,
  display_name: id,
  description: id,
  version: '1.0.0',
  release_id: `release-${id}`,
  artifact_sha256: 'a'.repeat(64),
  requires_device: false,
  component_keys: ['component-0'],
  components: [{
    component_key: 'component-0',
    kind: 'mcp_server',
    available: true,
    status: 'ready',
    prepare_provider: 'mcp_management',
    requires_workspace: true,
  }],
  commands: [],
});

describe('Plugin runtime scope', () => {
  it('resolves the bound Local Connector device', () => {
    expect(resolveTaskPluginRuntimeScope('conversation-1', project())).toEqual({
      projectId: 'project-1',
      sourceKind: 'local_connector',
      localDeviceId: 'device-1',
    });
  });

  it('rejects projects without a managed Local Connector root', () => {
    expect(resolveTaskPluginRuntimeScope('conversation-1', project({
      rootPath: 'local://connector/device-1/workspace-1/apps/backend',
    }))).toEqual({
      projectId: 'project-1',
      sourceKind: 'local_connector',
      localDeviceId: 'device-1',
    });
    expect(resolveTaskPluginRuntimeScope('conversation-1', project({
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

  it('keeps Plugins with an available component for Local Connector execution', () => {
    const first = plugin('first');
    const second = plugin('second');
    const unavailable = plugin('unavailable');
    unavailable.components[0].available = false;

    expect(filterPluginsForProjectRuntime([first, second, unavailable], 'local_connector'))
      .toEqual([first, second]);
  });
});

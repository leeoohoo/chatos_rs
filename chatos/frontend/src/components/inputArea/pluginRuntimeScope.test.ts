// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import {
  filterPluginsForRuntime,
  pluginRequiresLocalWorkspace,
  pluginUsesLocalConnector,
  taskPluginPickerEnabled,
} from './pluginRuntimeScope';

const plugin = (
  id: string,
  prepareProvider: 'task_runner_cloud' | 'local_connector',
  deviceId: string | null = null,
  requiresWorkspace = false,
): TaskRunnerSelectablePluginResponse => ({
  id,
  plugin_key: id,
  display_name: id,
  description: id,
  version: '1.0.0',
  release_id: `release-${id}`,
  artifact_sha256: 'a'.repeat(64),
  device_id: deviceId,
  execution_type: prepareProvider === 'task_runner_cloud' ? 'cloud' : 'local',
  requires_device: prepareProvider === 'local_connector',
  component_hosts: {},
  component_keys: ['main'],
  components: [{
    component_key: 'main',
    kind: 'mcp_server',
    execution_host: prepareProvider === 'task_runner_cloud' ? 'cloud' : 'local',
    available: true,
    status: 'ready',
    prepare_provider: prepareProvider,
    requires_workspace: requiresWorkspace,
  }],
  commands: [],
});

describe('Plugin runtime scope', () => {
  it('exposes manual Plugin selection only for Local Connector conversations', () => {
    expect(taskPluginPickerEnabled('conversation-1', true)).toBe(true);
    expect(taskPluginPickerEnabled('conversation-1', false)).toBe(false);
    expect(taskPluginPickerEnabled(null, true)).toBe(false);
  });

  it('keeps Local Connector Plugins out of cloud projects', () => {
    const cloud = plugin('cloud', 'task_runner_cloud');
    const local = plugin('local', 'local_connector', 'device-1');

    expect(filterPluginsForRuntime([cloud, local], 'cloud', null)).toEqual([cloud]);
  });

  it('shows cloud Plugins without a device and local Plugins only for the selected device', () => {
    const cloud = plugin('cloud', 'task_runner_cloud');
    const localOne = plugin('local-1', 'local_connector', 'device-1');
    const localTwo = plugin('local-2', 'local_connector', 'device-2');

    expect(filterPluginsForRuntime([cloud, localOne, localTwo], 'local_connector', null))
      .toEqual([cloud]);
    expect(filterPluginsForRuntime(
      [cloud, localOne, localTwo],
      'local_connector',
      'device-1',
    )).toEqual([cloud, localOne]);
  });

  it('derives device and workspace requirements from executable local components', () => {
    const local = plugin('local', 'local_connector', 'device-1', true);
    expect(pluginUsesLocalConnector(local)).toBe(true);
    expect(pluginRequiresLocalWorkspace(local)).toBe(true);

    local.components[0].available = false;
    expect(pluginUsesLocalConnector(local)).toBe(false);
    expect(pluginRequiresLocalWorkspace(local)).toBe(false);
  });
});

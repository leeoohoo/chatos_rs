// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';

export type TaskPluginRuntimeProvider = 'cloud' | 'local_connector';

export const taskPluginPickerEnabled = (
  conversationId: string | null | undefined,
  localConnectorEnabled: boolean,
): boolean => Boolean(conversationId && localConnectorEnabled);

export const pluginUsesLocalConnector = (
  plugin: TaskRunnerSelectablePluginResponse,
): boolean => (Array.isArray(plugin.components) ? plugin.components : []).some((component) => (
  component.available && component.prepare_provider === 'local_connector'
));

export const pluginRequiresLocalWorkspace = (
  plugin: TaskRunnerSelectablePluginResponse,
): boolean => (Array.isArray(plugin.components) ? plugin.components : []).some((component) => (
  component.available
  && component.prepare_provider === 'local_connector'
  && component.requires_workspace
));

export const filterPluginsForRuntime = (
  plugins: TaskRunnerSelectablePluginResponse[],
  runtimeProvider: TaskPluginRuntimeProvider,
  deviceId: string | null,
): TaskRunnerSelectablePluginResponse[] => plugins.filter((plugin) => {
  const usesLocalConnector = pluginUsesLocalConnector(plugin);
  if (runtimeProvider === 'cloud') {
    return !usesLocalConnector;
  }
  if (!usesLocalConnector) {
    return true;
  }
  return Boolean(deviceId) && plugin.device_id === deviceId;
});

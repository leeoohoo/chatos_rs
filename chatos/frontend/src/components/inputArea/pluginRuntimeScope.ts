// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PUBLIC_PROJECT_ID } from '../../lib/domain/contactSessions';
import { parseLocalConnectorProjectRoot } from '../../lib/api/localRuntime';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import type { Project } from '../../types';

export interface TaskPluginRuntimeScope {
  projectId: string;
  sourceKind: 'local_connector';
  localDeviceId: string | null;
}

export const resolveTaskPluginRuntimeScope = (
  conversationId: string | null | undefined,
  project: Project | null | undefined,
): TaskPluginRuntimeScope | null => {
  const projectId = String(project?.id || '').trim();
  if (!conversationId || !projectId || projectId === '0' || projectId === PUBLIC_PROJECT_ID) {
    return null;
  }
  const localRoot = parseLocalConnectorProjectRoot(project?.rootPath);
  if (!localRoot?.deviceId) {
    return null;
  }
  return {
    projectId,
    sourceKind: 'local_connector',
    localDeviceId: localRoot?.deviceId || null,
  };
};

export const filterPluginsForProjectRuntime = (
  plugins: TaskRunnerSelectablePluginResponse[],
  _sourceKind: 'local_connector',
): TaskRunnerSelectablePluginResponse[] => plugins.filter((plugin) => {
  const availableComponents = (Array.isArray(plugin.components) ? plugin.components : [])
    .filter((component) => component.available);
  return availableComponents.some((component) => (
    component.prepare_provider === 'local_connector'
  ));
});

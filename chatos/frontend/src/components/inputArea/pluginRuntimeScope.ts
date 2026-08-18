// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PUBLIC_PROJECT_ID } from '../../lib/domain/contactSessions';
import { resolveProjectSourceKind, type ProjectSourceKind } from '../../lib/domain/projectSource';
import { parseLocalConnectorProjectRoot } from '../../lib/api/localRuntime';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import type { Project } from '../../types';

export interface TaskPluginRuntimeScope {
  projectId: string;
  sourceKind: ProjectSourceKind;
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
  const sourceKind = resolveProjectSourceKind(project);
  const localRoot = sourceKind === 'local_connector'
    ? parseLocalConnectorProjectRoot(project?.rootPath)
    : null;
  if (sourceKind === 'local_connector' && !localRoot?.deviceId) {
    return null;
  }
  return {
    projectId,
    sourceKind,
    localDeviceId: localRoot?.deviceId || null,
  };
};

export const filterPluginsForProjectRuntime = (
  plugins: TaskRunnerSelectablePluginResponse[],
  sourceKind: ProjectSourceKind,
): TaskRunnerSelectablePluginResponse[] => plugins.filter((plugin) => {
  const availableComponents = (Array.isArray(plugin.components) ? plugin.components : [])
    .filter((component) => component.available);
  if (sourceKind === 'cloud') {
    return availableComponents.length > 0
      && availableComponents.every((component) => (
        component.prepare_provider === 'task_runner_cloud'
      ));
  }
  return availableComponents.some((component) => (
    component.prepare_provider === 'local_connector'
  ));
});

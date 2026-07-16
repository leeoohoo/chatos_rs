// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRuntimeEnvironmentProgressResponse,
  ProjectRuntimeEnvironmentResponse,
  UpdateProjectRuntimeEnvironmentSettingsPayload,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

const environmentPath = (projectId: string, suffix = ''): string => (
  `/api/local/runtime/projects/${encodeURIComponent(projectId)}/runtime-environment${suffix}`
);

export const getLocalProjectRuntimeEnvironment = (
  projectId: string,
): Promise<ProjectRuntimeEnvironmentResponse> => requestLocalRuntime(
  environmentPath(projectId),
);

export const updateLocalProjectRuntimeEnvironmentSettings = (
  projectId: string,
  payload: UpdateProjectRuntimeEnvironmentSettingsPayload,
): Promise<ProjectRuntimeEnvironmentResponse> => requestLocalRuntime(
  environmentPath(projectId, '/settings'),
  { method: 'PUT', body: JSON.stringify(payload) },
);

export const analyzeLocalProjectRuntimeEnvironment = (
  projectId: string,
): Promise<ProjectRuntimeEnvironmentResponse> => requestLocalRuntime(
  environmentPath(projectId, '/analyze'),
  { method: 'POST' },
);

export const getLocalProjectRuntimeEnvironmentProgress = (
  projectId: string,
): Promise<ProjectRuntimeEnvironmentProgressResponse> => requestLocalRuntime(
  environmentPath(projectId, '/progress'),
);

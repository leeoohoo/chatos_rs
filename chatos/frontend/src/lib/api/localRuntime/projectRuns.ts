// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRunCatalogResponse,
  ProjectRunEnvironmentResponse,
  ProjectRunExecuteResponse,
  ProjectRunStateResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

const projectRunEndpoint = (projectId: string, suffix: string): string => (
  `/api/local/runtime/projects/${encodeURIComponent(projectId)}/run/${suffix}`
);

export const analyzeLocalProjectRun = (projectId: string): Promise<ProjectRunCatalogResponse> =>
  requestLocalRuntime(projectRunEndpoint(projectId, 'analyze'), { method: 'POST' });

export const getLocalProjectRunCatalog = (projectId: string): Promise<ProjectRunCatalogResponse> =>
  requestLocalRuntime(projectRunEndpoint(projectId, 'catalog'));

export const getLocalProjectRunState = (projectId: string): Promise<ProjectRunStateResponse> =>
  requestLocalRuntime(projectRunEndpoint(projectId, 'state'));

export const getLocalProjectRunEnvironment = (projectId: string): Promise<ProjectRunEnvironmentResponse> =>
  requestLocalRuntime(projectRunEndpoint(projectId, 'environment'));

export const updateLocalProjectRunEnvironment = (
  projectId: string,
  data: {
    selected_toolchains?: Record<string, string>;
    custom_toolchains?: Record<string, { kind?: string; label?: string; path?: string }>;
    env_vars?: Record<string, string>;
    terminal_ui_enabled?: boolean;
  },
): Promise<ProjectRunEnvironmentResponse> => requestLocalRuntime(
  projectRunEndpoint(projectId, 'environment'),
  { method: 'PUT', body: JSON.stringify(data) },
);

export const executeLocalProjectRun = (
  projectId: string,
  data: {
    target_id?: string;
    cwd?: string;
    command?: string;
    create_if_missing?: boolean;
    terminal_id?: string;
  },
): Promise<ProjectRunExecuteResponse> => requestLocalRuntime(
  projectRunEndpoint(projectId, 'execute'),
  { method: 'POST', body: JSON.stringify(data) },
);

export const setLocalProjectRunDefault = (
  projectId: string,
  targetId: string,
): Promise<ProjectRunCatalogResponse> => requestLocalRuntime(
  projectRunEndpoint(projectId, 'default'),
  { method: 'POST', body: JSON.stringify({ target_id: targetId }) },
);

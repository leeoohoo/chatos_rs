// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CreateLocalConnectorProjectRequest,
  DeleteSuccessResponse,
  ProjectResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';
import { parseLocalConnectorProjectRoot } from './projectRoot';
import type { LocalRuntimeProjectRecord } from './types';

const encodeRootSegment = (value: string): string => encodeURIComponent(value.trim());

const localProjectRoot = (record: LocalRuntimeProjectRecord): string => {
  const base = `local://connector/${encodeRootSegment(record.device_id)}/${encodeRootSegment(record.workspace_id)}`;
  const relativePath = String(record.root_relative_path || '')
    .trim()
    .replace(/\\/g, '/')
    .replace(/^\/+|\/+$/g, '');
  if (!relativePath) {
    return base;
  }
  return `${base}/${relativePath.split('/').map(encodeRootSegment).join('/')}`;
};

export const localProjectRecordToResponse = (
  record: LocalRuntimeProjectRecord,
): ProjectResponse => ({
  id: record.project_id,
  name: record.project_name,
  root_path: localProjectRoot(record),
  source_type: 'local_connector',
  execution_plane: 'local_connector',
  user_id: record.owner_user_id,
  created_at: record.created_at,
  updated_at: record.updated_at,
});

export const listLocalRuntimeProjects = async (): Promise<ProjectResponse[]> => {
  const records = await requestLocalRuntime<LocalRuntimeProjectRecord[]>(
    '/api/local/runtime/projects',
  );
  return records.map(localProjectRecordToResponse);
};

export const createLocalRuntimeProject = async (
  data: CreateLocalConnectorProjectRequest,
): Promise<ProjectResponse> => {
  const record = await requestLocalRuntime<LocalRuntimeProjectRecord>(
    '/api/local/runtime/projects',
    {
      method: 'POST',
      body: JSON.stringify({
        project_name: data.name,
        workspace_id: data.workspace_id,
        root_relative_path: data.relative_path,
      }),
    },
  );
  return localProjectRecordToResponse(record);
};

export const getLocalRuntimeProject = async (projectId: string): Promise<ProjectResponse> => {
  const record = await requestLocalRuntime<LocalRuntimeProjectRecord>(
    `/api/local/runtime/projects/${encodeURIComponent(projectId)}`,
  );
  return localProjectRecordToResponse(record);
};

export const updateLocalRuntimeProject = async (
  projectId: string,
  data: { name?: string; root_path?: string },
): Promise<ProjectResponse> => {
  const current = await requestLocalRuntime<LocalRuntimeProjectRecord>(
    `/api/local/runtime/projects/${encodeURIComponent(projectId)}`,
  );
  const requestedRoot = data.root_path ? parseLocalConnectorProjectRoot(data.root_path) : null;
  const record = await requestLocalRuntime<LocalRuntimeProjectRecord>(
    `/api/local/runtime/projects/${encodeURIComponent(projectId)}`,
    {
      method: 'PUT',
      body: JSON.stringify({
        project_name: data.name || current.project_name,
        workspace_id: requestedRoot?.workspaceId || current.workspace_id,
        root_relative_path: requestedRoot?.relativePath ?? current.root_relative_path,
      }),
    },
  );
  return localProjectRecordToResponse(record);
};

export const deleteLocalRuntimeProject = (projectId: string): Promise<DeleteSuccessResponse> =>
  requestLocalRuntime<DeleteSuccessResponse>(
    `/api/local/runtime/projects/${encodeURIComponent(projectId)}`,
    { method: 'DELETE' },
  );

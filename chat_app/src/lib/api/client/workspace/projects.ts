import { buildQuery } from '../shared';
import type {
  DeleteSuccessResponse,
  ProjectChangeConfirmResponse,
  ProjectChangeLogResponse,
  ProjectChangeSummaryResponse,
  ProjectContactLinkResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
} from '../types';
import type { ApiRequestFn, ContactPaging } from './common';

export const listProjects = (request: ApiRequestFn, userId?: string): Promise<ProjectResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<ProjectResponse[]>(`/projects${query}`);
};

export const createProject = (
  request: ApiRequestFn,
  data: { name: string; root_path: string; description?: string; user_id?: string },
): Promise<ProjectResponse> => {
  return request<ProjectResponse>('/projects', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateProject = (
  request: ApiRequestFn,
  id: string,
  data: { name?: string; root_path?: string; description?: string },
): Promise<ProjectResponse> => {
  return request<ProjectResponse>(`/projects/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteProject = (request: ApiRequestFn, id: string): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/projects/${id}`, {
    method: 'DELETE',
  });
};

export const getProject = (request: ApiRequestFn, id: string): Promise<ProjectResponse> => {
  return request<ProjectResponse>(`/projects/${id}`);
};

export const analyzeProjectRun = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRunCatalogResponse> => {
  return request<ProjectRunCatalogResponse>(`/projects/${encodeURIComponent(projectId)}/run/analyze`, {
    method: 'POST',
  });
};

export const getProjectRunCatalog = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRunCatalogResponse> => {
  return request<ProjectRunCatalogResponse>(`/projects/${encodeURIComponent(projectId)}/run/catalog`);
};

export const executeProjectRun = (
  request: ApiRequestFn,
  projectId: string,
  data: {
    target_id?: string;
    cwd?: string;
    command?: string;
    create_if_missing?: boolean;
  },
): Promise<ProjectRunExecuteResponse> => {
  return request<ProjectRunExecuteResponse>(`/projects/${encodeURIComponent(projectId)}/run/execute`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const setProjectRunDefault = (
  request: ApiRequestFn,
  projectId: string,
  targetId: string,
): Promise<ProjectRunCatalogResponse> => {
  return request<ProjectRunCatalogResponse>(`/projects/${encodeURIComponent(projectId)}/run/default`, {
    method: 'POST',
    body: JSON.stringify({ target_id: targetId }),
  });
};

export const listProjectContacts = (
  request: ApiRequestFn,
  projectId: string,
  paging?: ContactPaging,
): Promise<ProjectContactLinkResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ProjectContactLinkResponse[]>(`/projects/${encodeURIComponent(projectId)}/contacts${query}`);
};

export const addProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  data: { contact_id: string },
): Promise<ProjectContactLinkResponse> => {
  return request<ProjectContactLinkResponse>(`/projects/${encodeURIComponent(projectId)}/contacts`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const removeProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  contactId: string,
): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(
    `/projects/${encodeURIComponent(projectId)}/contacts/${encodeURIComponent(contactId)}`,
    { method: 'DELETE' },
  );
};

export const listProjectChangeLogs = (
  request: ApiRequestFn,
  projectId: string,
  params?: { path?: string; limit?: number; offset?: number },
): Promise<ProjectChangeLogResponse[]> => {
  const query = buildQuery({
    path: params?.path,
    limit: params?.limit,
    offset: params?.offset,
  });
  return request<ProjectChangeLogResponse[]>(`/projects/${projectId}/changes${query}`);
};

export const getProjectChangeSummary = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectChangeSummaryResponse> => {
  return request<ProjectChangeSummaryResponse>(`/projects/${projectId}/changes/summary`);
};

export const confirmProjectChanges = (
  request: ApiRequestFn,
  projectId: string,
  payload: { mode?: 'all' | 'paths' | 'change_ids'; paths?: string[]; change_ids?: string[] },
): Promise<ProjectChangeConfirmResponse> => {
  return request<ProjectChangeConfirmResponse>(`/projects/${projectId}/changes/confirm`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

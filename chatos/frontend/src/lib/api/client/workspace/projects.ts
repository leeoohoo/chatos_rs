// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildQuery } from '../shared';
import type {
  DeleteSuccessResponse,
  ProjectContactLockResponse,
  ProjectContactLinkResponse,
  ProjectPlanOptions,
  ProjectPlanResponse,
  ProjectRequirementWorkItemsOptions,
  ProjectRequirementWorkItemsResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementExecuteResponse,
  ProjectRequirementStopResponse,
  ProjectRuntimeEnvironmentResponse,
  ProjectRuntimeEnvironmentProgressResponse,
  ProjectRunEnvironmentResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
  ProjectRunStateResponse,
  UpdateProjectRuntimeEnvironmentSettingsPayload,
} from '../types';
import type { ApiRequestFn, ContactPaging } from './common';

export const listProjects = (request: ApiRequestFn, userId?: string): Promise<ProjectResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<ProjectResponse[]>(`/projects${query}`);
};

export const createProject = (
  request: ApiRequestFn,
  data: { name: string; root_path: string; git_url?: string; description?: string; user_id?: string },
): Promise<ProjectResponse> => {
  return request<ProjectResponse>('/projects', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const createCloudProject = (
  request: ApiRequestFn,
  data: FormData,
): Promise<ProjectResponse> => {
  return request<ProjectResponse>('/projects/cloud', {
    method: 'POST',
    body: data,
  });
};

export const updateProject = (
  request: ApiRequestFn,
  id: string,
  data: { name?: string; root_path?: string; git_url?: string; description?: string },
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

export const getProjectRuntimeEnvironment = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRuntimeEnvironmentResponse> => {
  return request<ProjectRuntimeEnvironmentResponse>(`/projects/${encodeURIComponent(projectId)}/runtime-environment`);
};

export const updateProjectRuntimeEnvironmentSettings = (
  request: ApiRequestFn,
  projectId: string,
  data: UpdateProjectRuntimeEnvironmentSettingsPayload,
): Promise<ProjectRuntimeEnvironmentResponse> => {
  return request<ProjectRuntimeEnvironmentResponse>(`/projects/${encodeURIComponent(projectId)}/runtime-environment/settings`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const analyzeProjectRuntimeEnvironment = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRuntimeEnvironmentResponse> => {
  return request<ProjectRuntimeEnvironmentResponse>(
    `/projects/${encodeURIComponent(projectId)}/runtime-environment/analyze`,
    { method: 'POST' },
  );
};

export const generateProjectRuntimeEnvironmentImage = (
  request: ApiRequestFn,
  projectId: string,
  imageRecordId: string,
): Promise<ProjectRuntimeEnvironmentResponse> => {
  return request<ProjectRuntimeEnvironmentResponse>(
    `/projects/${encodeURIComponent(projectId)}/runtime-environment/images/${encodeURIComponent(imageRecordId)}/generate`,
    { method: 'POST' },
  );
};

export const getProjectRuntimeEnvironmentProgress = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRuntimeEnvironmentProgressResponse> => {
  return request<ProjectRuntimeEnvironmentProgressResponse>(
    `/projects/${encodeURIComponent(projectId)}/runtime-environment/progress`,
  );
};

export const getProjectPlan = (
  request: ApiRequestFn,
  projectId: string,
  options?: ProjectPlanOptions,
): Promise<ProjectPlanResponse> => {
  const query = buildQuery({
    include_archived: options?.includeArchived,
    include_work_items: options?.includeWorkItems,
  });
  return request<ProjectPlanResponse>(`/projects/${encodeURIComponent(projectId)}/plan${query}`);
};

export const listProjectRequirementWorkItems = (
  request: ApiRequestFn,
  projectId: string,
  requirementId: string,
  options?: ProjectRequirementWorkItemsOptions,
): Promise<ProjectRequirementWorkItemsResponse> => {
  const query = buildQuery({
    include_archived: options?.includeArchived,
    include_dependency_graph: options?.includeDependencyGraph,
  });
  return request<ProjectRequirementWorkItemsResponse>(
    `/projects/${encodeURIComponent(projectId)}/requirements/${encodeURIComponent(requirementId)}/work-items${query}`,
  );
};

export const listProjectRequirementDocuments = (
  request: ApiRequestFn,
  projectId: string,
  requirementId: string,
): Promise<ProjectRequirementDocumentResponse[]> => {
  return request<ProjectRequirementDocumentResponse[]>(
    `/projects/${encodeURIComponent(projectId)}/requirements/${encodeURIComponent(requirementId)}/documents`,
  );
};

export const executeProjectRequirement = (
  request: ApiRequestFn,
  projectId: string,
  requirementId: string,
  data?: {
    contact_id?: string;
    model_config_id?: string;
    modelConfigId?: string;
    include_prerequisite_dependents?: boolean;
    includePrerequisiteDependents?: boolean;
  },
): Promise<ProjectRequirementExecuteResponse> => {
  return request<ProjectRequirementExecuteResponse>(
    `/projects/${encodeURIComponent(projectId)}/requirements/${encodeURIComponent(requirementId)}/execute`,
    {
      method: 'POST',
      body: JSON.stringify(data || {}),
    },
  );
};

export const stopProjectRequirementExecution = (
  request: ApiRequestFn,
  projectId: string,
  requirementId: string,
  data?: { contact_id?: string },
): Promise<ProjectRequirementStopResponse> => {
  return request<ProjectRequirementStopResponse>(
    `/projects/${encodeURIComponent(projectId)}/requirements/${encodeURIComponent(requirementId)}/stop`,
    {
      method: 'POST',
      body: JSON.stringify(data || {}),
    },
  );
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
    terminal_id?: string;
  },
): Promise<ProjectRunExecuteResponse> => {
  return request<ProjectRunExecuteResponse>(`/projects/${encodeURIComponent(projectId)}/run/execute`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getProjectRunState = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRunStateResponse> => {
  return request<ProjectRunStateResponse>(`/projects/${encodeURIComponent(projectId)}/run/state`);
};

export const getProjectRunEnvironment = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRunEnvironmentResponse> => {
  return request<ProjectRunEnvironmentResponse>(`/projects/${encodeURIComponent(projectId)}/run/environment`);
};

export const updateProjectRunEnvironment = (
  request: ApiRequestFn,
  projectId: string,
  data: {
    selected_toolchains?: Record<string, string>;
    custom_toolchains?: Record<string, { kind?: string; label?: string; path?: string }>;
    env_vars?: Record<string, string>;
    terminal_ui_enabled?: boolean;
  },
): Promise<ProjectRunEnvironmentResponse> => {
  return request<ProjectRunEnvironmentResponse>(`/projects/${encodeURIComponent(projectId)}/run/environment`, {
    method: 'PUT',
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
    localRuntime = false,
): Promise<ProjectContactLinkResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
    local_runtime: localRuntime || undefined,
  });
  return request<ProjectContactLinkResponse[]>(`/projects/${encodeURIComponent(projectId)}/contacts${query}`);
};

export const getProjectContactLock = (
  request: ApiRequestFn,
  projectId: string,
  localRuntime = false,
): Promise<ProjectContactLockResponse> => {
  const query = buildQuery({ local_runtime: localRuntime || undefined });
  return request<ProjectContactLockResponse>(
    `/projects/${encodeURIComponent(projectId)}/contacts/lock${query}`,
  );
};

export const addProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  data: { contact_id: string },
  localRuntime = false,
): Promise<ProjectContactLinkResponse> => {
  const query = buildQuery({ local_runtime: localRuntime || undefined });
  return request<ProjectContactLinkResponse>(`/projects/${encodeURIComponent(projectId)}/contacts${query}`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const removeProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  contactId: string,
  localRuntime = false,
): Promise<DeleteSuccessResponse> => {
  const query = buildQuery({ local_runtime: localRuntime || undefined });
  return request<DeleteSuccessResponse>(
    `/projects/${encodeURIComponent(projectId)}/contacts/${encodeURIComponent(contactId)}${query}`,
    { method: 'DELETE' },
  );
};

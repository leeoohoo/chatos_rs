// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentAccountListItem,
  AuthUser,
  CreateProjectPayload,
  CreateRequirementPayload,
  CreateWorkItemPayload,
  DependencyGraphResponse,
  LoginPayload,
  LoginResponse,
  ProjectProfileRecord,
  ProjectManagementSkillLocale,
  ProjectManagementSkillResponse,
  ProjectRecord,
  ProjectRuntimeEnvironmentResponse,
  ProjectStatus,
  ProjectWorkItemRecord,
  ProjectWorkItemTaskRunnerLinkRecord,
  RequirementDependencyRecord,
  RequirementDocumentRecord,
  RequirementRecord,
  RequirementStatus,
  UpdateRequirementDocumentPayload,
  UpdateProjectPayload,
  UpdateProjectRuntimeEnvironmentSettingsPayload,
  UpdateRequirementPayload,
  UpdateWorkItemPayload,
  UpsertRequirementDocumentPayload,
  UpsertProjectProfilePayload,
  WorkItemDependencyRecord,
  ProjectWorkItemStatus,
} from '../types';

const RAW_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = RAW_API_BASE_URL.replace(/\/+$/, '').replace(/\/api$/, '');
const AUTH_TOKEN_STORAGE_KEY = 'project_management_service_auth_token';

export function getAuthToken(): string | null {
  return window.localStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}

export function setAuthToken(token: string): void {
  window.localStorage.setItem(AUTH_TOKEN_STORAGE_KEY, token);
  window.dispatchEvent(new Event('project-service-auth-changed'));
}

export function clearAuthToken(): void {
  window.localStorage.removeItem(AUTH_TOKEN_STORAGE_KEY);
  window.dispatchEvent(new Event('project-service-auth-changed'));
}

function buildApiUrl(path: string): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return API_BASE_URL ? `${API_BASE_URL}${normalizedPath}` : normalizedPath;
}

type QueryValue = string | number | boolean | null | undefined;

function withQuery(path: string, params: Record<string, QueryValue>): string {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value === undefined || value === null) {
      return;
    }
    const text = String(value).trim();
    if (text) {
      search.set(key, text);
    }
  });
  const query = search.toString();
  return query ? `${path}?${query}` : path;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const token = getAuthToken();
  if (token && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  const response = await fetch(buildApiUrl(path), {
    ...init,
    headers,
  });
  if (!response.ok) {
    let message = response.statusText;
    try {
      const data = (await response.json()) as { error?: string };
      if (data.error) {
        message = data.error;
      }
    } catch {
      // keep status text
    }
    if (response.status === 401) {
      clearAuthToken();
    }
    throw new Error(message);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  if (!text.trim()) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

export const api = {
  login: (payload: LoginPayload) =>
    request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  currentUser: () => request<AuthUser>('/api/auth/me'),
  listAgentAccounts: () => request<AgentAccountListItem[]>('/api/agent-accounts'),
  getProjectManagementSkill: (locale: ProjectManagementSkillLocale) =>
    request<ProjectManagementSkillResponse>(
      withQuery('/api/skills/project-management', {
        lang: locale,
      }),
    ),
  listProjects: (status?: ProjectStatus) =>
    request<ProjectRecord[]>(
      withQuery('/api/projects', {
        status,
      }),
    ),
  createProject: (payload: CreateProjectPayload) =>
    request<ProjectRecord>('/api/projects', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  getProject: (id: string) => request<ProjectRecord>(`/api/projects/${id}`),
  updateProject: (id: string, payload: UpdateProjectPayload) =>
    request<ProjectRecord>(`/api/projects/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  archiveProject: (id: string) =>
    request<ProjectRecord>(`/api/projects/${id}`, {
      method: 'DELETE',
    }),
  getProjectProfile: (projectId: string) =>
    request<ProjectProfileRecord>(`/api/projects/${projectId}/profile`),
  upsertProjectProfile: (projectId: string, payload: UpsertProjectProfilePayload) =>
    request<ProjectProfileRecord>(`/api/projects/${projectId}/profile`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  getProjectRuntimeEnvironment: (projectId: string) =>
    request<ProjectRuntimeEnvironmentResponse>(
      `/api/projects/${projectId}/runtime-environment`,
    ),
  updateProjectRuntimeEnvironmentSettings: (
    projectId: string,
    payload: UpdateProjectRuntimeEnvironmentSettingsPayload,
  ) =>
    request<ProjectRuntimeEnvironmentResponse>(
      `/api/projects/${projectId}/runtime-environment/settings`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      },
    ),
  analyzeProjectRuntimeEnvironment: (projectId: string) =>
    request<ProjectRuntimeEnvironmentResponse>(
      `/api/projects/${projectId}/runtime-environment/analyze`,
      {
        method: 'POST',
      },
    ),
  listRequirements: (
    projectId: string,
    filters?: { status?: RequirementStatus; keyword?: string; include_archived?: boolean },
  ) =>
    request<RequirementRecord[]>(
      withQuery(`/api/projects/${projectId}/requirements`, {
        status: filters?.status,
        keyword: filters?.keyword,
        include_archived: filters?.include_archived,
      }),
    ),
  createRequirement: (projectId: string, payload: CreateRequirementPayload) =>
    request<RequirementRecord>(`/api/projects/${projectId}/requirements`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateRequirement: (id: string, payload: UpdateRequirementPayload) =>
    request<RequirementRecord>(`/api/requirements/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  archiveRequirement: (id: string) =>
    request<RequirementRecord>(`/api/requirements/${id}`, {
      method: 'DELETE',
    }),
  listRequirementDependencies: (id: string) =>
    request<RequirementDependencyRecord[]>(`/api/requirements/${id}/dependencies`),
  setRequirementDependencies: (id: string, ids: string[]) =>
    request<RequirementDependencyRecord[]>(`/api/requirements/${id}/dependencies`, {
      method: 'PUT',
      body: JSON.stringify({ prerequisite_requirement_ids: ids }),
    }),
  getRequirementTechnicalOverview: (id: string) =>
    request<RequirementDocumentRecord>(`/api/requirements/${id}/technical-overview`),
  upsertRequirementTechnicalOverview: (
    id: string,
    payload: { title?: string; format?: string; content: string },
  ) =>
    request<RequirementDocumentRecord>(`/api/requirements/${id}/technical-overview`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  listRequirementDocuments: (id: string, filters?: { doc_type?: string }) =>
    request<RequirementDocumentRecord[]>(
      withQuery(`/api/requirements/${id}/documents`, { doc_type: filters?.doc_type }),
    ),
  createRequirementDocument: (id: string, payload: UpsertRequirementDocumentPayload) =>
    request<RequirementDocumentRecord>(`/api/requirements/${id}/documents`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  getRequirementDocument: (requirementId: string, documentId: string) =>
    request<RequirementDocumentRecord>(
      `/api/requirements/${requirementId}/documents/${documentId}`,
    ),
  updateRequirementDocument: (
    requirementId: string,
    documentId: string,
    payload: UpdateRequirementDocumentPayload,
  ) =>
    request<RequirementDocumentRecord>(
      `/api/requirements/${requirementId}/documents/${documentId}`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      },
    ),
  listProjectWorkItems: (
    projectId: string,
    filters?: {
      status?: ProjectWorkItemStatus;
      keyword?: string;
      is_planning_task?: boolean;
      include_archived?: boolean;
    },
  ) =>
    request<ProjectWorkItemRecord[]>(
      withQuery(`/api/projects/${projectId}/work-items`, {
        status: filters?.status,
        keyword: filters?.keyword,
        is_planning_task: filters?.is_planning_task,
        include_archived: filters?.include_archived,
      }),
    ),
  createWorkItem: (requirementId: string, payload: CreateWorkItemPayload) =>
    request<ProjectWorkItemRecord>(`/api/requirements/${requirementId}/work-items`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateWorkItem: (id: string, payload: UpdateWorkItemPayload) =>
    request<ProjectWorkItemRecord>(`/api/work-items/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  archiveWorkItem: (id: string) =>
    request<ProjectWorkItemRecord>(`/api/work-items/${id}`, {
      method: 'DELETE',
    }),
  listWorkItemDependencies: (id: string) =>
    request<WorkItemDependencyRecord[]>(`/api/work-items/${id}/dependencies`),
  setWorkItemDependencies: (id: string, ids: string[]) =>
    request<WorkItemDependencyRecord[]>(`/api/work-items/${id}/dependencies`, {
      method: 'PUT',
      body: JSON.stringify({ prerequisite_work_item_ids: ids }),
    }),
  getProjectDependencyGraph: (projectId: string, filters?: { include_archived?: boolean }) =>
    request<DependencyGraphResponse>(
      withQuery(`/api/projects/${projectId}/dependency-graph`, {
        include_archived: filters?.include_archived,
      }),
    ),
  listTaskRunnerLinks: (workItemId: string) =>
    request<ProjectWorkItemTaskRunnerLinkRecord[]>(
      `/api/work-items/${workItemId}/task-runner-links`,
    ),
  linkTaskRunnerTask: (
    workItemId: string,
    payload: { task_runner_task_id: string; task_runner_run_id?: string; link_type?: string },
  ) =>
    request<ProjectWorkItemTaskRunnerLinkRecord>(
      `/api/work-items/${workItemId}/task-runner-links`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      },
    ),
  deleteTaskRunnerLink: (workItemId: string, linkId: string) =>
    request<void>(`/api/work-items/${workItemId}/task-runner-links/${linkId}`, {
      method: 'DELETE',
    }),
};

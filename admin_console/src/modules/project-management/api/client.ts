// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentAccountListItem,
  CreateProjectPayload,
  CreateRequirementPayload,
  CreateWorkItemPayload,
  DependencyGraphResponse,
  ProjectProfileRecord,
  ProjectManagementSkillLocale,
  ProjectManagementSkillResponse,
  ProjectRecord,
  ProjectStatus,
  ProjectWorkItemRecord,
  ProjectWorkItemTaskRunnerLinkRecord,
  RequirementDependencyRecord,
  RequirementDocumentRecord,
  RequirementRecord,
  RequirementStatus,
  UpdateRequirementDocumentPayload,
  UpdateProjectPayload,
  UpdateRequirementPayload,
  UpdateWorkItemPayload,
  UpsertRequirementDocumentPayload,
  UpsertProjectProfilePayload,
  WorkItemDependencyRecord,
  ProjectWorkItemStatus,
} from '../types';

import {
  createJsonApiClient,
  withQuery,
} from '@chatos/frontend-runtime';
import {
  clearAuthToken as clearSharedAuthToken,
  getAuthToken as getSharedAuthToken,
} from '../../../shared/auth/tokenStore';
import { ADMIN_SERVICE_BASES, stripBackendApiPrefix } from '../../../shared/api/servicePaths';

const API_BASE_URL = ADMIN_SERVICE_BASES.projectService;

export function getAuthToken(): string | null {
  return getSharedAuthToken();
}

export function clearAuthToken(): void {
  clearSharedAuthToken();
}

const rawRequest = createJsonApiClient({
  baseUrl: API_BASE_URL,
  timeoutMs: 30_000,
  getAuthToken,
  onUnauthorized: clearAuthToken,
});

const request = <T,>(path: string, init?: RequestInit) =>
  rawRequest<T>(stripBackendApiPrefix(path), init);

export const api = {
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

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectPlanOptions,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementWorkItemsOptions,
  ProjectRequirementWorkItemsResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

export const getLocalProjectPlan = (
  projectId: string,
  options: ProjectPlanOptions = {},
): Promise<ProjectPlanResponse> => {
  const query = new URLSearchParams();
  if (options.includeArchived !== undefined) {
    query.set('include_archived', String(options.includeArchived));
  }
  if (options.includeWorkItems !== undefined) {
    query.set('include_work_items', String(options.includeWorkItems));
  }
  const suffix = query.size > 0 ? `?${query.toString()}` : '';
  return requestLocalRuntime<ProjectPlanResponse>(
    `/api/local/runtime/projects/${encodeURIComponent(projectId)}/plan${suffix}`,
  );
};

export const listLocalProjectRequirementWorkItems = (
  projectId: string,
  requirementId: string,
  options: ProjectRequirementWorkItemsOptions = {},
): Promise<ProjectRequirementWorkItemsResponse> => {
  const query = new URLSearchParams();
  if (options.includeArchived !== undefined) {
    query.set('include_archived', String(options.includeArchived));
  }
  if (options.includeDependencyGraph !== undefined) {
    query.set('include_dependency_graph', String(options.includeDependencyGraph));
  }
  const suffix = query.size > 0 ? `?${query.toString()}` : '';
  return requestLocalRuntime<ProjectRequirementWorkItemsResponse>(
    `/api/local/runtime/projects/${encodeURIComponent(projectId)}/requirements/${encodeURIComponent(requirementId)}/work-items${suffix}`,
  );
};

export const listLocalProjectRequirementDocuments = (
  projectId: string,
  requirementId: string,
): Promise<ProjectRequirementDocumentResponse[]> => requestLocalRuntime<
  ProjectRequirementDocumentResponse[]
>(
  `/api/local/runtime/projects/${encodeURIComponent(projectId)}/requirements/${encodeURIComponent(requirementId)}/documents`,
);

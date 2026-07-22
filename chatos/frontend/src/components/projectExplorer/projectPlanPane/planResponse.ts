// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectDependencyGraphResponse,
  ProjectPlanResponse,
  ProjectRequirementWorkItemsResponse,
  ProjectWorkItemCountsResponse,
  ProjectWorkItemResponse,
} from '../../../lib/api/client/types';

export const normalizeRequirementWorkItemsResponse = (
  response: ProjectRequirementWorkItemsResponse | ProjectWorkItemResponse[],
): {
  dependencyGraph: ProjectDependencyGraphResponse | null;
  workItems: ProjectWorkItemResponse[];
} => {
  if (Array.isArray(response)) return { dependencyGraph: null, workItems: response };
  return {
    dependencyGraph: response.dependencyGraph || response.dependency_graph || null,
    workItems: Array.isArray(response.workItems) ? response.workItems : (response.work_items || []),
  };
};

export const planWorkItemCounts = (
  plan: ProjectPlanResponse | null,
): ProjectWorkItemCountsResponse | null => plan?.workItemCounts || plan?.work_item_counts || null;

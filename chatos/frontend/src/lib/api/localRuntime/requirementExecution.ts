// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRequirementExecuteResponse,
  ProjectRequirementStopResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

type RequirementExecutionPayload = {
  contact_id?: string;
  include_prerequisite_dependents?: boolean;
  includePrerequisiteDependents?: boolean;
};

const requirementPath = (
  projectId: string,
  requirementId: string,
  action: 'execute' | 'stop',
): string => (
  `/api/local/runtime/projects/${encodeURIComponent(projectId)}`
  + `/requirements/${encodeURIComponent(requirementId)}/${action}`
);

export const executeLocalProjectRequirement = (
  projectId: string,
  requirementId: string,
  payload: RequirementExecutionPayload = {},
): Promise<ProjectRequirementExecuteResponse> => requestLocalRuntime(
  requirementPath(projectId, requirementId, 'execute'),
  { method: 'POST', body: JSON.stringify(payload) },
);

export const stopLocalProjectRequirement = (
  projectId: string,
  requirementId: string,
  payload: Pick<RequirementExecutionPayload, 'contact_id'> = {},
): Promise<ProjectRequirementStopResponse> => requestLocalRuntime(
  requirementPath(projectId, requirementId, 'stop'),
  { method: 'POST', body: JSON.stringify(payload) },
);

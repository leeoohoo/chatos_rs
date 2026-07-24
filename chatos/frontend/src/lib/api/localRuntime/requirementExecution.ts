// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRequirementExecuteResponse,
  ProjectRequirementExecutionPlanResponse,
  ProjectRequirementDispatchResponse,
  ProjectRequirementConfirmResponse,
  ProjectRequirementStopResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

type RequirementExecutionPayload = {
  contact_id?: string;
  model_config_id?: string;
  modelConfigId?: string;
  include_prerequisite_dependents?: boolean;
  includePrerequisiteDependents?: boolean;
  planning_feedback?: string;
  planningFeedback?: string;
  replaces_execution_group_id?: string;
  replacesExecutionGroupId?: string;
  replaces_conversation_id?: string;
  replacesConversationId?: string;
};

const requirementPath = (
  projectId: string,
  requirementId: string,
  action: 'execute' | 'execution-plan' | 'confirm-execution' | 'pause' | 'resume' | 'stop' | 'rerun',
): string => (
  `/api/local/runtime/projects/${encodeURIComponent(projectId)}`
  + `/requirements/${encodeURIComponent(requirementId)}/${action}`
);

export const getLocalProjectRequirementExecutionPlan = (
  projectId: string,
  requirementId: string,
  identity?: { conversationId?: string; executionGroupId?: string },
): Promise<ProjectRequirementExecutionPlanResponse> => {
  const query = new URLSearchParams();
  if (identity?.conversationId) query.set('conversation_id', identity.conversationId);
  if (identity?.executionGroupId) query.set('execution_group_id', identity.executionGroupId);
  const suffix = query.size > 0 ? `?${query.toString()}` : '';
  return requestLocalRuntime(
    `${requirementPath(projectId, requirementId, 'execution-plan')}${suffix}`,
  );
};

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
  payload: {
    contact_id?: string;
    execution_group_id?: string;
    conversation_id?: string;
    discard_tasks?: boolean;
  } = {},
): Promise<ProjectRequirementStopResponse> => requestLocalRuntime(
  requirementPath(projectId, requirementId, 'stop'),
  { method: 'POST', body: JSON.stringify(payload) },
);

export const confirmLocalProjectRequirementExecution = (
  projectId: string,
  requirementId: string,
  payload: {
    execution_group_id: string;
    conversation_id: string;
    contact_id?: string;
  },
): Promise<ProjectRequirementConfirmResponse> => requestLocalRuntime(
  requirementPath(projectId, requirementId, 'confirm-execution'),
  { method: 'POST', body: JSON.stringify(payload) },
);

const mutateLocalProjectRequirementExecutionDispatch = (
  projectId: string,
  requirementId: string,
  action: 'pause' | 'resume',
  payload: {
    execution_group_id: string;
    conversation_id: string;
    contact_id?: string;
  },
): Promise<ProjectRequirementDispatchResponse> => requestLocalRuntime(
  requirementPath(projectId, requirementId, action),
  { method: 'POST', body: JSON.stringify(payload) },
);

export const pauseLocalProjectRequirementExecution = (
  projectId: string,
  requirementId: string,
  payload: {
    execution_group_id: string;
    conversation_id: string;
    contact_id?: string;
  },
): Promise<ProjectRequirementDispatchResponse> => (
  mutateLocalProjectRequirementExecutionDispatch(projectId, requirementId, 'pause', payload)
);

export const resumeLocalProjectRequirementExecution = (
  projectId: string,
  requirementId: string,
  payload: {
    execution_group_id: string;
    conversation_id: string;
    contact_id?: string;
  },
): Promise<ProjectRequirementDispatchResponse> => (
  mutateLocalProjectRequirementExecutionDispatch(projectId, requirementId, 'resume', payload)
);

export const rerunLocalProjectRequirementExecution = (
  projectId: string,
  requirementId: string,
  payload: {
    execution_group_id: string;
    conversation_id: string;
    contact_id?: string;
  },
): Promise<ProjectRequirementExecuteResponse> => requestLocalRuntime(
  requirementPath(projectId, requirementId, 'rerun'),
  { method: 'POST', body: JSON.stringify(payload) },
);

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRequirementExecuteResponse,
  ProjectRequirementExecutionPlanResponse,
  ProjectRequirementResponse,
  ProjectRequirementStopResponse,
  RequirementExecutionRecoveryAction,
} from '../../../lib/api/client/types';
import { ApiRequestError } from '../../../lib/api/client/shared';
import { normalizeRawMessages } from '../../../lib/domain/messages';
import type { Message } from '../../../types';
import { readText } from './model';
import type { RequirementExecutionProcessPhase } from './requirementExecutionPhase';

export interface RequirementExecutionProcess {
  requirement: ProjectRequirementResponse;
  projectId: string;
  conversationId: string;
  executionGroupId: string;
  messageId: string;
  contactId?: string | null;
  executionPlane?: string | null;
  selectedModelId?: string | null;
  includePrerequisiteDependents?: boolean;
  planningFeedback?: string | null;
  planningFeedbackHistory?: string[];
  serverStatus?: string | null;
  confirmationStatus?: string | null;
  taskCount?: number | null;
  hasStartedRuns?: boolean;
  executionPaused?: boolean;
  tasksDiscarded?: boolean;
  recoveryAction?: RequirementExecutionRecoveryAction | null;
  recoveryReason?: string | null;
  replacePreviousBatch?: boolean;
  initialMessage?: Message | null;
}

type ExecutionResponse =
  | ProjectRequirementExecuteResponse
  | ProjectRequirementExecutionPlanResponse
  | ProjectRequirementStopResponse;

const readTextList = (value: unknown): string[] => (
  Array.isArray(value)
    ? value.map(readText).filter(Boolean)
    : []
);

export const REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS = 10_000;

export const isPendingRequirementExecutionPlanError = (error: unknown): boolean => {
  if (!(error instanceof Error)) {
    return false;
  }
  const code = error instanceof ApiRequestError ? error.code : undefined;
  if (code === 'local_execution_plan_source_missing') {
    return true;
  }
  const message = error.message.toLowerCase();
  return message.includes('需求执行规划消息不存在')
    || message.includes('需求执行会话不存在')
    || message.includes('local execution plan source message was not found');
};

export const isRequirementExecutionRerunCancellationSettlingError = (error: unknown): boolean => {
  if (!(error instanceof ApiRequestError) || error.status !== 409) {
    return false;
  }
  return error.message.includes('旧执行批次仍有')
    || error.message.includes('旧批次仍有')
    || error.message.includes('正在取消');
};

export const shouldReplaceRequirementExecutionBatch = ({
  planDiscarded,
  replacePreviousBatch,
}: {
  planDiscarded: boolean;
  replacePreviousBatch?: boolean;
}): boolean => (
  !planDiscarded
  && Boolean(replacePreviousBatch)
);

const readRecoveryAction = (value: unknown): RequirementExecutionRecoveryAction | null => {
  const action = readText(value).trim().toLowerCase();
  return action === 'none' || action === 'rerun' || action === 'regenerate'
    ? action
    : null;
};

const readNumber = (value: unknown): number | null => {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
};

const responseHasField = (
  response: ExecutionResponse,
  snakeKey: string,
  camelKey: string,
): boolean => (
  Object.prototype.hasOwnProperty.call(response, snakeKey)
  || Object.prototype.hasOwnProperty.call(response, camelKey)
);

const readResponseField = (
  response: ExecutionResponse,
  snakeKey: string,
  camelKey: string,
): unknown => {
  const record = response as Record<string, unknown>;
  return record[snakeKey] ?? record[camelKey];
};

const normalizeRequirementExecutionRecoveryAction = ({
  discardedTasks,
  explicitAction,
  hasStartedRuns,
  status,
  taskCount,
}: {
  discardedTasks?: boolean;
  explicitAction: RequirementExecutionRecoveryAction | null;
  hasStartedRuns: boolean;
  status: string;
  taskCount: number | null;
}): RequirementExecutionRecoveryAction | null => {
  const normalizedStatus = status.trim().toLowerCase();
  if (normalizedStatus === 'stopping') {
    return explicitAction || 'none';
  }
  const canDeriveFromServerSummary = taskCount !== null;
  if (
    canDeriveFromServerSummary
    && ['stopped', 'cancelled', 'canceled'].includes(normalizedStatus)
  ) {
    if (discardedTasks || taskCount === 0) {
      return 'regenerate';
    }
    return 'rerun';
  }
  if (
    canDeriveFromServerSummary
    && normalizedStatus === 'failed'
    && !hasStartedRuns
  ) {
    return 'regenerate';
  }
  return explicitAction;
};

export const shouldStopRequirementExecutionBeforeReplacement = ({
  phase,
  replacePreviousBatch,
}: {
  phase: RequirementExecutionProcessPhase;
  replacePreviousBatch: boolean;
}): boolean => (
  replacePreviousBatch
  && !['stopped', 'completed'].includes(phase)
);

export const shouldShowCancelRequirementExecution = ({
  actuallyStarted,
  phase,
}: {
  actuallyStarted: boolean;
  phase: RequirementExecutionProcessPhase;
}): boolean => (
  actuallyStarted
  && phase !== 'stopped'
  && phase !== 'completed'
);

export const shouldShowDiscardRequirementPlan = ({
  actuallyStarted,
  phase,
}: {
  actuallyStarted: boolean;
  phase: RequirementExecutionProcessPhase;
}): boolean => (
  !actuallyStarted
  && phase !== 'stopped'
  && phase !== 'completed'
);

export const buildRequirementExecutionProcess = ({
  fallback,
  projectId,
  requirement,
  response,
}: {
  fallback?: RequirementExecutionProcess | null;
  projectId: string;
  requirement: ProjectRequirementResponse;
  response: ExecutionResponse;
}): RequirementExecutionProcess | null => {
  if ('found' in response && response.found === false) {
    return null;
  }
  const conversationId = readText(response.conversation_id)
    || readText(response.conversationId)
    || fallback?.conversationId
    || '';
  const [normalizedMessage] = response.message && conversationId
    ? normalizeRawMessages([response.message], conversationId)
    : [];
  const executionGroupId = readText(response.execution_group_id)
    || readText(response.executionGroupId)
    || readText(normalizedMessage?.metadata?.project_requirement_execution?.execution_group_id)
    || readText(normalizedMessage?.metadata?.conversation_turn_id)
    || fallback?.executionGroupId
    || '';
  const messageId = readText(response.message_id)
    || readText(response.messageId)
    || readText(normalizedMessage?.id)
    || fallback?.messageId
    || executionGroupId;
  if (!conversationId || !executionGroupId || !messageId) {
    return null;
  }
  const latestPlanningFeedback = readText(response.planning_feedback)
    || readText(response.planningFeedback)
    || readText(normalizedMessage?.metadata?.project_requirement_execution?.planning_feedback);
  const snakeCaseFeedbackHistory = readTextList(response.planning_feedback_history);
  const responseFeedbackHistory = snakeCaseFeedbackHistory.length > 0
    ? snakeCaseFeedbackHistory
    : readTextList(response.planningFeedbackHistory);
  const messageFeedbackHistory = readTextList(
    normalizedMessage?.metadata?.project_requirement_execution?.planning_feedback_history,
  );
  const fallbackFeedbackHistory = fallback?.planningFeedbackHistory?.length
    ? fallback.planningFeedbackHistory
    : fallback?.planningFeedback ? [fallback.planningFeedback] : [];
  const planningFeedbackHistory = (
    responseFeedbackHistory.length > 0
        ? responseFeedbackHistory
      : messageFeedbackHistory.length > 0
        ? messageFeedbackHistory
        : fallbackFeedbackHistory
  ).slice();
  if (
    latestPlanningFeedback
    && planningFeedbackHistory[planningFeedbackHistory.length - 1] !== latestPlanningFeedback
  ) {
    planningFeedbackHistory.push(latestPlanningFeedback);
  }
  const responseExecutionPaused = response.execution_paused ?? response.executionPaused;
  const responseTasksDiscarded = 'discarded_tasks' in response || 'discardedTasks' in response
    ? response.discarded_tasks ?? response.discardedTasks
    : undefined;
  const taskCount = responseHasField(response, 'task_count', 'taskCount')
    ? readNumber(readResponseField(response, 'task_count', 'taskCount'))
    : fallback?.taskCount ?? null;
  const hasStartedRuns = response.has_started_runs
    ?? response.hasStartedRuns
    ?? fallback?.hasStartedRuns
    ?? false;
  const metadataExecutionPaused = normalizedMessage
    ?.metadata
    ?.task_runner_async
    ?.execution_paused;
  const responseStatus = readText(response.status) || fallback?.serverStatus || '';
  const explicitRecoveryAction = readRecoveryAction(response.recovery_action)
    || readRecoveryAction(response.recoveryAction)
    || null;
  const recoveryAction = normalizeRequirementExecutionRecoveryAction({
    discardedTasks: typeof responseTasksDiscarded === 'boolean'
      ? responseTasksDiscarded
      : fallback?.tasksDiscarded,
    explicitAction: explicitRecoveryAction,
    hasStartedRuns,
    status: responseStatus,
    taskCount,
  });
  return {
    requirement,
    projectId,
    conversationId,
    executionGroupId,
    messageId,
    contactId: readText(response.contact_id)
      || readText(response.contactId)
      || readText(normalizedMessage?.metadata?.project_requirement_execution?.contact_id)
      || fallback?.contactId
      || null,
    executionPlane: readText(response.execution_plane)
      || readText(response.executionPlane)
      || readText(normalizedMessage?.metadata?.project_requirement_execution?.execution_plane)
      || fallback?.executionPlane
      || null,
    selectedModelId: readText(response.model_config_id)
      || readText(response.modelConfigId)
      || readText(normalizedMessage?.metadata?.model_config_id)
      || fallback?.selectedModelId
      || null,
    includePrerequisiteDependents: response.include_prerequisite_dependents
      ?? response.includePrerequisiteDependents
      ?? fallback?.includePrerequisiteDependents
      ?? false,
    planningFeedback: latestPlanningFeedback
      || planningFeedbackHistory[planningFeedbackHistory.length - 1]
      || fallback?.planningFeedback
      || null,
    planningFeedbackHistory,
    serverStatus: readText(response.status) || fallback?.serverStatus || null,
    confirmationStatus: readText(response.confirmation_status)
      || readText(response.confirmationStatus)
      || fallback?.confirmationStatus
      || null,
    taskCount,
    hasStartedRuns,
    executionPaused: typeof responseExecutionPaused === 'boolean'
      ? responseExecutionPaused
      : typeof metadataExecutionPaused === 'boolean'
        ? metadataExecutionPaused
        : fallback?.executionPaused ?? false,
    tasksDiscarded: typeof responseTasksDiscarded === 'boolean'
      ? responseTasksDiscarded
      : fallback?.tasksDiscarded ?? false,
    recoveryAction,
    recoveryReason: readText(response.recovery_reason)
      || readText(response.recoveryReason)
      || null,
    replacePreviousBatch: response.replace_previous_batch
      ?? response.replacePreviousBatch
      ?? false,
    initialMessage: normalizedMessage || fallback?.initialMessage || null,
  };
};

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../types';
import type {
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { readString } from './utils';

export interface ProjectExecutionConfirmationState {
  isProjectExecution: boolean;
  awaitingConfirmation: boolean;
  graphReadyForConfirmation: boolean;
  hasStartedTasks: boolean;
  canConfirm: boolean;
  projectId: string | null;
  requirementId: string | null;
  executionGroupId: string | null;
  conversationId: string;
  contactId: string | null;
  overallStatus: string;
}

export const resolveProjectExecutionConfirmationState = ({
  graph,
  message,
  tasks,
}: {
  graph: MessageTaskRunnerGraphResponse;
  message: Message;
  tasks: MessageTaskRunnerTask[];
}): ProjectExecutionConfirmationState => {
  const execution = message.metadata?.project_requirement_execution;
  const taskRunnerAsync = message.metadata?.task_runner_async;
  const mode = readString(taskRunnerAsync?.mode)?.toLowerCase() || '';
  const executionKind = readString(taskRunnerAsync?.execution_kind)?.toLowerCase() || '';
  const confirmationStatus = readString(taskRunnerAsync?.confirmation_status)?.toLowerCase() || '';
  const executionStatus = (
    readString(taskRunnerAsync?.overall_status)
    || readString(taskRunnerAsync?.status)
    || ''
  ).toLowerCase();
  const explicitlyAwaitingConfirmation = confirmationStatus === 'awaiting_confirmation'
    || executionStatus === 'awaiting_confirmation';
  const isProjectExecution = Boolean(execution)
    || mode === 'project_requirement_execution'
    || executionKind === 'project_requirement_execution';
  const graphReadyForConfirmation = tasks.length > 0 && tasks.every((task) => {
    const status = readString(task.status)?.toLowerCase() || '';
    return !readString(task.last_run_id)
      && ['ready', 'todo', 'queued', 'pending', 'doing'].includes(status);
  });
  const hasStartedTasks = tasks.some((task) => Boolean(readString(task.last_run_id)));
  const terminalPlanStatus = [
    'completed',
    'failed',
    'error',
    'stopped',
    'cancelled',
    'canceled',
  ].includes(executionStatus);
  const awaitingConfirmation = explicitlyAwaitingConfirmation
    || (graphReadyForConfirmation && !hasStartedTasks && !terminalPlanStatus);
  const overallStatus = awaitingConfirmation
    ? 'awaiting_confirmation'
    : executionStatus || confirmationStatus;
  const projectId = readString(execution?.project_id)
    || readString(taskRunnerAsync?.project_id);
  const requirementId = readString(execution?.requirement_id)
    || readString(taskRunnerAsync?.requirement_id);
  const executionGroupId = readString(execution?.execution_group_id)
    || readString(message.metadata?.conversation_turn_id)
    || readString(graph.source_turn_id)
    || readString(taskRunnerAsync?.source_turn_id)
    || readString(graph.source_user_message_id)
    || readString(taskRunnerAsync?.source_user_message_id)
    || readString(message.id);
  return {
    isProjectExecution,
    awaitingConfirmation,
    graphReadyForConfirmation,
    hasStartedTasks,
    canConfirm: Boolean(
      isProjectExecution
      && awaitingConfirmation
      && graphReadyForConfirmation
      && projectId
      && requirementId
      && executionGroupId
      && message.sessionId,
    ),
    projectId,
    requirementId,
    executionGroupId,
    conversationId: message.sessionId,
    contactId: readString(execution?.contact_id),
    overallStatus,
  };
};

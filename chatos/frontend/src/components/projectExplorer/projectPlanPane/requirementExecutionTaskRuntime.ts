// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MessageTaskRunnerTask } from '../../../lib/api/client/types';
import { readString } from '../../messageTasks/utils';
import { ACTIVE_TASK_STATUSES } from './requirementExecutionPhase';

const TERMINAL_TASK_STATUSES = new Set([
  'succeeded',
  'success',
  'completed',
  'done',
  'failed',
  'error',
  'cancelled',
  'canceled',
  'blocked',
  'archived',
]);

const normalizedTaskStatus = (task: MessageTaskRunnerTask): string => (
  readString(task.status)?.toLowerCase() || ''
);

const normalizedRunStatus = (task: MessageTaskRunnerTask): string => (
  readString(task.last_run?.status)?.toLowerCase() || ''
);

export const taskHasTerminalStatus = (task: MessageTaskRunnerTask): boolean => (
  TERMINAL_TASK_STATUSES.has(normalizedTaskStatus(task))
);

export const taskHasActiveRun = (task: MessageTaskRunnerTask): boolean => {
  if (!readString(task.last_run_id)) return false;
  if (taskHasTerminalStatus(task)) return false;
  const runStatus = normalizedRunStatus(task);
  if (runStatus) return ACTIVE_TASK_STATUSES.has(runStatus);
  return ACTIVE_TASK_STATUSES.has(normalizedTaskStatus(task));
};

export const taskHasRunningRun = (task: MessageTaskRunnerTask): boolean => {
  if (!readString(task.last_run_id)) return false;
  if (taskHasTerminalStatus(task)) return false;
  const runStatus = normalizedRunStatus(task);
  const status = runStatus || normalizedTaskStatus(task);
  return ['running', 'processing', 'in_progress', 'doing'].includes(status);
};

export const taskHasQueuedRun = (task: MessageTaskRunnerTask): boolean => {
  if (!readString(task.last_run_id)) return false;
  if (taskHasTerminalStatus(task)) return false;
  const runStatus = normalizedRunStatus(task);
  const status = runStatus || normalizedTaskStatus(task);
  return ['queued', 'pending'].includes(status);
};

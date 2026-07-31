// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MessageTaskRunnerTask } from '../../../lib/api/client/types';
import { readString } from '../../messageTasks/utils';
import { ACTIVE_TASK_STATUSES } from './requirementExecutionPhase';

export const taskHasActiveRun = (task: MessageTaskRunnerTask): boolean => {
  if (!readString(task.last_run_id)) return false;
  const runStatus = readString(task.last_run?.status)?.toLowerCase() || '';
  if (runStatus) return ACTIVE_TASK_STATUSES.has(runStatus);
  return ACTIVE_TASK_STATUSES.has(readString(task.status)?.toLowerCase() || '');
};

export const taskHasRunningRun = (task: MessageTaskRunnerTask): boolean => {
  if (!readString(task.last_run_id)) return false;
  const runStatus = readString(task.last_run?.status)?.toLowerCase() || '';
  const status = runStatus || readString(task.status)?.toLowerCase() || '';
  return ['running', 'processing', 'in_progress', 'doing'].includes(status);
};

export const taskHasQueuedRun = (task: MessageTaskRunnerTask): boolean => {
  if (!readString(task.last_run_id)) return false;
  const runStatus = readString(task.last_run?.status)?.toLowerCase() || '';
  const status = runStatus || readString(task.status)?.toLowerCase() || '';
  return ['queued', 'pending'].includes(status);
};

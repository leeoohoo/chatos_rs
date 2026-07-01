// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TaskRunStatus, AskUserPromptStatus } from '../../types';

export type RunStatusFilter = TaskRunStatus | 'all';

export const runColorMap: Record<TaskRunStatus, string> = {
  queued: 'default',
  running: 'processing',
  succeeded: 'success',
  failed: 'error',
  cancelled: 'default',
  blocked: 'warning',
};

export const promptColorMap: Record<AskUserPromptStatus, string> = {
  pending: 'processing',
  submitted: 'success',
  cancelled: 'default',
  timed_out: 'warning',
  failed: 'error',
};

export const runStatusFilterValues: RunStatusFilter[] = [
  'all',
  'queued',
  'running',
  'succeeded',
  'failed',
];

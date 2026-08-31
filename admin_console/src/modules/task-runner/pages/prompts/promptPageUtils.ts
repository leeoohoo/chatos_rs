// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { AskUserPromptStatus } from '../../types';

export type PromptStatusFilter = AskUserPromptStatus | 'all';

export const promptColorMap: Record<AskUserPromptStatus, string> = {
  pending: 'processing',
  submitted: 'success',
  cancelled: 'default',
  timed_out: 'warning',
  failed: 'error',
};

export const promptStatusFilterValues: PromptStatusFilter[] = [
  'all',
  'pending',
  'submitted',
  'cancelled',
  'timed_out',
];

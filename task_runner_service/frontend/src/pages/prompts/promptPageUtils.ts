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

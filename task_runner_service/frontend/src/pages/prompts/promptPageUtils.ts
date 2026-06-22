import type { UiPromptStatus } from '../../types';

export type PromptStatusFilter = UiPromptStatus | 'all';

export const promptColorMap: Record<UiPromptStatus, string> = {
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

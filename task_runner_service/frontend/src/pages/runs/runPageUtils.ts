// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { TranslateFn } from '../../i18n/I18nProvider';
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

export function formatUserVisibleRunText(
  value: string | null | undefined,
  t: TranslateFn,
): string {
  const trimmed = value?.trim();
  if (!trimmed) {
    return '-';
  }
  const lower = trimmed.toLowerCase();
  if (
    [
      'sandbox environment',
      'sandbox_environment',
      'docker environment image build',
      'copy failed: file not found',
      '.chatos/sandboxes',
      '/api/sandbox-environments/',
    ].some((marker) => lower.includes(marker))
  ) {
    return t('runs.error.environmentPreparationFailed');
  }
  if (
    [
      'error sending request for url',
      'connection refused',
      'connection reset',
      'network is unreachable',
      'service unavailable',
      'bad gateway',
      'gateway timeout',
      'timed out',
      'timeout',
    ].some((marker) => lower.includes(marker))
    || /http\s+5\d\d\b/.test(lower)
  ) {
    return t('runs.error.serviceUnavailable');
  }
  if (
    ['api_key', 'api key', 'authorization', 'bearer ', 'access_token', 'internal_trace']
      .some((marker) => lower.includes(marker))
  ) {
    return t('runs.error.requestFailed');
  }
  return trimmed;
}

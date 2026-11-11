// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { JobTypeKey } from './types';

export const RUN_STATUS_OPTIONS = ['running', 'done', 'failed', 'queued'];

export const JOB_TYPE_OPTIONS: JobTypeKey[] = [
  'summary',
  'rollup',
  'subject_memory',
];

export const THINKING_LEVEL_OPTIONS = [
  { label: 'none', value: 'none' },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
];

export const THREAD_TABLE_SCROLL_Y = '100%';
export const RECORD_TABLE_SCROLL_Y = '100%';
export const DETAIL_TABLE_SCROLL_Y = '100%';

export const JOB_TRIGGER_LABELS: Record<string, string> = {
  thread_direct: '线程直接触发',
  scheduler: '系统调度',
  database_worker: '后台自动执行',
  queue: '后台队列',
};

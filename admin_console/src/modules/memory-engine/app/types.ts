// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  EngineJobPolicy,
  EngineJobRun,
  EngineSource,
  ThreadQuery,
  UpsertEngineSourcePayload,
} from '../types';

export type TabKey = 'dashboard' | 'data' | 'sources' | 'policies' | 'runs' | 'reliability';

export type SourceFormValues = {
  tenant_id: string;
  source_id: string;
  name: string;
  description?: string;
  enabled: boolean;
};

export type ThreadFilterFormValues = ThreadQuery;
export type JobTypeKey = 'summary' | 'rollup' | 'subject_memory';
export type PolicyViewKey = JobTypeKey;

export type ToolSection = {
  key: string;
  label: string;
  body: string;
};

export type DashboardStats = {
  sources: number;
  policies: number;
  running: number;
  done: number;
  failed: number;
};

export type PolicyMap = Partial<Record<JobTypeKey, EngineJobPolicy>>;

export type SourcePayloadResult = {
  sourceId: string;
  payload: UpsertEngineSourcePayload;
};

export type ThreadNameLookup = Record<string, string>;

export type ThreadRunLookupInput = Pick<
  EngineJobRun,
  'thread_id' | 'tenant_id' | 'source_id'
>;

export type ThreadDisplayInput = Pick<EngineSource, never> | null;

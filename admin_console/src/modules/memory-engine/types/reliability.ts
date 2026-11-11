// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type ReliabilityScope = {
  tenant_id: string;
  source_id: string;
  limit?: number;
};

export type ReliabilityMigrationRun = {
  id: string;
  migration_kind: string;
  tenant_id: string;
  source_id: string;
  status: string;
  cursor: unknown;
  statistics: unknown;
  error_detail?: unknown | null;
  started_at: string;
  finished_at?: string | null;
};

export type PartitionRebuildBacklogItem = {
  tenant_id: string;
  source_id: string;
  thread_id: string;
  project_partition_key: string;
  rebuild_kind: string;
  status: string;
  migration_run_id: string;
  reason: string;
  attempt_count: number;
  last_error?: unknown | null;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
};

export type RecordCoverage = {
  id: string;
  summary_id: string;
  record_id: string;
  thread_id: string;
  project_partition_key: string;
  ordinal: number;
  source_ref: string;
  status: string;
  generation_id: string;
  created_at: string;
  retired_at?: string | null;
  retired_reason?: string | null;
};

export type DerivationLineage = {
  id: string;
  derivation_kind: string;
  output_scope_key: string;
  input_project_partition_key: string;
  output_kind: 'thread_summary' | 'subject_memory';
  output_id: string;
  input_kind: 'thread_summary' | 'subject_memory';
  input_id: string;
  ordinal: number;
  status: string;
  generation_id: string;
  created_at: string;
  retired_at?: string | null;
  retired_reason?: string | null;
};

export type ComposeDryRunRequest = {
  tenant_id: string;
  source_id: string;
  subject_id?: string;
  context_project_subject_id?: string;
  thread_id: string;
  query_text?: string;
  current_turn_id?: string;
  model_capability_revision: string;
  policy: {
    max_memory_tokens: number;
    max_recent_record_tokens: number;
    max_thread_summary_tokens: number;
    max_project_memory_tokens: number;
    max_agent_memory_tokens: number;
  };
};

export type ComposeDryRunItem = {
  id: string;
  item_type: string;
  pool: string;
  project_partition_key: string;
  estimated_tokens: number;
  score_components: Record<string, number>;
  selection_reason: string;
};

export type ComposeDryRunResponse = {
  thread_id: string;
  selected: ComposeDryRunItem[];
  meta: {
    candidate_count_by_pool: Record<string, number>;
    selected_count_by_pool: Record<string, number>;
    dropped_count_by_reason: Record<string, number>;
    budget_requested: unknown;
    budget_effective: unknown;
    budget_used: unknown;
    estimator_id: string;
    tokenizer_id?: string | null;
  };
};

export type ReliabilityQueueItem = {
  stream: string;
  item_id: string;
  project_partition_key?: string | null;
  status: string;
  version: number;
  error_code?: string | null;
  updated_at: string;
};

export type ReliabilityOperatorAction = {
  id: string;
  tenant_id: string;
  source_id: string;
  operator_id: string;
  action_kind: string;
  object_kind: string;
  object_id: string;
  project_partition_key?: string | null;
  reason: string;
  before_state: unknown;
  after_state: unknown;
  created_at: string;
};

export type RearmRebuildRequest = {
  tenant_id: string;
  source_id: string;
  thread_id: string;
  project_partition_key: string;
  rebuild_kind: string;
  reason: string;
};

export type ResummarizeCoveredRangeRequest = {
  tenant_id: string;
  source_id: string;
  summary_id: string;
  reason: string;
};

export type ResummarizeCoveredRangeResponse = {
  thread_id: string;
  accepted: boolean;
  running: boolean;
  job_run_id?: string | null;
  generated: boolean;
  summary_id?: string | null;
  source_record_count: number;
};

export type RetireSummaryRequest = {
  tenant_id: string;
  source_id: string;
  summary_id: string;
  reason: string;
};

export type RestoreRecordPendingRequest = {
  tenant_id: string;
  source_id: string;
  record_id: string;
  reason: string;
};

export type RearmReliabilityQueueRequest = {
  tenant_id: string;
  source_id: string;
  stream: string;
  item_id: string;
  project_partition_key?: string;
  version: number;
  reason: string;
};

export type ReliabilityDashboard = {
  schema_version: string;
  generated_at: string;
  finding_count: number;
  alerts: Array<{ code: string; severity: string; count: number }>;
  baseline: Record<string, number>;
  source_contract_readiness: Array<Record<string, unknown>>;
  runtime_readiness: {
    components: Array<{
      component: string;
      state: string;
      source_unit_ready: boolean;
      runtime_assertion_ready: boolean;
      model_capability_ready: boolean;
      missing: string[];
    }>;
  };
};

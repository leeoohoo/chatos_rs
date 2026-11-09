// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface EngineJobPolicy {
  job_type: string;
  dispatch_threshold_tokens?: number | null;
  max_source_batch_tokens?: number | null;
  max_model_input_tokens?: number | null;
  max_leaf_tokens?: number | null;
  max_merge_rounds?: number | null;
  count_limit?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
  updated_at: string;
}

export interface EngineJobRun {
  id: string;
  job_type: string;
  trigger_type: string;
  tenant_id?: string | null;
  source_id?: string | null;
  thread_id?: string | null;
  subject_id?: string | null;
  thread_label?: string | null;
  thread_display_name?: string | null;
  status: string;
  input_count: number;
  output_count: number;
  processed_count: number;
  success_count: number;
  error_count: number;
  metadata?: Record<string, unknown> | null;
  error_message?: string | null;
  started_at: string;
  finished_at?: string | null;
}

export interface JobRunQuery {
  job_type?: string;
  trigger_type?: string;
  thread_id?: string;
  status?: string;
  tenant_id?: string;
  source_id?: string;
  limit?: number;
}

export interface JobRunsBundle {
  thread_runs: EngineJobRun[];
  scheduler_runs: EngineJobRun[];
}

export interface DashboardOverview {
  source_count: number;
  policy_count: number;
  job_stats: Record<string, Record<string, number>>;
}

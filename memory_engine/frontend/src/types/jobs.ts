// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface EngineJobPolicy {
  job_type: string;
  enabled: boolean;
  model_profile_id?: string | null;
  summary_prompt?: string | null;
  summary_prompt_zh?: string | null;
  summary_prompt_en?: string | null;
  summary_prompt_language?: 'zh' | 'en';
  rollup_summary_prompt?: string | null;
  rollup_summary_prompt_zh?: string | null;
  rollup_summary_prompt_en?: string | null;
  rollup_summary_prompt_language?: 'zh' | 'en';
  token_limit?: number | null;
  target_summary_tokens?: number | null;
  interval_seconds?: number | null;
  max_threads_per_tick?: number | null;
  count_limit?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
  updated_at: string;
}

export interface UpsertEngineJobPolicyPayload {
  enabled?: boolean;
  model_profile_id?: string | null;
  summary_prompt?: string | null;
  summary_prompt_zh?: string | null;
  summary_prompt_en?: string | null;
  summary_prompt_language?: 'zh' | 'en';
  rollup_summary_prompt?: string | null;
  rollup_summary_prompt_zh?: string | null;
  rollup_summary_prompt_en?: string | null;
  rollup_summary_prompt_language?: 'zh' | 'en';
  token_limit?: number | null;
  target_summary_tokens?: number | null;
  interval_seconds?: number | null;
  max_threads_per_tick?: number | null;
  count_limit?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
}

export interface GenerateJobPolicyPromptPayload {
  prompt_field: 'summary_prompt' | 'rollup_summary_prompt';
  user_input: string;
}

export interface GenerateJobPolicyPromptResult {
  prompt_zh: string;
  prompt_en: string;
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
  model_count: number;
  policy_count: number;
  job_stats: Record<string, Record<string, number>>;
}

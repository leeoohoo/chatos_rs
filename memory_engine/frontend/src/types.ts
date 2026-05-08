export interface EngineModelProfile {
  id: string;
  name: string;
  provider: string;
  model: string;
  base_url?: string | null;
  api_key?: string | null;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  temperature?: number | null;
  thinking_level?: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface UpsertEngineModelProfilePayload {
  name: string;
  provider: string;
  model: string;
  base_url?: string | null;
  api_key?: string | null;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
  temperature?: number | null;
  thinking_level?: string | null;
  enabled?: boolean;
}

export interface EngineJobPolicy {
  job_type: string;
  enabled: boolean;
  model_profile_id?: string | null;
  summary_prompt?: string | null;
  token_limit?: number | null;
  round_limit?: number | null;
  target_summary_tokens?: number | null;
  interval_seconds?: number | null;
  max_threads_per_tick?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
  max_records_per_thread?: number | null;
  updated_at: string;
}

export interface UpsertEngineJobPolicyPayload {
  enabled?: boolean;
  model_profile_id?: string | null;
  summary_prompt?: string | null;
  token_limit?: number | null;
  round_limit?: number | null;
  target_summary_tokens?: number | null;
  interval_seconds?: number | null;
  max_threads_per_tick?: number | null;
  keep_level0_count?: number | null;
  max_level?: number | null;
  max_records_per_thread?: number | null;
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
  status?: string;
  tenant_id?: string;
  source_id?: string;
  limit?: number;
}

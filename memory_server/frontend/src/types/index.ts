export interface Session {
  id: string;
  user_id: string;
  project_id?: string | null;
  title?: string | null;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  session_id: string;
  role: string;
  content: string;
  summary_status: string;
  created_at: string;
}

export interface SessionSummary {
  id: string;
  session_id: string;
  summary_text: string;
  summary_model: string;
  trigger_type: string;
  source_message_count: number;
  source_estimated_tokens: number;
  status: string;
  level: number;
  rollup_status: string;
  rollup_summary_id?: string | null;
  created_at: string;
}

export interface AiModelConfig {
  id: string;
  user_id: string;
  name: string;
  provider: string;
  model: string;
  base_url?: string | null;
  api_key?: string | null;
  supports_images: number;
  supports_reasoning: number;
  supports_responses: number;
  temperature?: number | null;
  thinking_level?: string | null;
  enabled: number;
  created_at: string;
  updated_at: string;
}

export interface SummaryJobConfig {
  user_id: string;
  enabled: number;
  summary_model_config_id?: string | null;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  max_sessions_per_tick: number;
}

export interface RollupJobConfig {
  user_id: string;
  enabled: number;
  summary_model_config_id?: string | null;
  token_limit: number;
  round_limit: number;
  target_summary_tokens: number;
  job_interval_seconds: number;
  keep_raw_level0_count: number;
  max_level: number;
  max_sessions_per_tick: number;
}

export interface JobRun {
  id: string;
  job_type: string;
  session_id?: string | null;
  status: string;
  trigger_type?: string | null;
  input_count: number;
  output_count: number;
  error_message?: string | null;
  started_at: string;
  finished_at?: string | null;
}

export interface SummaryLevelItem {
  level: number;
  total: number;
  pending: number;
  summarized: number;
}

export interface SummaryGraphNode {
  id: string;
  level: number;
  status: string;
  rollup_status: string;
  rollup_summary_id?: string | null;
  created_at: string;
  summary_excerpt: string;
}

export interface SummaryGraphEdge {
  from: string;
  to: string;
}

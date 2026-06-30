export interface ModelConfigRecord {
  id: string;
  owner_user_id?: string | null;
  owner_username?: string | null;
  owner_display_name?: string | null;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  usage_scenario?: string | null;
  temperature?: number | null;
  max_output_tokens?: number | null;
  thinking_level?: string | null;
  supports_responses: boolean;
  instructions?: string | null;
  request_cwd?: string | null;
  include_prompt_cache_retention: boolean;
  request_body_limit_bytes?: number | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface ModelConfigUsageRecord {
  model_config_id: string;
  task_count: number;
  run_count: number;
}

export interface CreateModelConfigPayload {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  usage_scenario?: string;
  temperature?: number;
  max_output_tokens?: number;
  thinking_level?: string;
  supports_responses?: boolean;
  instructions?: string;
  request_cwd?: string;
  include_prompt_cache_retention?: boolean;
  request_body_limit_bytes?: number;
  enabled?: boolean;
}

export interface UpdateModelConfigPayload extends Partial<CreateModelConfigPayload> {}

export interface RuntimeSettingsRecord {
  id: string;
  task_execution_max_iterations: number;
  execution_timeout_ms?: number | null;
  tool_result_model_max_chars: number;
  tool_results_model_total_max_chars: number;
  created_at: string;
  updated_at: string;
}

export interface UpdateRuntimeSettingsPayload {
  task_execution_max_iterations?: number;
  execution_timeout_ms?: number;
  tool_result_model_max_chars?: number;
  tool_results_model_total_max_chars?: number;
}

export interface PreviewModelCatalogPayload {
  provider: string;
  base_url?: string;
  api_key?: string;
  model?: string;
  supports_responses?: boolean;
}

export interface ProviderModelRecord {
  id: string;
  owned_by?: string | null;
  context_length?: number | null;
  supports_images: boolean;
  supports_video: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  raw?: unknown;
}

export interface ModelCatalogResponse {
  provider_config_id?: string | null;
  provider: string;
  base_url: string;
  source: string;
  fetched_at?: string | null;
  models: ProviderModelRecord[];
  error?: string | null;
}

export interface TestModelConfigPayload {
  prompt?: string;
}

export interface ModelConfigTestResponse {
  ok: boolean;
  model_config_id: string;
  provider: string;
  model: string;
  content?: string | null;
  reasoning?: string | null;
  usage?: unknown;
  response_id?: string | null;
  error?: string | null;
  tested_at: string;
}

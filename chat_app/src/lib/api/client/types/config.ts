export interface McpConfigCreatePayload {
  id: string;
  name: string;
  command: string;
  type: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled: boolean;
  user_id?: string;
}

export interface McpConfigUpdatePayload {
  id?: string;
  name?: string;
  command?: string;
  type?: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled?: boolean;
  userId?: string;
}

export interface McpConfigResponse {
  id: string;
  name: string;
  display_name?: string | null;
  displayName?: string | null;
  command: string;
  type: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled?: boolean;
  readonly?: boolean;
  builtin?: boolean;
  config?: Record<string, unknown> | null;
  user_id?: string | null;
  userId?: string | null;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface AiModelConfigCreatePayload {
  id: string;
  name: string;
  provider: string;
  model?: string;
  thinking_level?: string;
  task_usage_scenario?: string;
  task_thinking_level?: string;
  api_key: string;
  base_url: string;
  enabled: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface AiModelConfigUpdatePayload {
  id?: string;
  name?: string;
  provider?: string;
  model?: string;
  model_name?: string;
  thinking_level?: string;
  task_usage_scenario?: string;
  task_thinking_level?: string;
  api_key?: string;
  clear_api_key?: boolean;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface AiModelConfigResponse {
  id: string;
  name: string;
  provider: string;
  model?: string;
  model_name?: string;
  thinking_level?: string;
  task_usage_scenario?: string | null;
  task_thinking_level?: string | null;
  api_key?: string;
  has_api_key?: boolean;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
  sync_warnings?: string[];
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface AiModelProviderCreatePayload {
  id?: string;
  name: string;
  provider: string;
  api_key: string;
  base_url: string;
  enabled: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface AiModelProviderUpdatePayload {
  id?: string;
  name?: string;
  provider?: string;
  api_key?: string;
  clear_api_key?: boolean;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
}

export interface AiModelProviderResponse {
  id: string;
  name: string;
  provider: string;
  api_key?: string;
  has_api_key?: boolean;
  base_url?: string;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
  last_sync_status?: string | null;
  last_sync_error?: string | null;
  last_synced_at?: string | null;
  imported_model_count?: number;
  sync_warnings?: string[];
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
}

export interface AiModelSettingsResponse {
  user_id: string;
  memory_summary_model_config_id?: string | null;
  memory_summary_thinking_level?: string | null;
  updated_at?: string;
  sync_warnings?: string[];
}

export interface AiModelSettingsUpdatePayload {
  user_id?: string;
  memory_summary_model_config_id?: string | null;
  memory_summary_thinking_level?: string | null;
}

export interface AiProviderModelOptionResponse {
  id: string;
  owned_by?: string | null;
  context_length?: number | null;
  supports_images?: boolean;
  supports_video?: boolean;
  supports_reasoning?: boolean;
  supports_responses?: boolean;
  raw?: unknown;
}

export interface AiProviderModelsResponse {
  provider_config_id: string;
  provider: string;
  base_url: string;
  source: 'live' | 'cache' | 'fallback' | string;
  fetched_at?: string | null;
  models: AiProviderModelOptionResponse[];
  error?: string | null;
}

export interface SystemContextCreatePayload {
  name: string;
  content: string;
  user_id: string;
  app_ids?: string[];
}

export interface SystemContextUpdatePayload {
  name: string;
  content: string;
  app_ids?: string[];
}

export interface SystemContextModelConfigPayload {
  temperature?: number;
}

export interface SystemContextResponse {
  id: string;
  name: string;
  content: string;
  user_id?: string;
  userId?: string;
  is_active?: boolean;
  isActive?: boolean;
  created_at?: string;
  createdAt?: string;
  updated_at?: string;
  updatedAt?: string;
  app_ids?: string[];
}

export interface ActiveSystemContextResponse {
  content: string;
  context: SystemContextResponse | null;
}

export interface PromptQualityReportResponse {
  clarity?: number;
  constraint_completeness?: number;
  conflict_risk?: number;
  verbosity?: number;
  overall?: number;
  warnings?: string[];
}

export interface PromptCandidateResponse {
  title?: string;
  content: string;
  score?: number;
  report?: PromptQualityReportResponse;
}

export interface SystemContextDraftGeneratePayload {
  user_id: string;
  scene: string;
  style?: string;
  language?: string;
  output_format?: string;
  constraints?: string[];
  forbidden?: string[];
  candidate_count?: number;
  model_config_id?: string;
  ai_model_config?: SystemContextModelConfigPayload;
}

export interface SystemContextDraftGenerateResponse {
  candidates?: PromptCandidateResponse[];
}

export interface SystemContextDraftOptimizePayload {
  user_id: string;
  content: string;
  goal?: string;
  keep_intent?: boolean;
  model_config_id?: string;
  ai_model_config?: SystemContextModelConfigPayload;
}

export interface SystemContextDraftOptimizeResponse {
  optimized_content?: string;
  score_after?: number;
  report_after?: PromptQualityReportResponse;
}

export interface SystemContextDraftEvaluatePayload {
  content: string;
  model_config_id?: string;
}

export interface SystemContextDraftEvaluateResponse {
  report?: PromptQualityReportResponse;
}

export interface ApplicationCreatePayload {
  name: string;
  url: string;
  icon_url?: string | null;
  user_id?: string;
}

export interface ApplicationUpdatePayload {
  name?: string;
  url?: string;
  icon_url?: string | null;
}

export interface ApplicationResponse {
  id: string;
  name: string;
  url: string;
  icon_url?: string | null;
  iconUrl?: string | null;
  description?: string | null;
  user_id?: string | null;
  enabled?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface McpConfigResourceResponse {
  success: boolean;
  config: Record<string, unknown> | null;
  alias?: string;
}

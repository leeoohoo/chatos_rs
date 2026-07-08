// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { UnknownRecord } from './common';

export interface SystemContext {
  id: string;
  name: string;
  content: string;
  userId: string;
  isActive: boolean;
  createdAt: Date;
  updatedAt: Date;
  app_ids?: string[];
}

export interface ChatConfig {
  model: string;
  temperature: number;
  systemPrompt: string;
  enableMcp: boolean;
}

export interface McpConfig {
  id: string;
  name: string;
  display_name?: string;
  command: string;
  type: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled: boolean;
  readonly?: boolean;
  builtin?: boolean;
  config?: UnknownRecord | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface AiModelConfig {
  id: string;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  has_api_key: boolean;
  model_name: string;
  thinking_level?: string;
  task_usage_scenario?: string | null;
  task_thinking_level?: string | null;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  sync_warnings?: string[];
  createdAt: Date;
  updatedAt: Date;
}

export interface AiModelProvider {
  id: string;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  has_api_key: boolean;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  last_sync_status?: string | null;
  last_sync_error?: string | null;
  last_synced_at?: string | null;
  imported_model_count: number;
  sync_warnings?: string[];
  createdAt: Date;
  updatedAt: Date;
}

export interface AiModelSettings {
  user_id: string;
  memory_summary_model_config_id?: string | null;
  memory_summary_thinking_level?: string | null;
  updated_at?: string;
  sync_warnings?: string[];
}

export interface AgentConfig {
  id: string;
  name: string;
  description?: string;
  category?: string;
  ai_model_config_id: string;
  enabled: boolean;
  task_runner_agent_account_id?: string | null;
  project_id?: string | null;
  workspace_dir?: string | null;
  role_definition?: string;
  skills?: Array<{ id?: string; name?: string; content?: string }>;
  skill_ids?: string[];
  default_skill_ids?: string[];
  runtime_skills?: Array<{
    id?: string;
    name?: string;
    description?: string | null;
    plugin_source?: string | null;
    source_type?: string;
    source_path?: string | null;
    updated_at?: string | null;
  }>;
  plugin_sources?: string[];
  runtime_plugins?: Array<{
    source?: string;
    name?: string;
    category?: string | null;
    description?: string | null;
    content_summary?: string | null;
    updated_at?: string | null;
  }>;
  mcp_policy?: UnknownRecord | null;
  project_policy?: UnknownRecord | null;
  ui_status?: 'creating';
  createdAt: Date;
  updatedAt: Date;
  app_ids?: string[];
}

export interface AiClientConfig {
  apiKey: string;
  baseUrl?: string;
  model: string;
  temperature: number;
  systemPrompt?: string;
  enableStreaming: boolean;
}

export interface McpToolConfig {
  name: string;
  command: string;
  enabled: boolean;
  timeout: number;
  retryCount: number;
}

export interface Application {
  id: string;
  name: string;
  url: string;
  iconUrl?: string;
  createdAt: Date;
  updatedAt: Date;
}

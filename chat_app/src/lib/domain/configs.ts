// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentConfig,
  AiModelConfig,
  AiModelProvider,
  McpConfig,
  SystemContext,
} from '../../types';
import type {
  AiModelConfigResponse,
  AiModelProviderResponse,
  McpConfigResponse,
  MemoryAgentResponse,
  SessionSummaryResponse,
  SystemContextResponse,
} from '../api/client/types';

export interface SessionSummaryItem {
  id: string;
  summaryText: string;
  summaryModel: string;
  triggerType: string;
  sourceMessageCount: number;
  sourceEstimatedTokens: number;
  status: string;
  errorMessage: string | null;
  level: number;
  createdAt: string;
  updatedAt: string;
}

const toDate = (value?: string): Date => {
  if (!value) {
    return new Date();
  }

  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? new Date() : parsed;
};

export const normalizeAiModelConfig = (config: AiModelConfigResponse): AiModelConfig => {
  const createdAt = config.created_at || config.createdAt;
  const updatedAt = config.updated_at || config.updatedAt || createdAt;

  return {
    id: config.id,
    name: config.name,
    provider: config.provider || 'gpt',
    base_url: config.base_url || '',
    api_key: '',
    has_api_key: config.has_api_key === true || Boolean(config.api_key?.trim()),
    model_name: config.model_name || config.model || '',
    thinking_level: config.thinking_level || undefined,
    task_usage_scenario: config.task_usage_scenario || null,
    task_thinking_level: config.task_thinking_level || null,
    enabled: config.enabled === true,
    supports_images: config.supports_images === true,
    supports_reasoning: config.supports_reasoning === true,
    supports_responses: config.supports_responses === true,
    sync_warnings: config.sync_warnings || [],
    createdAt: toDate(createdAt),
    updatedAt: toDate(updatedAt),
  };
};

export const normalizeAiModelProvider = (provider: AiModelProviderResponse): AiModelProvider => {
  const createdAt = provider.created_at || provider.createdAt;
  const updatedAt = provider.updated_at || provider.updatedAt || createdAt;

  return {
    id: provider.id,
    name: provider.name,
    provider: provider.provider || 'gpt',
    base_url: provider.base_url || '',
    api_key: '',
    has_api_key: provider.has_api_key === true || Boolean(provider.api_key?.trim()),
    enabled: provider.enabled === true,
    supports_images: provider.supports_images === true,
    supports_reasoning: provider.supports_reasoning === true,
    supports_responses: provider.supports_responses === true,
    last_sync_status: provider.last_sync_status || null,
    last_sync_error: provider.last_sync_error || null,
    last_synced_at: provider.last_synced_at || null,
    imported_model_count: Number(provider.imported_model_count || 0),
    sync_warnings: provider.sync_warnings || [],
    createdAt: toDate(createdAt),
    updatedAt: toDate(updatedAt),
  };
};

export const normalizeMcpConfig = (config: McpConfigResponse): McpConfig => {
  const createdAt = config.created_at || config.createdAt;
  const updatedAt = config.updated_at || config.updatedAt || createdAt;

  return {
    id: config.id,
    name: config.name,
    display_name: config.display_name ?? config.displayName ?? undefined,
    command: config.command,
    type: config.type,
    args: config.args ?? null,
    env: config.env ?? null,
    cwd: config.cwd ?? null,
    enabled: config.enabled === true,
    readonly: config.readonly,
    builtin: config.builtin,
    config: config.config ?? undefined,
    createdAt: toDate(createdAt),
    updatedAt: toDate(updatedAt),
  };
};

export const normalizeSystemContext = (context: SystemContextResponse): SystemContext => ({
  id: context.id,
  name: context.name,
  content: context.content,
  userId: context.user_id || context.userId || '',
  isActive: context.is_active === true || context.isActive === true,
  createdAt: toDate(context.created_at || context.createdAt),
  updatedAt: toDate(context.updated_at || context.updatedAt || context.created_at || context.createdAt),
  app_ids: Array.isArray(context.app_ids) ? context.app_ids : [],
});

export const normalizeAgent = (agent: MemoryAgentResponse): AgentConfig => ({
  id: agent.id,
  name: agent.name,
  description: agent.description || '',
  category: agent.category || '',
  ai_model_config_id: '',
  enabled: agent.enabled !== false,
  task_runner_agent_account_id: agent.task_runner_agent_account_id || null,
  project_id: typeof agent.project_policy?.project_id === 'string' ? agent.project_policy.project_id : null,
  workspace_dir: typeof agent.project_policy?.project_root === 'string' ? agent.project_policy.project_root : null,
  app_ids: [],
  role_definition: agent.role_definition || '',
  skills: Array.isArray(agent.skills) ? agent.skills : [],
  skill_ids: Array.isArray(agent.skill_ids) ? agent.skill_ids : [],
  default_skill_ids: Array.isArray(agent.default_skill_ids) ? agent.default_skill_ids : [],
  mcp_policy: agent.mcp_policy || null,
  project_policy: agent.project_policy || null,
  createdAt: toDate(agent.created_at),
  updatedAt: toDate(agent.updated_at || agent.created_at),
});

export const normalizeSessionSummary = (
  item: SessionSummaryResponse | unknown,
): SessionSummaryItem | null => {
  const record = item && typeof item === 'object' && !Array.isArray(item)
    ? item as Record<string, unknown>
    : null;
  const readString = (snakeKey: string, camelKey?: string): string => {
    const snakeValue = record?.[snakeKey];
    const camelValue = camelKey ? record?.[camelKey] : undefined;
    if (typeof snakeValue === 'string') return snakeValue;
    if (typeof camelValue === 'string') return camelValue;
    return '';
  };

  const id = readString('id').trim();
  if (!id) {
    return null;
  }
  const createdAt = readString('created_at', 'createdAt');
  const updatedAt = readString('updated_at', 'updatedAt') || createdAt;

  return {
    id,
    summaryText: readString('summary_text', 'summaryText'),
    summaryModel: readString('summary_model', 'summaryModel'),
    triggerType: readString('trigger_type', 'triggerType'),
    sourceMessageCount: Number(record?.source_message_count ?? record?.sourceMessageCount ?? 0) || 0,
    sourceEstimatedTokens: Number(record?.source_estimated_tokens ?? record?.sourceEstimatedTokens ?? 0) || 0,
    status: readString('status'),
    errorMessage: readString('error_message', 'errorMessage') || null,
    level: Number(record?.level ?? 0) || 0,
    createdAt,
    updatedAt,
  };
};

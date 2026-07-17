// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  LocalModelConfig,
  LocalModelConfigDraft,
  LocalProviderModel,
} from '../api';

export type ModelDraftState = LocalModelConfigDraft & {
  api_key_text: string;
};

export type LocalModelProviderGroup = {
  key: string;
  name: string;
  provider: string;
  prompt_vendor: string;
  base_url: string;
  items: LocalModelConfig[];
  enabled_count: number;
  has_api_key: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
};

export type TaskModelDraft = {
  task_usage_scenario: string;
  task_thinking_level: string;
  temperature: number | null;
  max_output_tokens: number | null;
  enabled: boolean;
};

export function emptyModelDraft(): ModelDraftState {
  return {
    name: '',
    provider: 'gpt',
    prompt_vendor: 'gpt',
    model: '',
    base_url: '',
    api_key_text: '',
    enabled: true,
    supports_images: false,
    supports_reasoning: false,
    supports_responses: true,
    thinking_level: '',
    task_usage_scenario: '',
    task_thinking_level: '',
    temperature: null,
    max_output_tokens: null,
  };
}

export function buildModelConfigPayload(
  draft: ModelDraftState,
  fallbackName = '',
): LocalModelConfigDraft {
  return {
    id: draft.id,
    server_model_config_id: normalizeBlank(draft.server_model_config_id || undefined),
    name: draft.name.trim() || fallbackName,
    provider: normalizeBlank(draft.provider || undefined),
    prompt_vendor: normalizePromptVendor(draft.prompt_vendor),
    model: normalizeBlank(draft.model || undefined),
    base_url: normalizeBlank(draft.base_url || undefined),
    api_key: normalizeBlank(draft.api_key_text),
    clear_api_key: draft.clear_api_key || false,
    enabled: draft.enabled ?? true,
    supports_images: draft.supports_images || false,
    supports_reasoning: draft.supports_reasoning || false,
    supports_responses: draft.supports_responses ?? true,
    thinking_level: normalizeBlank(draft.thinking_level || undefined),
    task_usage_scenario: normalizeBlank(draft.task_usage_scenario || undefined),
    task_thinking_level: normalizeBlank(draft.task_thinking_level || undefined),
    temperature: cleanOptionalNumber(draft.temperature),
    max_output_tokens: cleanOptionalNumber(draft.max_output_tokens),
  };
}

export function buildProviderPreviewPayload(draft: ModelDraftState): LocalModelConfigDraft {
  return {
    name: draft.name.trim() || 'preview',
    id: draft.id,
    server_model_config_id: normalizeBlank(draft.server_model_config_id || undefined),
    provider: normalizeBlank(draft.provider || undefined),
    prompt_vendor: normalizePromptVendor(draft.prompt_vendor),
    base_url: normalizeBlank(draft.base_url || undefined),
    api_key: normalizeBlank(draft.api_key_text),
    clear_api_key: draft.clear_api_key || false,
    enabled: draft.enabled ?? true,
    supports_images: draft.supports_images || false,
    supports_reasoning: draft.supports_reasoning || false,
    supports_responses: draft.supports_responses ?? true,
  };
}

export function findExistingImportedModel(
  items: LocalModelConfig[],
  draft: ModelDraftState,
  baseUrl: string,
  modelId: string,
): LocalModelConfig | undefined {
  const provider = normalizeModelProvider(draft.provider || 'gpt');
  const normalizedBaseUrl = normalizeUrlForCompare(baseUrl || draft.base_url || '');
  return items.find((item) => (
    normalizeModelProvider(item.provider) === provider
    && (item.prompt_vendor || defaultPromptVendor(item.provider))
      === (draft.prompt_vendor || defaultPromptVendor(draft.provider))
    && normalizeUrlForCompare(item.base_url || '') === normalizedBaseUrl
    && item.model === modelId
  ));
}

export function buildImportedModelConfigPayload(
  draft: ModelDraftState,
  model: LocalProviderModel,
  baseUrl: string,
  existing?: LocalModelConfig,
): LocalModelConfigDraft {
  const providerName = draft.name.trim();
  return {
    id: existing?.id,
    server_model_config_id: existing?.server_model_config_id || undefined,
    name: providerName ? `${providerName} / ${model.id}` : model.id,
    provider: normalizeBlank(draft.provider || undefined),
    prompt_vendor: normalizePromptVendor(draft.prompt_vendor),
    model: model.id,
    base_url: normalizeBlank(baseUrl || draft.base_url || undefined),
    api_key: normalizeBlank(draft.api_key_text),
    copy_api_key_from_id: draft.id && !normalizeBlank(draft.api_key_text) ? draft.id : undefined,
    clear_api_key: draft.clear_api_key || false,
    enabled: draft.enabled ?? true,
    supports_images: model.supports_images || draft.supports_images || false,
    supports_reasoning: model.supports_reasoning || draft.supports_reasoning || false,
    supports_responses: model.supports_responses || (draft.supports_responses ?? true),
  };
}

export function groupLocalModelProviders(items: LocalModelConfig[]): LocalModelProviderGroup[] {
  const groups = new Map<string, LocalModelProviderGroup>();
  for (const item of items) {
    const provider = normalizeModelProvider(item.provider || 'gpt');
    const promptVendor = item.prompt_vendor || defaultPromptVendor(provider);
    const baseUrl = normalizeUrlForCompare(item.base_url || '');
    const name = providerGroupNameFromModel(item);
    const key = `${provider}\u0000${promptVendor}\u0000${baseUrl}\u0000${name.toLowerCase()}`;
    const existing = groups.get(key);
    if (existing) {
      existing.items.push(item);
      existing.enabled_count += item.enabled ? 1 : 0;
      existing.has_api_key = existing.has_api_key || item.has_api_key;
      existing.supports_images = existing.supports_images || item.supports_images;
      existing.supports_reasoning = existing.supports_reasoning || item.supports_reasoning;
      existing.supports_responses = existing.supports_responses || item.supports_responses;
      continue;
    }
    groups.set(key, {
      key,
      name,
      provider,
      prompt_vendor: promptVendor,
      base_url: baseUrl,
      items: [item],
      enabled_count: item.enabled ? 1 : 0,
      has_api_key: item.has_api_key,
      supports_images: item.supports_images,
      supports_reasoning: item.supports_reasoning,
      supports_responses: item.supports_responses,
    });
  }
  return Array.from(groups.values())
    .map((group) => ({
      ...group,
      items: [...group.items].sort((left, right) => left.model.localeCompare(right.model)),
    }))
    .sort((left, right) => left.name.localeCompare(right.name));
}

export function providerLabel(provider: string) {
  switch (normalizeModelProvider(provider || 'gpt')) {
    case 'gpt':
      return 'OpenAI';
    case 'deepseek':
      return 'DeepSeek';
    case 'kimi':
      return 'Kimi';
    case 'glm':
      return 'GLM';
    default:
      return provider || 'Provider';
  }
}

export function defaultPromptVendor(provider?: string | null): 'glm' | 'deepseek' | 'gpt' | 'kimi' {
  const normalized = normalizeModelProvider(provider || 'gpt');
  if (normalized === 'deepseek') return 'deepseek';
  if (normalized === 'kimi') return 'kimi';
  if (normalized === 'glm' || normalized === 'zhipu' || normalized === 'zai') return 'glm';
  return 'gpt';
}

export function formatProviderModelOption(model: LocalProviderModel) {
  const details = [
    model.owned_by || null,
    typeof model.context_length === 'number' ? `${model.context_length} ctx` : null,
  ].filter(Boolean);
  return details.length ? `${model.id} (${details.join(' · ')})` : model.id;
}

export function thinkingValueForProvider(provider?: string | null, value?: string | null) {
  return normalizeThinkingLevelForProvider(provider, value) || '';
}

export function normalizeThinkingLevelForProvider(
  provider?: string | null,
  value?: string | null,
): string | null {
  const normalized = (value || '').trim();
  if (!provider || !normalized) {
    return null;
  }
  const options = thinkingOptionsForProvider(provider);
  return options.some((option) => option.value === normalized) ? normalized : null;
}

export function emptyTaskModelDraft(): TaskModelDraft {
  return {
    task_usage_scenario: '',
    task_thinking_level: '',
    temperature: null,
    max_output_tokens: null,
    enabled: true,
  };
}

export function taskDraftFromModel(model: LocalModelConfig): TaskModelDraft {
  return {
    task_usage_scenario: model.task_usage_scenario || '',
    task_thinking_level: model.task_thinking_level || '',
    temperature: model.temperature ?? null,
    max_output_tokens: model.max_output_tokens ?? null,
    enabled: model.enabled,
  };
}

export function taskDraftChanged(model: LocalModelConfig, draft: TaskModelDraft) {
  return (
    (model.task_usage_scenario || '') !== draft.task_usage_scenario.trim()
    || (model.task_thinking_level || '') !== draft.task_thinking_level.trim()
    || (model.temperature ?? null) !== (draft.temperature ?? null)
    || (model.max_output_tokens ?? null) !== (draft.max_output_tokens ?? null)
    || model.enabled !== draft.enabled
  );
}

export function buildTaskModelConfigPayload(
  model: LocalModelConfig,
  draft: TaskModelDraft,
): LocalModelConfigDraft {
  return {
    id: model.id,
    server_model_config_id: model.server_model_config_id || undefined,
    name: model.name,
    provider: model.provider,
    prompt_vendor: model.prompt_vendor || defaultPromptVendor(model.provider),
    model: model.model,
    clear_api_key: false,
    enabled: draft.enabled,
    supports_images: model.supports_images,
    supports_reasoning: model.supports_reasoning,
    supports_responses: model.supports_responses,
    thinking_level: model.thinking_level || undefined,
    task_usage_scenario: draft.task_usage_scenario.trim(),
    task_thinking_level: draft.task_thinking_level.trim(),
    temperature: draft.temperature,
    clear_temperature: draft.temperature == null,
    max_output_tokens: draft.max_output_tokens,
    clear_max_output_tokens: draft.max_output_tokens == null,
  };
}

export function thinkingOptionsForProvider(provider?: string | null) {
  const normalized = (provider || 'gpt').trim().toLowerCase().replace('-', '_');
  if (normalized === 'deepseek') {
    return [
      { value: '', label: '默认' },
      { value: 'none', label: '关闭' },
      { value: 'high', label: 'high' },
      { value: 'max', label: 'max' },
    ];
  }
  if (normalized === 'kimi' || normalized === 'kimik2' || normalized === 'moonshot') {
    return [
      { value: '', label: '默认' },
      { value: 'auto', label: 'auto' },
      { value: 'none', label: '关闭' },
    ];
  }
  if (
    normalized === 'glm'
    || normalized === 'zhipu'
    || normalized === 'zai'
  ) {
    return [
      { value: '', label: '默认' },
      { value: 'none', label: 'none' },
      { value: 'low', label: 'low' },
      { value: 'medium', label: 'medium' },
      { value: 'high', label: 'high' },
      { value: 'xhigh', label: 'xhigh' },
    ];
  }
  return [
    { value: '', label: '默认' },
    { value: 'none', label: 'none' },
    { value: 'minimal', label: 'minimal' },
    { value: 'low', label: 'low' },
    { value: 'medium', label: 'medium' },
    { value: 'high', label: 'high' },
    { value: 'xhigh', label: 'xhigh' },
  ];
}

export function numericInput(value: string): number | null {
  if (!value.trim()) {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function providerGroupNameFromModel(item: LocalModelConfig) {
  const modelSuffix = ` / ${item.model}`;
  const trimmedName = item.name.trim();
  if (trimmedName.endsWith(modelSuffix)) {
    const providerName = trimmedName.slice(0, -modelSuffix.length).trim();
    if (providerName) {
      return providerName;
    }
  }
  return trimmedName || providerLabel(item.provider);
}

function normalizeModelProvider(value: string) {
  const normalized = value.trim().toLowerCase().replace('-', '_');
  if (normalized === 'openai') return 'gpt';
  if (['zhipu', 'zhipuai', 'zai', 'chatglm'].includes(normalized)) return 'glm';
  return normalized;
}

function normalizeUrlForCompare(value: string) {
  return value.trim().replace(/\/+$/, '');
}

function normalizeBlank(value?: string | null): string | undefined {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

function normalizePromptVendor(
  value?: string | null,
): 'glm' | 'deepseek' | 'gpt' | 'kimi' | undefined {
  return ['glm', 'deepseek', 'gpt', 'kimi'].includes(value || '')
    ? value as 'glm' | 'deepseek' | 'gpt' | 'kimi'
    : undefined;
}

function cleanOptionalNumber(value?: number | null): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

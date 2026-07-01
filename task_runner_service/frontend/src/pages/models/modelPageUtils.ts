// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ModelCatalogResponse, ProviderModelRecord } from '../../types';

export type ModelEnabledFilter = 'all' | 'enabled' | 'disabled';

export type ModelFormValues = {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  usage_scenario?: string;
  temperature?: number;
  max_output_tokens?: number;
  thinking_level?: string;
  supports_responses: boolean;
  instructions?: string;
  request_cwd?: string;
  include_prompt_cache_retention: boolean;
  request_body_limit_bytes?: number;
  enabled: boolean;
};

export type SupportedProvider = 'openai' | 'openai_compatible' | 'deepseek' | 'kimik2';

export const SUPPORTED_PROVIDER_OPTIONS: Array<{
  label: SupportedProvider;
  value: SupportedProvider;
}> = [
  { label: 'openai', value: 'openai' },
  { label: 'openai_compatible', value: 'openai_compatible' },
  { label: 'deepseek', value: 'deepseek' },
  { label: 'kimik2', value: 'kimik2' },
];

export const THINKING_LEVEL_OPTIONS: Record<
  SupportedProvider,
  Array<{ label: string; value: string }>
> = {
  openai: [
    { label: 'none', value: 'none' },
    { label: 'minimal', value: 'minimal' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'xhigh', value: 'xhigh' },
  ],
  openai_compatible: [
    { label: 'none', value: 'none' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'xhigh', value: 'xhigh' },
  ],
  deepseek: [
    { label: 'none', value: 'none' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'max', value: 'max' },
  ],
  kimik2: [
    { label: 'none', value: 'none' },
    { label: 'auto', value: 'auto' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'xhigh', value: 'xhigh' },
  ],
};

export function defaultBaseUrlForProvider(provider?: string): string {
  switch (provider) {
    case 'deepseek':
      return 'https://api.deepseek.com';
    case 'kimik2':
      return 'https://api.moonshot.ai/v1';
    case 'openai_compatible':
      return 'https://api.openai.com/v1';
    case 'openai':
    default:
      return 'https://api.openai.com/v1';
  }
}

export function normalizeSupportedProvider(provider?: string): SupportedProvider {
  const value = (provider || '').trim().toLowerCase();
  if (value === 'deepseek') {
    return 'deepseek';
  }
  if (value === 'openai_compatible' || value === 'openai-compatible' || value === 'compatible') {
    return 'openai_compatible';
  }
  if (value === 'kimi' || value === 'kimik2' || value === 'kiminik2' || value === 'moonshot') {
    return 'kimik2';
  }
  return 'openai';
}

export function buildModelOptions({
  modelCatalog,
  currentModel,
  ownerProvider,
  supportsResponses,
}: {
  modelCatalog: ModelCatalogResponse | null;
  currentModel?: string;
  ownerProvider?: string | null;
  supportsResponses?: boolean;
}) {
  const options = new Map<string, ProviderModelRecord>();
  (modelCatalog?.models || []).forEach((item) => {
    options.set(item.id, item);
  });
  if (currentModel && !options.has(currentModel)) {
    options.set(currentModel, {
      id: currentModel,
      owned_by: ownerProvider || null,
      context_length: null,
      supports_images: false,
      supports_video: false,
      supports_reasoning: false,
      supports_responses: supportsResponses ?? false,
      raw: undefined,
    });
  }
  return Array.from(options.values()).map((item) => ({
    label: item.context_length
      ? `${item.id} (${item.context_length.toLocaleString()})`
      : item.id,
    value: item.id,
  }));
}

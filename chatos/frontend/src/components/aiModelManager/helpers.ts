// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { AgentPromptVendor, AiModelConfig, AiModelProvider } from '../../types';
import { generateId } from '../../lib/utils';

import type { AiModelFormData } from './types';

export const AI_MODEL_PROVIDERS = ['gpt', 'deepseek', 'kimi', 'minimax', 'openai_compatible'] as const;
export const AGENT_PROMPT_VENDORS: AgentPromptVendor[] = ['glm', 'deepseek', 'gpt', 'kimi'];

export const defaultAgentPromptVendor = (provider: string): AgentPromptVendor => {
  switch (provider.trim().toLowerCase()) {
    case 'deepseek':
      return 'deepseek';
    case 'kimi':
    case 'kimik2':
    case 'moonshot':
      return 'kimi';
    case 'glm':
    case 'zhipu':
      return 'glm';
    default:
      return 'gpt';
  }
};

const DEFAULT_FORM_DATA: AiModelFormData = {
  name: '',
  provider: 'gpt',
  prompt_vendor: 'gpt',
  base_url: '',
  api_key: '',
  has_stored_api_key: false,
  clear_api_key: false,
  model_name: '',
  thinking_level: '',
  enabled: true,
  supports_images: false,
  supports_reasoning: false,
  supports_responses: false,
};

export const getDefaultAiModelFormData = (): AiModelFormData => ({
  ...DEFAULT_FORM_DATA,
});

export const applyProviderChange = (
  current: AiModelFormData,
  provider: string,
): AiModelFormData => ({
  ...current,
  provider,
  prompt_vendor: defaultAgentPromptVendor(provider),
  thinking_level: provider === 'gpt' ? current.thinking_level : '',
});

export const toAiModelFormData = (config: AiModelConfig | AiModelProvider): AiModelFormData => ({
  name: config.name,
  provider: config.provider || 'gpt',
  prompt_vendor: config.prompt_vendor || defaultAgentPromptVendor(config.provider),
  base_url: config.base_url,
  api_key: '',
  has_stored_api_key: config.has_api_key || Boolean(config.api_key.trim()),
  clear_api_key: false,
  model_name: 'model_name' in config ? config.model_name : '',
  thinking_level: 'thinking_level' in config ? config.thinking_level || '' : '',
  enabled: config.enabled,
  supports_images: config.supports_images ?? false,
  supports_reasoning: config.supports_reasoning ?? false,
  supports_responses: config.supports_responses ?? false,
});

export const buildAiModelConfig = (
  formData: AiModelFormData,
  current?: AiModelConfig | null,
): AiModelConfig => {
  const apiKey = formData.clear_api_key ? '' : formData.api_key.trim();
  const hasApiKey = formData.clear_api_key
    ? false
    : Boolean(apiKey || current?.has_api_key || formData.has_stored_api_key);

  return {
    id: current?.id || generateId(),
    name: formData.name.trim(),
    provider: formData.provider,
    prompt_vendor: formData.prompt_vendor,
    base_url: formData.base_url.trim(),
    api_key: apiKey,
    has_api_key: hasApiKey,
    model_name: formData.model_name.trim(),
    thinking_level: current?.thinking_level,
    task_usage_scenario: current?.task_usage_scenario ?? null,
    task_thinking_level: current?.task_thinking_level ?? null,
    enabled: formData.enabled,
    supports_images: formData.supports_images,
    supports_reasoning: formData.supports_reasoning,
    supports_responses: formData.supports_responses,
    createdAt: current?.createdAt || new Date(),
    updatedAt: new Date(),
  };
};

export const canSubmitAiModelForm = (formData: AiModelFormData): boolean => {
  return canSubmitAiModelFormWithOptions(formData);
};

export const canSubmitAiModelFormWithOptions = (
  formData: AiModelFormData,
  options?: { requireApiKey?: boolean },
): boolean => {
  const requireApiKey = options?.requireApiKey === true;
  return Boolean(
    formData.name.trim()
      && formData.base_url.trim()
      && (!requireApiKey || formData.api_key.trim()),
  );
};

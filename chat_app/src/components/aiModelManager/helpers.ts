import type { AiModelConfig } from '../../types';
import { generateId } from '../../lib/utils';

import type { AiModelFormData } from './types';

export const AI_MODEL_PROVIDERS = ['gpt', 'deepseek', 'kimik2', 'minimax'] as const;

export const AI_MODEL_THINKING_LEVELS = [
  '',
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
] as const;

const DEFAULT_FORM_DATA: AiModelFormData = {
  name: '',
  provider: 'gpt',
  base_url: '',
  api_key: '',
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
  thinking_level: provider === 'gpt' ? current.thinking_level : '',
});

export const toAiModelFormData = (config: AiModelConfig): AiModelFormData => ({
  name: config.name,
  provider: config.provider || 'gpt',
  base_url: config.base_url,
  api_key: config.api_key,
  model_name: config.model_name,
  thinking_level: config.thinking_level || '',
  enabled: config.enabled,
  supports_images: config.supports_images ?? false,
  supports_reasoning: config.supports_reasoning ?? false,
  supports_responses: config.supports_responses ?? false,
});

export const buildAiModelConfig = (
  formData: AiModelFormData,
  current?: AiModelConfig | null,
): AiModelConfig => {
  const normalizedThinking = formData.provider === 'gpt' && formData.thinking_level.trim()
    ? formData.thinking_level.trim()
    : undefined;

  return {
    id: current?.id || generateId(),
    name: formData.name.trim(),
    provider: formData.provider,
    base_url: formData.base_url.trim(),
    api_key: formData.api_key.trim(),
    model_name: formData.model_name.trim(),
    thinking_level: normalizedThinking,
    enabled: formData.enabled,
    supports_images: formData.supports_images,
    supports_reasoning: formData.supports_reasoning,
    supports_responses: formData.supports_responses,
    createdAt: current?.createdAt || new Date(),
    updatedAt: new Date(),
  };
};

export const canSubmitAiModelForm = (formData: AiModelFormData): boolean => {
  return Boolean(
    formData.name.trim()
      && formData.base_url.trim()
      && formData.api_key.trim()
      && formData.model_name.trim(),
  );
};

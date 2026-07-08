// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { AiModelConfig } from '../../types';

import {
  buildAiModelConfig,
  canSubmitAiModelFormWithOptions,
  toAiModelFormData,
} from './helpers';

const buildConfig = (overrides: Partial<AiModelConfig> = {}): AiModelConfig => ({
  id: 'cfg_1',
  name: 'Primary model',
  provider: 'gpt',
  base_url: 'https://api.openai.com/v1',
  api_key: '',
  has_api_key: true,
  model_name: 'gpt-4.1',
  thinking_level: 'medium',
  enabled: true,
  supports_images: true,
  supports_reasoning: true,
  supports_responses: true,
  createdAt: new Date('2026-06-01T00:00:00Z'),
  updatedAt: new Date('2026-06-01T00:00:00Z'),
  ...overrides,
});

describe('aiModelManager helpers', () => {
  it('keeps edit forms blank while tracking saved api keys', () => {
    const formData = toAiModelFormData(buildConfig({
      api_key: 'legacy-secret',
      has_api_key: true,
    }));

    expect(formData.api_key).toBe('');
    expect(formData.has_stored_api_key).toBe(true);
    expect(formData.clear_api_key).toBe(false);
  });

  it('preserves the saved api key indicator when edit submission leaves api key blank', () => {
    const current = buildConfig({ has_api_key: true });
    const formData = toAiModelFormData(current);

    const result = buildAiModelConfig(formData, current);

    expect(result.api_key).toBe('');
    expect(result.has_api_key).toBe(true);
  });

  it('clears the saved api key indicator when requested', () => {
    const current = buildConfig({ has_api_key: true });
    const formData = {
      ...toAiModelFormData(current),
      clear_api_key: true,
    };

    const result = buildAiModelConfig(formData, current);

    expect(result.api_key).toBe('');
    expect(result.has_api_key).toBe(false);
  });

  it('still requires api key during create', () => {
    const formData = toAiModelFormData(buildConfig({
      id: 'new',
      has_api_key: false,
    }));

    expect(canSubmitAiModelFormWithOptions(formData, { requireApiKey: true })).toBe(false);
    expect(canSubmitAiModelFormWithOptions({
      ...formData,
      api_key: 'new-secret',
    }, { requireApiKey: true })).toBe(true);
  });

  it('lets creates omit concrete model because user_service imports provider models', () => {
    const formData = {
      ...toAiModelFormData(buildConfig()),
      model_name: '  ',
      api_key: 'new-secret',
    };

    expect(canSubmitAiModelFormWithOptions(formData, { requireApiKey: true })).toBe(true);
  });
});

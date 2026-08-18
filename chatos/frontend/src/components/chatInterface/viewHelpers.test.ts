// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { AiModelConfig } from '../../types';
import { resolveModelSupportFlags } from './viewHelpers';

const model = (overrides: Partial<AiModelConfig>): AiModelConfig => ({
  id: 'model-1',
  source_provider_id: null,
  name: 'Primary',
  provider: 'openai',
  base_url: 'https://api.example.test/v1',
  api_key: '',
  has_api_key: true,
  model_name: 'gpt-test',
  temperature: null,
  max_output_tokens: null,
  enabled: true,
  supports_images: false,
  supports_reasoning: false,
  supports_responses: true,
  sync_warnings: [],
  createdAt: new Date('2026-08-11T00:00:00Z'),
  updatedAt: new Date('2026-08-11T00:00:00Z'),
  ...overrides,
});

describe('resolveModelSupportFlags', () => {
  it('shows reasoning when the cloud capability flag is enabled', () => {
    expect(resolveModelSupportFlags('model-1', [model({ supports_reasoning: true })]))
      .toEqual({ supportsImages: false, supportsReasoning: true });
  });

  it('keeps the reasoning control visible for legacy models with a thinking level', () => {
    expect(resolveModelSupportFlags('model-1', [model({ thinking_level: 'high' })]))
      .toEqual({ supportsImages: false, supportsReasoning: true });
  });

  it('shows reasoning after the user selects a session thinking level', () => {
    expect(resolveModelSupportFlags('model-1', [model({})], 'high'))
      .toEqual({ supportsImages: false, supportsReasoning: true });
  });

  it('keeps the reasoning control visible for every selected model', () => {
    expect(resolveModelSupportFlags('model-1', [model({})]))
      .toEqual({ supportsImages: false, supportsReasoning: true });
  });
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { AiModelConfig } from '../../types';
import {
  resolveChatModelSelection,
  selectChatModelOptions,
} from './useInputAreaContextModel';

const model = (overrides: Partial<AiModelConfig>): AiModelConfig => ({
  id: 'model-default',
  name: 'my_api / gpt-5.4',
  provider: 'gpt',
  base_url: 'https://api.example.test/v1',
  api_key: '',
  has_api_key: true,
  model_name: 'gpt-5.4',
  enabled: true,
  supports_images: false,
  supports_reasoning: true,
  supports_responses: true,
  createdAt: new Date('2026-07-17T00:00:00Z'),
  updatedAt: new Date('2026-07-17T00:00:00Z'),
  ...overrides,
});

describe('chat model options', () => {
  it('deduplicates legacy no-credential configs in favor of the usable copy', () => {
    const legacy = model({
      id: 'legacy-model',
      base_url: '',
      has_api_key: false,
      updatedAt: new Date('2026-07-10T00:00:00Z'),
    });
    const replacement = model({ id: 'replacement-model' });

    expect(selectChatModelOptions([legacy, replacement])).toEqual([replacement]);
    expect(resolveChatModelSelection(
      [legacy, replacement],
      [replacement],
      legacy.id,
    )).toBe(replacement);
  });

  it('hides an identity when its credential-bearing authoritative copy is disabled', () => {
    const legacy = model({
      id: 'legacy-model',
      base_url: '',
      has_api_key: false,
      updatedAt: new Date('2026-07-10T00:00:00Z'),
    });
    const disabledReplacement = model({
      id: 'replacement-model',
      enabled: false,
    });

    expect(selectChatModelOptions([legacy, disabledReplacement])).toEqual([]);
  });

  it('keeps distinct provider and model identities in their original order', () => {
    const gpt = model({ id: 'gpt-model' });
    const glm = model({
      id: 'glm-model',
      name: 'test / glm-5',
      provider: 'glm',
      model_name: 'glm-5',
    });

    expect(selectChatModelOptions([gpt, glm])).toEqual([gpt, glm]);
  });
});

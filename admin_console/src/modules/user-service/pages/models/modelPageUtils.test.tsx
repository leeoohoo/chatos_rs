// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  buildCreateProviderPayload,
  buildUpdateProviderPayload,
  type ProviderFormValues,
} from './modelPageUtils';

const values: ProviderFormValues = {
  name: ' Gateway ',
  prompt_vendor: 'deepseek',
  api_key: ' secret ',
  base_url: ' https://gateway.example/v1 ',
  enabled: true,
  supports_images: true,
  supports_reasoning: true,
};

describe('model provider payloads', () => {
  it('always saves the unified OpenAI Responses transport', () => {
    const created = buildCreateProviderPayload({
      values,
      isSuperAdmin: false,
      selectedUserId: 'user-1',
    });
    const updated = buildUpdateProviderPayload(values);

    for (const payload of [created, updated]) {
      expect(payload.provider).toBe('gpt');
      expect(payload.prompt_vendor).toBe('deepseek');
      expect(payload.supports_responses).toBe(true);
    }
  });
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { AiModelConfig } from '../../types';
import {
  buildTaskModelPatch,
  defaultModelDraftsFromSettings,
  isCloudConfiguredModel,
  taskModelDraftsFromModels,
} from './cloudAiSettingsModel';

const buildModel = (overrides: Partial<AiModelConfig> = {}): AiModelConfig => ({
  id: 'model_1',
  name: 'Cloud model',
  provider: 'openai',
  base_url: 'https://api.openai.com/v1',
  api_key: '',
  has_api_key: true,
  model_name: 'gpt-5',
  enabled: true,
  supports_images: false,
  supports_reasoning: true,
  supports_responses: true,
  createdAt: new Date('2026-07-01T00:00:00Z'),
  updatedAt: new Date('2026-07-01T00:00:00Z'),
  ...overrides,
});

describe('cloud AI settings model', () => {
  it('maps the environment initialization default independently', () => {
    expect(defaultModelDraftsFromSettings({
      user_id: 'user-1',
      project_management_agent_model_config_id: 'project-model',
      environment_initialization_model_config_id: 'environment-model',
      environment_initialization_thinking_level: 'high',
    }).environment).toEqual({
      modelId: 'environment-model',
      thinking: 'high',
    });
  });

  it('excludes local metadata records without cloud connection details', () => {
    expect(isCloudConfiguredModel(buildModel())).toBe(true);
    expect(isCloudConfiguredModel(buildModel({ base_url: '' }))).toBe(false);
    expect(isCloudConfiguredModel(buildModel({ has_api_key: false }))).toBe(false);
  });

  it('builds task settings updates and explicit clear fields', () => {
    const model = buildModel({
      task_usage_scenario: 'coding',
      task_thinking_level: 'medium',
      temperature: 0.5,
      max_output_tokens: 4096,
    });
    const draft = taskModelDraftsFromModels([model])[model.id];

    expect(buildTaskModelPatch(model, {
      ...draft,
      usage: 'review',
      thinking: 'high',
      temperature: '',
      maxOutputTokens: '',
      enabled: false,
    })).toEqual({
      task_usage_scenario: 'review',
      task_thinking_level: 'high',
      enabled: false,
      clear_temperature: true,
      clear_max_output_tokens: true,
    });
  });

  it('rejects invalid temperature and max token values', () => {
    const model = buildModel();
    const draft = taskModelDraftsFromModels([model])[model.id];

    expect(() => buildTaskModelPatch(model, { ...draft, temperature: '2.1' }))
      .toThrow('invalid_temperature');
    expect(() => buildTaskModelPatch(model, { ...draft, maxOutputTokens: '1.5' }))
      .toThrow('invalid_max_output_tokens');
  });
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { AiModelConfigUpdatePayload } from '../../lib/api/client/types';
import type { AiModelConfig, AiModelSettings } from '../../types';
import type {
  DefaultModelDrafts,
  TaskModelDraft,
  TaskModelDrafts,
} from './cloudAiSettingsTypes';

export const isCloudConfiguredModel = (model: AiModelConfig): boolean => (
  model.has_api_key
  && Boolean(model.base_url.trim())
  && Boolean(model.model_name.trim())
);

export const isCloudRunnableModel = (model: AiModelConfig): boolean => (
  model.enabled && isCloudConfiguredModel(model)
);

export const defaultModelDraftsFromSettings = (
  settings: AiModelSettings,
): DefaultModelDrafts => ({
  memory: {
    modelId: settings.memory_summary_model_config_id || '',
    thinking: settings.memory_summary_thinking_level || '',
  },
  project: {
    modelId: settings.project_management_agent_model_config_id || '',
    thinking: settings.project_management_agent_thinking_level || '',
  },
  environment: {
    modelId: settings.environment_initialization_model_config_id || '',
    thinking: settings.environment_initialization_thinking_level || '',
  },
});

export const taskModelDraftsFromModels = (models: AiModelConfig[]): TaskModelDrafts => {
  const drafts: TaskModelDrafts = {};
  models.forEach((model) => {
    drafts[model.id] = {
      usage: model.task_usage_scenario || '',
      thinking: model.task_thinking_level || '',
      temperature: model.temperature == null ? '' : String(model.temperature),
      maxOutputTokens: model.max_output_tokens == null ? '' : String(model.max_output_tokens),
      enabled: model.enabled,
    };
  });
  return drafts;
};

export const buildTaskModelPatch = (
  model: AiModelConfig,
  draft: TaskModelDraft,
): AiModelConfigUpdatePayload => {
  const patch: AiModelConfigUpdatePayload = {};
  const usage = draft.usage.trim();
  const thinking = draft.thinking.trim();
  const temperatureText = draft.temperature.trim();
  const maxTokensText = draft.maxOutputTokens.trim();

  if ((model.task_usage_scenario || '') !== usage) patch.task_usage_scenario = usage;
  if ((model.task_thinking_level || '') !== thinking) patch.task_thinking_level = thinking;
  if (model.enabled !== draft.enabled) patch.enabled = draft.enabled;

  if (!temperatureText) {
    if (model.temperature != null) patch.clear_temperature = true;
  } else {
    const temperature = Number(temperatureText);
    if (!Number.isFinite(temperature) || temperature < 0 || temperature > 2) {
      throw new Error('invalid_temperature');
    }
    if (model.temperature !== temperature) patch.temperature = temperature;
  }

  if (!maxTokensText) {
    if (model.max_output_tokens != null) patch.clear_max_output_tokens = true;
  } else {
    const maxOutputTokens = Number(maxTokensText);
    if (!Number.isInteger(maxOutputTokens) || maxOutputTokens <= 0) {
      throw new Error('invalid_max_output_tokens');
    }
    if (model.max_output_tokens !== maxOutputTokens) {
      patch.max_output_tokens = maxOutputTokens;
    }
  }
  return patch;
};

import type {
  EngineJobPolicy,
  EngineModelProfile,
  EngineSource,
  UpsertEngineJobPolicyPayload,
  UpsertEngineModelProfilePayload,
} from '../../types';
import type {
  ModelFormValues,
  PolicyFormValues,
  SourceFormValues,
  SourcePayloadResult,
} from '../types';

import { numberOrNull, textOrUndefined } from './common';

export function buildModelPayload(
  values: ModelFormValues,
): UpsertEngineModelProfilePayload {
  return {
    name: values.name.trim(),
    provider: values.provider.trim(),
    model: values.model.trim(),
    base_url: textOrUndefined(values.base_url) ?? null,
    api_key: textOrUndefined(values.api_key),
    supports_images: values.supports_images,
    supports_reasoning: values.supports_reasoning,
    supports_responses: values.supports_responses,
    temperature: numberOrNull(values.temperature),
    thinking_level: textOrUndefined(values.thinking_level) ?? null,
    is_default: values.is_default,
    enabled: values.enabled,
  };
}

export function buildPolicyPayload(
  values: PolicyFormValues,
): UpsertEngineJobPolicyPayload {
  const summaryPromptZh = textOrUndefined(values.summary_prompt_zh) ?? null;
  const summaryPromptEn = textOrUndefined(values.summary_prompt_en) ?? null;
  const summaryPromptLanguage = values.summary_prompt_language ?? 'zh';
  const rollupPromptZh = textOrUndefined(values.rollup_summary_prompt_zh) ?? null;
  const rollupPromptEn = textOrUndefined(values.rollup_summary_prompt_en) ?? null;
  const rollupPromptLanguage = values.rollup_summary_prompt_language ?? 'zh';

  const summaryPrompt =
    summaryPromptLanguage === 'en'
      ? summaryPromptEn ?? summaryPromptZh
      : summaryPromptZh ?? summaryPromptEn;
  const rollupSummaryPrompt =
    rollupPromptLanguage === 'en'
      ? rollupPromptEn ?? rollupPromptZh
      : rollupPromptZh ?? rollupPromptEn;

  return {
    enabled: values.enabled,
    model_profile_id: textOrUndefined(values.model_profile_id) ?? null,
    summary_prompt: summaryPrompt ?? null,
    summary_prompt_zh: summaryPromptZh,
    summary_prompt_en: summaryPromptEn,
    summary_prompt_language: summaryPromptLanguage,
    rollup_summary_prompt: rollupSummaryPrompt ?? null,
    rollup_summary_prompt_zh: rollupPromptZh,
    rollup_summary_prompt_en: rollupPromptEn,
    rollup_summary_prompt_language: rollupPromptLanguage,
    token_limit: numberOrNull(values.token_limit),
    target_summary_tokens: numberOrNull(values.target_summary_tokens),
    interval_seconds: numberOrNull(values.interval_seconds),
    max_threads_per_tick: numberOrNull(values.max_threads_per_tick),
    count_limit: numberOrNull(values.count_limit),
    keep_level0_count: numberOrNull(values.keep_level0_count),
    max_level: numberOrNull(values.max_level),
  };
}

export function buildSourcePayload(values: SourceFormValues): SourcePayloadResult {
  return {
    sourceId: values.source_id.trim(),
    payload: {
      tenant_id: textOrUndefined(values.tenant_id) ?? null,
      source_type: 'sdk_system',
      name: values.name.trim(),
      description: textOrUndefined(values.description) ?? null,
      config: null,
      sdk_enabled: true,
      status: values.enabled ? 'active' : 'disabled',
    },
  };
}

export function modelFormInitialValues(
  model?: EngineModelProfile | null,
): ModelFormValues {
  return {
    name: model?.name ?? '',
    provider: model?.provider ?? '',
    model: model?.model ?? '',
    base_url: model?.base_url ?? '',
    api_key: '',
    supports_images: model?.supports_images ?? false,
    supports_reasoning: model?.supports_reasoning ?? false,
    supports_responses: model?.supports_responses ?? false,
    temperature: model?.temperature ?? null,
    thinking_level: model?.thinking_level ?? '',
    is_default: model?.is_default ?? false,
    enabled: model?.enabled ?? true,
  };
}

export function policyFormInitialValues(policy: EngineJobPolicy): PolicyFormValues {
  return {
    enabled: policy.enabled,
    model_profile_id: policy.model_profile_id ?? undefined,
    summary_prompt: policy.summary_prompt ?? '',
    summary_prompt_zh: policy.summary_prompt_zh ?? policy.summary_prompt ?? '',
    summary_prompt_en: policy.summary_prompt_en ?? '',
    summary_prompt_language: policy.summary_prompt_language ?? 'zh',
    rollup_summary_prompt: policy.rollup_summary_prompt ?? '',
    rollup_summary_prompt_zh:
      policy.rollup_summary_prompt_zh ?? policy.rollup_summary_prompt ?? '',
    rollup_summary_prompt_en: policy.rollup_summary_prompt_en ?? '',
    rollup_summary_prompt_language: policy.rollup_summary_prompt_language ?? 'zh',
    token_limit: policy.token_limit ?? null,
    target_summary_tokens: policy.target_summary_tokens ?? null,
    interval_seconds: policy.interval_seconds ?? null,
    max_threads_per_tick: policy.max_threads_per_tick ?? null,
    count_limit: policy.count_limit ?? null,
    keep_level0_count: policy.keep_level0_count ?? null,
    max_level: policy.max_level ?? null,
  };
}

export function sourceFormInitialValues(source?: EngineSource | null): SourceFormValues {
  return {
    tenant_id: source?.tenant_id ?? '',
    source_id: source?.source_id ?? '',
    name: source?.name ?? '',
    description: source?.description ?? '',
    enabled: (source?.status ?? 'active') === 'active',
  };
}

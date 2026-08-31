// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  DashboardOverview,
  EngineJobPolicy,
  EngineModelProfile,
  EngineSource,
  GenerateJobPolicyPromptPayload,
  GenerateJobPolicyPromptResult,
  JobRunsBundle,
  JobRunQuery,
  RotateSourceSecretResponse,
  UpsertEngineJobPolicyPayload,
  UpsertEngineModelProfilePayload,
  UpsertEngineSourcePayload,
} from '../types';

import { client } from './client';

function normalizeModelProfile(data: Partial<EngineModelProfile>): EngineModelProfile {
  return {
    id: data.id ?? '',
    owner_user_id: data.owner_user_id ?? null,
    owner_username: data.owner_username ?? null,
    name: data.name ?? '',
    provider: data.provider ?? '',
    model: data.model ?? '',
    base_url: data.base_url ?? null,
    api_key: data.api_key ?? null,
    supports_images: data.supports_images ?? false,
    supports_reasoning: data.supports_reasoning ?? false,
    supports_responses: data.supports_responses ?? false,
    temperature: data.temperature ?? null,
    thinking_level: data.thinking_level ?? null,
    is_default: data.is_default ?? false,
    enabled: data.enabled ?? true,
    created_at: data.created_at ?? '',
    updated_at: data.updated_at ?? '',
  };
}

function normalizeSource(data: Partial<EngineSource>): EngineSource {
  return {
    id: data.id ?? '',
    tenant_id: data.tenant_id ?? null,
    source_id: data.source_id ?? '',
    source_type: data.source_type ?? '',
    name: data.name ?? '',
    description: data.description ?? null,
    config: data.config ?? null,
    status: data.status ?? 'active',
    sdk_enabled: data.sdk_enabled ?? false,
    secret_key_hint: data.secret_key_hint ?? null,
    key_last_rotated_at: data.key_last_rotated_at ?? null,
    created_at: data.created_at ?? '',
    updated_at: data.updated_at ?? '',
  };
}

function normalizeJobPolicy(data: Partial<EngineJobPolicy>): EngineJobPolicy {
  return {
    job_type: data.job_type ?? '',
    enabled: data.enabled ?? true,
    model_profile_id: data.model_profile_id ?? null,
    summary_prompt: data.summary_prompt ?? null,
    summary_prompt_zh: data.summary_prompt_zh ?? data.summary_prompt ?? null,
    summary_prompt_en: data.summary_prompt_en ?? null,
    summary_prompt_language: data.summary_prompt_language ?? 'zh',
    rollup_summary_prompt: data.rollup_summary_prompt ?? null,
    rollup_summary_prompt_zh:
      data.rollup_summary_prompt_zh ?? data.rollup_summary_prompt ?? null,
    rollup_summary_prompt_en: data.rollup_summary_prompt_en ?? null,
    rollup_summary_prompt_language: data.rollup_summary_prompt_language ?? 'zh',
    token_limit: data.token_limit ?? null,
    target_summary_tokens: data.target_summary_tokens ?? null,
    interval_seconds: data.interval_seconds ?? null,
    max_threads_per_tick: data.max_threads_per_tick ?? null,
    count_limit: data.count_limit ?? null,
    keep_level0_count: data.keep_level0_count ?? null,
    max_level: data.max_level ?? null,
    updated_at: data.updated_at ?? '',
  };
}

export const adminApi = {
  async listModelProfiles(): Promise<EngineModelProfile[]> {
    const { data } = await client.get('/admin/model-profiles');
    return (data.items ?? []).map((item: Partial<EngineModelProfile>) =>
      normalizeModelProfile(item),
    );
  },

  async createModelProfile(payload: UpsertEngineModelProfilePayload): Promise<EngineModelProfile> {
    const { data } = await client.post('/admin/model-profiles', payload);
    return normalizeModelProfile(data);
  },

  async updateModelProfile(
    modelId: string,
    payload: UpsertEngineModelProfilePayload,
  ): Promise<EngineModelProfile> {
    const { data } = await client.put(
      `/admin/model-profiles/${encodeURIComponent(modelId)}`,
      payload,
    );
    return normalizeModelProfile(data);
  },

  async deleteModelProfile(modelId: string): Promise<void> {
    await client.delete(`/admin/model-profiles/${encodeURIComponent(modelId)}`);
  },

  async listSources(): Promise<EngineSource[]> {
    const { data } = await client.get('/admin/sources', {
      params: { limit: 500 },
    });
    return (data.items ?? []).map((item: Partial<EngineSource>) => normalizeSource(item));
  },

  async upsertSource(
    sourceId: string,
    payload: UpsertEngineSourcePayload,
  ): Promise<EngineSource> {
    const { data } = await client.put(`/admin/sources/${encodeURIComponent(sourceId)}`, payload);
    return normalizeSource(data);
  },

  async rotateSourceSecret(
    sourceId: string,
    tenantId?: string | null,
  ): Promise<RotateSourceSecretResponse> {
    const { data } = await client.post(
      `/admin/sources/${encodeURIComponent(sourceId)}/rotate-key`,
      null,
      { params: { tenant_id: tenantId || undefined } },
    );
    return {
      source: normalizeSource(data.source ?? {}),
      secret_key: data.secret_key ?? '',
    };
  },

  async listJobPolicies(): Promise<EngineJobPolicy[]> {
    const { data } = await client.get('/admin/job-policies');
    return (data.items ?? []).map((item: Partial<EngineJobPolicy>) =>
      normalizeJobPolicy(item),
    );
  },

  async updateJobPolicy(
    jobType: string,
    payload: UpsertEngineJobPolicyPayload,
  ): Promise<EngineJobPolicy> {
    const { data } = await client.put(
      `/admin/job-policies/${encodeURIComponent(jobType)}`,
      payload,
    );
    return normalizeJobPolicy(data);
  },

  async generateJobPolicyPrompt(
    jobType: string,
    payload: GenerateJobPolicyPromptPayload,
  ): Promise<GenerateJobPolicyPromptResult> {
    const { data } = await client.post(
      `/admin/job-policies/${encodeURIComponent(jobType)}/generate-prompt`,
      payload,
    );
    return {
      prompt_zh: data.prompt_zh ?? '',
      prompt_en: data.prompt_en ?? '',
    };
  },

  async getDashboardOverview(): Promise<DashboardOverview> {
    const { data } = await client.get('/admin/dashboard/overview');
    return {
      source_count: Number(data.source_count ?? 0),
      model_count: Number(data.model_count ?? 0),
      policy_count: Number(data.policy_count ?? 0),
      job_stats: data.job_stats ?? {},
    };
  },

  async getJobRunsBundle(params?: JobRunQuery): Promise<JobRunsBundle> {
    const { data } = await client.get('/admin/job-runs/bundle', {
      params: {
        job_type: params?.job_type,
        trigger_type: params?.trigger_type,
        thread_id: params?.thread_id,
        status: params?.status,
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
        limit: params?.limit ?? 200,
      },
    });
    return {
      thread_runs: data.thread_runs ?? [],
      scheduler_runs: data.scheduler_runs ?? [],
    };
  },
};

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentDirectoryQuery,
  DashboardOverview,
  EngineAgentDirectoryItem,
  EngineAgentConversation,
  EngineAgentProject,
  EngineJobPolicy,
  EngineSource,
  JobRunsBundle,
  JobRunQuery,
  RotateSourceSecretResponse,
  DerivationLineage,
  ComposeDryRunRequest,
  ComposeDryRunResponse,
  PartitionRebuildBacklogItem,
  RearmRebuildRequest,
  ResummarizeCoveredRangeRequest,
  ResummarizeCoveredRangeResponse,
  RecordCoverage,
  ReliabilityOperatorAction,
  ReliabilityQueueItem,
  ReliabilityMigrationRun,
  ReliabilityDashboard,
  ReliabilityScope,
  RetireSummaryRequest,
  RestoreRecordPendingRequest,
  RearmReliabilityQueueRequest,
  UpsertEngineSourcePayload,
} from '../types';

import { client } from './client';

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
    model_config_id: data.model_config_id ?? null,
    provider: data.provider ?? null,
    model: data.model ?? null,
    model_capability_revision: data.model_capability_revision ?? null,
    context_window_tokens: data.context_window_tokens ?? null,
    output_reserve_tokens: data.output_reserve_tokens ?? null,
    max_model_input_tokens: data.max_model_input_tokens ?? null,
    updated_at: data.updated_at ?? '',
  };
}

export const adminApi = {
  async getReliabilityDashboard(params: ReliabilityScope): Promise<ReliabilityDashboard> {
    const { data } = await client.get('/admin/reliability/dashboard', { params });
    return data;
  },

  async composeReliabilityDryRun(
    payload: ComposeDryRunRequest,
  ): Promise<ComposeDryRunResponse> {
    const { data } = await client.post('/admin/reliability/compose-dry-run', payload);
    return data;
  },

  async listReliabilityQueues(params: ReliabilityScope): Promise<ReliabilityQueueItem[]> {
    const { data } = await client.get('/admin/reliability/queues', { params });
    return data ?? [];
  },

  async listReliabilityOperatorActions(
    params: ReliabilityScope,
  ): Promise<ReliabilityOperatorAction[]> {
    const { data } = await client.get('/admin/reliability/operator-actions', { params });
    return data ?? [];
  },

  async rearmPartitionRebuild(payload: RearmRebuildRequest): Promise<void> {
    await client.post('/admin/reliability/rebuild-backlog/rearm', payload);
  },

  async resummarizeCoveredRange(
    payload: ResummarizeCoveredRangeRequest,
  ): Promise<ResummarizeCoveredRangeResponse> {
    const { data } = await client.post('/admin/reliability/resummarize', payload);
    return data;
  },

  async retireSummary(payload: RetireSummaryRequest): Promise<void> {
    await client.post('/admin/reliability/retire-summary', payload);
  },

  async restoreRecordPending(payload: RestoreRecordPendingRequest): Promise<void> {
    await client.post('/admin/reliability/restore-pending', payload);
  },

  async rearmReliabilityQueue(payload: RearmReliabilityQueueRequest): Promise<void> {
    await client.post('/admin/reliability/queues/rearm', payload);
  },

  async listReliabilityMigrations(
    params: ReliabilityScope,
  ): Promise<ReliabilityMigrationRun[]> {
    const { data } = await client.get('/admin/reliability/migrations', { params });
    return data ?? [];
  },

  async listPartitionRebuildBacklog(
    params: ReliabilityScope,
  ): Promise<PartitionRebuildBacklogItem[]> {
    const { data } = await client.get('/admin/reliability/rebuild-backlog', { params });
    return data ?? [];
  },

  async getRecordCoverage(
    recordId: string,
    params: ReliabilityScope,
  ): Promise<RecordCoverage[]> {
    const { data } = await client.get(
      `/admin/reliability/records/${encodeURIComponent(recordId)}/coverage`,
      { params },
    );
    return data ?? [];
  },

  async getOutputLineage(
    outputKind: 'thread_summary' | 'subject_memory',
    outputId: string,
    params: ReliabilityScope,
  ): Promise<DerivationLineage[]> {
    const { data } = await client.get(
      `/admin/reliability/outputs/${outputKind}/${encodeURIComponent(outputId)}/lineage`,
      { params },
    );
    return data ?? [];
  },

  async listAgents(params?: AgentDirectoryQuery): Promise<EngineAgentDirectoryItem[]> {
    const { data } = await client.get('/admin/agents', {
      params: {
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
        agent_id: params?.agent_id,
        limit: params?.limit ?? 100,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async listAgentProjects(
    agentSubjectId: string,
    params: { tenant_id: string; source_id: string },
  ): Promise<EngineAgentProject[]> {
    const { data } = await client.get(
      `/admin/agents/${encodeURIComponent(agentSubjectId)}/projects`,
      { params },
    );
    return data.items ?? [];
  },

  async listAgentConversations(
    agentSubjectId: string,
    params: { tenant_id: string; source_id: string },
  ): Promise<EngineAgentConversation[]> {
    const { data } = await client.get(
      `/admin/agents/${encodeURIComponent(agentSubjectId)}/conversations`,
      { params },
    );
    return data.items ?? [];
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

  async getDashboardOverview(): Promise<DashboardOverview> {
    const { data } = await client.get('/admin/dashboard/overview');
    return {
      source_count: Number(data.source_count ?? 0),
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

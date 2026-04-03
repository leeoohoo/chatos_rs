import type { AxiosInstance } from 'axios';

import type {
  AgentMemoryJobConfig,
  JobRun,
  RollupJobConfig,
  SummaryGraphEdge,
  SummaryGraphNode,
  SummaryJobConfig,
  SummaryLevelItem,
  SessionSummary,
  TaskExecutionRollupJobConfig,
  TaskExecutionSummaryJobConfig,
} from '../types';

function normalizeOptionalPrompt(value: string | null | undefined): string | null | undefined {
  if (value === undefined) {
    return undefined;
  }
  if (value === null) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function buildJobConfigsApi(client: AxiosInstance) {
  return {
    async listSummaries(
      sessionId: string,
      params?: { status?: string; level?: number; limit?: number; offset?: number },
    ): Promise<SessionSummary[]> {
      const { data } = await client.get(`/sessions/${sessionId}/summaries`, {
        params: {
          status: params?.status,
          level: params?.level,
          limit: params?.limit ?? 200,
          offset: params?.offset ?? 0,
        },
      });
      return data.items ?? [];
    },

    async listSummaryLevels(sessionId: string): Promise<SummaryLevelItem[]> {
      const { data } = await client.get(`/sessions/${sessionId}/summaries/levels`);
      return data.items ?? [];
    },

    async getSummaryGraph(
      sessionId: string,
    ): Promise<{ nodes: SummaryGraphNode[]; edges: SummaryGraphEdge[] }> {
      const { data } = await client.get(`/sessions/${sessionId}/summaries/graph`);
      return {
        nodes: data.nodes ?? [],
        edges: data.edges ?? [],
      };
    },

    async getSummaryJobConfig(userId: string): Promise<SummaryJobConfig | null> {
      const { data } = await client.get('/configs/summary-job', { params: { user_id: userId } });
      return data ?? null;
    },

    async saveSummaryJobConfig(payload: Partial<SummaryJobConfig> & { user_id: string }) {
      const req = {
        user_id: payload.user_id,
        enabled:
          payload.enabled === undefined
            ? undefined
            : typeof payload.enabled === 'number'
              ? payload.enabled === 1
              : Boolean(payload.enabled),
        summary_model_config_id: payload.summary_model_config_id,
        summary_prompt: normalizeOptionalPrompt(payload.summary_prompt),
        token_limit: payload.token_limit,
        round_limit: payload.round_limit,
        target_summary_tokens: payload.target_summary_tokens,
        job_interval_seconds: payload.job_interval_seconds,
        max_sessions_per_tick: payload.max_sessions_per_tick,
      };
      const { data } = await client.put('/configs/summary-job', req);
      return data;
    },

    async getRollupJobConfig(userId: string): Promise<RollupJobConfig | null> {
      const { data } = await client.get('/configs/summary-rollup-job', {
        params: { user_id: userId },
      });
      return data ?? null;
    },

    async saveRollupJobConfig(payload: Partial<RollupJobConfig> & { user_id: string }) {
      const req = {
        user_id: payload.user_id,
        enabled:
          payload.enabled === undefined
            ? undefined
            : typeof payload.enabled === 'number'
              ? payload.enabled === 1
              : Boolean(payload.enabled),
        summary_model_config_id: payload.summary_model_config_id,
        summary_prompt: normalizeOptionalPrompt(payload.summary_prompt),
        token_limit: payload.token_limit,
        round_limit: payload.round_limit,
        target_summary_tokens: payload.target_summary_tokens,
        job_interval_seconds: payload.job_interval_seconds,
        keep_raw_level0_count: payload.keep_raw_level0_count,
        max_level: payload.max_level,
        max_sessions_per_tick: payload.max_sessions_per_tick,
      };
      const { data } = await client.put('/configs/summary-rollup-job', req);
      return data;
    },

    async getTaskExecutionSummaryJobConfig(
      userId: string,
    ): Promise<TaskExecutionSummaryJobConfig | null> {
      const { data } = await client.get('/configs/task-execution-summary-job', {
        params: { user_id: userId },
      });
      return data ?? null;
    },

    async saveTaskExecutionSummaryJobConfig(
      payload: Partial<TaskExecutionSummaryJobConfig> & { user_id: string },
    ) {
      const req = {
        user_id: payload.user_id,
        enabled:
          payload.enabled === undefined
            ? undefined
            : typeof payload.enabled === 'number'
              ? payload.enabled === 1
              : Boolean(payload.enabled),
        summary_model_config_id: payload.summary_model_config_id,
        summary_prompt: normalizeOptionalPrompt(payload.summary_prompt),
        token_limit: payload.token_limit,
        round_limit: payload.round_limit,
        target_summary_tokens: payload.target_summary_tokens,
        job_interval_seconds: payload.job_interval_seconds,
        max_scopes_per_tick: payload.max_scopes_per_tick,
      };
      const { data } = await client.put('/configs/task-execution-summary-job', req);
      return data;
    },

    async getTaskExecutionRollupJobConfig(
      userId: string,
    ): Promise<TaskExecutionRollupJobConfig | null> {
      const { data } = await client.get('/configs/task-execution-rollup-job', {
        params: { user_id: userId },
      });
      return data ?? null;
    },

    async saveTaskExecutionRollupJobConfig(
      payload: Partial<TaskExecutionRollupJobConfig> & { user_id: string },
    ) {
      const req = {
        user_id: payload.user_id,
        enabled:
          payload.enabled === undefined
            ? undefined
            : typeof payload.enabled === 'number'
              ? payload.enabled === 1
              : Boolean(payload.enabled),
        summary_model_config_id: payload.summary_model_config_id,
        summary_prompt: normalizeOptionalPrompt(payload.summary_prompt),
        token_limit: payload.token_limit,
        round_limit: payload.round_limit,
        target_summary_tokens: payload.target_summary_tokens,
        job_interval_seconds: payload.job_interval_seconds,
        keep_raw_level0_count: payload.keep_raw_level0_count,
        max_level: payload.max_level,
        max_scopes_per_tick: payload.max_scopes_per_tick,
      };
      const { data } = await client.put('/configs/task-execution-rollup-job', req);
      return data;
    },

    async runSummaryOnce(userId: string, sessionId?: string) {
      const { data } = await client.post('/jobs/summary/run-once', {
        user_id: userId,
        session_id: sessionId,
      });
      return data;
    },

    async runRollupOnce(userId: string) {
      const { data } = await client.post('/jobs/summary-rollup/run-once', {
        user_id: userId,
      });
      return data;
    },

    async getAgentMemoryJobConfig(userId: string): Promise<AgentMemoryJobConfig | null> {
      const { data } = await client.get('/configs/agent-memory-job', { params: { user_id: userId } });
      return data ?? null;
    },

    async saveAgentMemoryJobConfig(
      payload: Partial<AgentMemoryJobConfig> & { user_id: string },
    ) {
      const req = {
        user_id: payload.user_id,
        enabled:
          payload.enabled === undefined
            ? undefined
            : typeof payload.enabled === 'number'
              ? payload.enabled === 1
              : Boolean(payload.enabled),
        summary_model_config_id: payload.summary_model_config_id,
        summary_prompt: normalizeOptionalPrompt(payload.summary_prompt),
        token_limit: payload.token_limit,
        round_limit: payload.round_limit,
        target_summary_tokens: payload.target_summary_tokens,
        job_interval_seconds: payload.job_interval_seconds,
        keep_raw_level0_count: payload.keep_raw_level0_count,
        max_level: payload.max_level,
        max_agents_per_tick: payload.max_agents_per_tick,
      };
      const { data } = await client.put('/configs/agent-memory-job', req);
      return data;
    },

    async runAgentMemoryOnce(userId: string) {
      const { data } = await client.post('/jobs/agent-memory/run-once', {
        user_id: userId,
      });
      return data;
    },

    async runTaskExecutionSummaryOnce(userId: string) {
      const { data } = await client.post('/jobs/task-execution-summary/run-once', {
        user_id: userId,
      });
      return data;
    },

    async runTaskExecutionRollupOnce(userId: string) {
      const { data } = await client.post('/jobs/task-execution-rollup/run-once', {
        user_id: userId,
      });
      return data;
    },

    async listJobRuns(params?: {
      job_type?: string;
      session_id?: string;
      status?: string;
      limit?: number;
    }): Promise<JobRun[]> {
      const { data } = await client.get('/jobs/runs', {
        params: {
          limit: params?.limit ?? 200,
          job_type: params?.job_type,
          session_id: params?.session_id,
          status: params?.status,
        },
      });
      return data.items ?? [];
    },

    async getJobStats(): Promise<Record<string, Record<string, number>>> {
      const { data } = await client.get('/jobs/stats');
      return data.stats ?? {};
    },

    async composeContext(sessionId: string) {
      const { data } = await client.post('/context/compose', {
        session_id: sessionId,
        summary_limit: 3,
        include_raw_messages: true,
      });
      return data;
    },
  };
}

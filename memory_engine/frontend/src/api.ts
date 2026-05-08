import axios from 'axios';

import type {
  EngineJobPolicy,
  EngineJobRun,
  EngineModelProfile,
  JobRunQuery,
  UpsertEngineJobPolicyPayload,
  UpsertEngineModelProfilePayload,
} from './types';

const baseURL =
  import.meta.env.VITE_MEMORY_ENGINE_API_BASE ?? 'http://localhost:7081/api/memory-engine/v1';

const client = axios.create({
  baseURL,
  timeout: 30000,
});

export const api = {
  async listModelProfiles(): Promise<EngineModelProfile[]> {
    const { data } = await client.get('/admin/model-profiles');
    return data.items ?? [];
  },

  async createModelProfile(payload: UpsertEngineModelProfilePayload): Promise<EngineModelProfile> {
    const { data } = await client.post('/admin/model-profiles', payload);
    return data;
  },

  async updateModelProfile(
    modelId: string,
    payload: UpsertEngineModelProfilePayload,
  ): Promise<EngineModelProfile> {
    const { data } = await client.put(`/admin/model-profiles/${modelId}`, payload);
    return data;
  },

  async deleteModelProfile(modelId: string): Promise<void> {
    await client.delete(`/admin/model-profiles/${modelId}`);
  },

  async listJobPolicies(): Promise<EngineJobPolicy[]> {
    const { data } = await client.get('/admin/job-policies');
    return data.items ?? [];
  },

  async updateJobPolicy(
    jobType: string,
    payload: UpsertEngineJobPolicyPayload,
  ): Promise<EngineJobPolicy> {
    const { data } = await client.put(`/admin/job-policies/${jobType}`, payload);
    return data;
  },

  async listJobRuns(params?: JobRunQuery): Promise<EngineJobRun[]> {
    const { data } = await client.get('/admin/job-runs', {
      params: {
        job_type: params?.job_type,
        status: params?.status,
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
        limit: params?.limit ?? 200,
      },
    });
    return data.items ?? [];
  },

  async getJobRunStats(): Promise<Record<string, Record<string, number>>> {
    const { data } = await client.get('/admin/job-runs/stats');
    return data.stats ?? {};
  },
};

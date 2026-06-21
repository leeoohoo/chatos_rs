import type {
  EngineRecord,
  EngineSubjectMemory,
  EngineSummary,
  EngineThread,
  SubjectMemoriesQuery,
  ThreadQuery,
  ThreadRecordsPage,
  ThreadRecordsQuery,
  ThreadSummariesQuery,
} from '../types';

import { client } from './client';

export const threadsApi = {
  async listThreads(params?: ThreadQuery): Promise<EngineThread[]> {
    const { data } = await client.get('/admin/threads/query', {
      params: {
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
        subject_id: params?.subject_id,
        external_thread_id: params?.external_thread_id,
        session_id: params?.session_id,
        contact_id: params?.contact_id,
        project_id: params?.project_id,
        agent_id: params?.agent_id,
        mapping_source: params?.mapping_source,
        mapping_version: params?.mapping_version,
        thread_label: params?.thread_label,
        status: params?.status,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async getThread(
    threadId: string,
    params?: { tenant_id?: string; source_id?: string },
  ): Promise<EngineThread | null> {
    const { data } = await client.get(`/threads/${encodeURIComponent(threadId)}`, {
      params: {
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
      },
    });
    return data.item ?? null;
  },

  async listThreadRecords(
    threadId: string,
    params?: ThreadRecordsQuery,
  ): Promise<ThreadRecordsPage> {
    const { data } = await client.get(`/threads/${encodeURIComponent(threadId)}/records`, {
      params: {
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
        role: params?.role,
        record_type: params?.record_type,
        summary_status: params?.summary_status,
        limit: params?.limit ?? 200,
        offset: params?.offset ?? 0,
        order: params?.order ?? 'asc',
      },
    });
    return {
      items: data.items ?? [],
      total: Number(data.total ?? 0),
    };
  },

  async listThreadSummaries(
    threadId: string,
    params?: ThreadSummariesQuery,
  ): Promise<EngineSummary[]> {
    const { data } = await client.get(`/threads/${encodeURIComponent(threadId)}/summaries`, {
      params: {
        tenant_id: params?.tenant_id,
        source_id: params?.source_id,
        summary_type: params?.summary_type,
        status: params?.status,
        level: params?.level,
        limit: params?.limit ?? 100,
        offset: params?.offset ?? 0,
      },
    });
    return data.items ?? [];
  },

  async listSubjectMemories(
    subjectId: string,
    params: SubjectMemoriesQuery,
  ): Promise<EngineSubjectMemory[]> {
    const { data } = await client.get(`/subjects/${encodeURIComponent(subjectId)}/memories`, {
      params: {
        tenant_id: params.tenant_id,
        source_id: params.source_id,
        memory_type: params.memory_type,
        level: params.level,
        limit: params.limit ?? 100,
        offset: params.offset ?? 0,
      },
    });
    return data.items ?? [];
  },
};

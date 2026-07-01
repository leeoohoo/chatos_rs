// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../api';
import { renderHook } from '../../test/renderHook';
import { buildThreadFilters, useThreadExplorer } from './useThreadExplorer';

vi.mock('../../api', () => ({
  api: {
    listThreads: vi.fn(),
    listThreadRecords: vi.fn(),
    listThreadSummaries: vi.fn(),
    listSubjectMemories: vi.fn(),
  },
}));

const threadA = {
  id: 'thread-a',
  tenant_id: 'tenant-a',
  source_id: 'source-a',
  subject_id: 'subject-a',
  thread_type: 'chat',
  status: 'active',
  summary_status: 'idle',
  pending_record_count: 0,
  pending_summary_tokens: 0,
  created_at: '2026-05-20T00:00:00Z',
  updated_at: '2026-05-20T00:00:00Z',
};

const threadB = {
  id: 'thread-b',
  tenant_id: 'tenant-a',
  source_id: 'source-a',
  subject_id: 'subject-b',
  thread_type: 'chat',
  status: 'active',
  summary_status: 'idle',
  pending_record_count: 0,
  pending_summary_tokens: 0,
  created_at: '2026-05-20T00:00:00Z',
  updated_at: '2026-05-20T00:00:00Z',
};

const threadWithAgentLabel = {
  ...threadA,
  id: 'thread-agent-label',
  subject_id: 'session:thread-agent-label',
  labels: ['agent:agent-42', 'project:project-1'],
};

const threadWithAgentMetadata = {
  ...threadA,
  id: 'thread-agent-metadata',
  subject_id: 'session:thread-agent-metadata',
  metadata: {
    legacy_session_mapping: {
      agent_id: 'agent-84',
    },
  },
};

describe('useThreadExplorer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads a thread record page with a single paged request', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);

    listThreads.mockResolvedValue([threadA]);
    listThreadRecords.mockResolvedValue({
      items: [
        {
          id: 'record-1',
          thread_id: 'thread-a',
          tenant_id: 'tenant-a',
          source_id: 'source-a',
          role: 'user',
          record_type: 'message',
          content: 'hello',
          summary_status: 'pending',
          created_at: '2026-05-20T00:00:00Z',
        },
      ],
      total: 1,
    });

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.threadsLoading).toBe(false);
      expect(result.current.selectedThread?.id).toBe('thread-a');
    });

    expect(listThreads).toHaveBeenCalledTimes(1);
    expect(listThreadRecords).toHaveBeenCalledTimes(1);
    expect(listThreadRecords).toHaveBeenCalledWith('thread-a', {
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      order: 'asc',
      limit: 20,
      offset: 0,
    });
    expect(result.current.threadRecordTotal).toBe(1);
    expect(result.current.threadRecords).toHaveLength(1);
  });

  it('builds extended thread filters for supported query fields', () => {
    expect(
      buildThreadFilters({
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        subject_id: 'subject-a',
        external_thread_id: 'ext-thread-a',
        session_id: 'session-a',
        contact_id: 'contact-a',
        project_id: 'project-a',
        agent_id: 'agent-a',
        mapping_source: 'slack',
        mapping_version: 'v2',
        thread_label: 'support',
        status: 'active',
        limit: 20,
        offset: 0,
      }),
    ).toEqual({
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      subject_id: 'subject-a',
      external_thread_id: 'ext-thread-a',
      session_id: 'session-a',
      contact_id: 'contact-a',
      project_id: 'project-a',
      agent_id: 'agent-a',
      mapping_source: 'slack',
      mapping_version: 'v2',
      thread_label: 'support',
      status: 'active',
      limit: 20,
      offset: 0,
    });
  });

  it('does not double-load summaries when switching threads on the summaries tab', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const listThreadSummaries = vi.mocked(api.listThreadSummaries);

    listThreads.mockResolvedValue([threadA, threadB]);
    listThreadRecords.mockResolvedValue({
      items: [],
      total: 0,
    });
    listThreadSummaries.mockImplementation(async (threadId: string) => [
      {
        id: `summary-${threadId}`,
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        thread_id: threadId,
        subject_id: threadId === 'thread-a' ? 'subject-a' : 'subject-b',
        summary_type: 'thread_incremental',
        level: 0,
        summary_text: `summary for ${threadId}`,
        source_record_count: 1,
        status: 'done',
        rollup_status: 'pending',
        subject_memory_summarized: 0,
        created_at: '2026-05-20T00:00:00Z',
        updated_at: '2026-05-20T00:00:00Z',
      },
    ]);

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-a');
    });

    await act(async () => {
      result.current.setDetailTab('summaries');
    });

    await waitFor(() => {
      expect(listThreadSummaries).toHaveBeenCalledTimes(1);
      expect(result.current.threadSummaries[0]?.thread_id).toBe('thread-a');
    });

    await act(async () => {
      await result.current.loadThreadDetails(threadB, { resetPage: true });
    });

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-b');
    });

    expect(listThreadSummaries).toHaveBeenCalledTimes(2);
    expect(listThreadSummaries).toHaveBeenNthCalledWith(1, 'thread-a', {
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      limit: 200,
      offset: 0,
    });
    expect(listThreadSummaries).toHaveBeenNthCalledWith(2, 'thread-b', {
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      limit: 200,
      offset: 0,
    });
  });

  it('refreshes the active summaries tab when reloading the same selected thread', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const listThreadSummaries = vi.mocked(api.listThreadSummaries);

    listThreads.mockResolvedValue([threadA]);
    listThreadRecords.mockResolvedValue({
      items: [],
      total: 0,
    });
    listThreadSummaries
      .mockResolvedValueOnce([
        {
          id: 'summary-thread-a-initial',
          tenant_id: 'tenant-a',
          source_id: 'source-a',
          thread_id: 'thread-a',
          subject_id: 'subject-a',
          summary_type: 'thread_incremental',
          level: 0,
          summary_text: 'initial summary',
          source_record_count: 1,
          status: 'done',
          rollup_status: 'pending',
          subject_memory_summarized: 0,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ])
      .mockResolvedValueOnce([
        {
          id: 'summary-thread-a-refreshed',
          tenant_id: 'tenant-a',
          source_id: 'source-a',
          thread_id: 'thread-a',
          subject_id: 'subject-a',
          summary_type: 'thread_incremental',
          level: 0,
          summary_text: 'refreshed summary',
          source_record_count: 2,
          status: 'done',
          rollup_status: 'pending',
          subject_memory_summarized: 0,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ]);

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-a');
    });

    await act(async () => {
      result.current.setDetailTab('summaries');
    });

    await waitFor(() => {
      expect(result.current.threadSummaries[0]?.id).toBe('summary-thread-a-initial');
    });

    await act(async () => {
      await result.current.loadThreads();
    });

    await waitFor(() => {
      expect(listThreadSummaries).toHaveBeenCalledTimes(2);
      expect(result.current.threadSummaries[0]?.id).toBe('summary-thread-a-refreshed');
    });
  });

  it('loads subject memories with the agent subject from thread labels', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const listSubjectMemories = vi.mocked(api.listSubjectMemories);

    listThreads.mockResolvedValue([threadWithAgentLabel]);
    listThreadRecords.mockResolvedValue({
      items: [],
      total: 0,
    });
    listSubjectMemories.mockResolvedValue([
      {
        id: 'memory-agent-42',
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        subject_id: 'agent:agent-42',
        memory_key: 'agent_recall:l0:1',
        memory_type: 'agent_recall',
        text: 'memory for agent 42',
        level: 0,
        status: 'active',
        rollup_status: 'pending',
        created_at: '2026-05-20T00:00:00Z',
        updated_at: '2026-05-20T00:00:00Z',
      },
    ]);

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-agent-label');
    });

    await act(async () => {
      result.current.setDetailTab('memories');
    });

    await waitFor(() => {
      expect(listSubjectMemories).toHaveBeenCalledTimes(1);
      expect(result.current.subjectMemories[0]?.subject_id).toBe('agent:agent-42');
    });

    expect(listSubjectMemories).toHaveBeenCalledWith('agent:agent-42', {
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      limit: 100,
      offset: 0,
    });
  });

  it('falls back to legacy session mapping when loading subject memories', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const listSubjectMemories = vi.mocked(api.listSubjectMemories);

    listThreads.mockResolvedValue([threadWithAgentMetadata]);
    listThreadRecords.mockResolvedValue({
      items: [],
      total: 0,
    });
    listSubjectMemories.mockResolvedValue([
      {
        id: 'memory-agent-84',
        tenant_id: 'tenant-a',
        source_id: 'source-a',
        subject_id: 'agent:agent-84',
        memory_key: 'agent_recall:l0:2',
        memory_type: 'agent_recall',
        text: 'memory for agent 84',
        level: 0,
        status: 'active',
        rollup_status: 'pending',
        created_at: '2026-05-20T00:00:00Z',
        updated_at: '2026-05-20T00:00:00Z',
      },
    ]);

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-agent-metadata');
    });

    await act(async () => {
      result.current.setDetailTab('memories');
    });

    await waitFor(() => {
      expect(listSubjectMemories).toHaveBeenCalledTimes(1);
      expect(result.current.subjectMemories[0]?.subject_id).toBe('agent:agent-84');
    });

    expect(listSubjectMemories).toHaveBeenCalledWith('agent:agent-84', {
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      limit: 100,
      offset: 0,
    });
  });

  it('ignores stale thread record responses when switching threads quickly', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);

    let resolveThreadARecords: ((value: { items: Array<{ id: string; thread_id: string; tenant_id: string; source_id: string; role: string; record_type: string; content: string; summary_status: string; created_at: string }>; total: number }) => void) | null = null;
    let resolveThreadBRecords: ((value: { items: Array<{ id: string; thread_id: string; tenant_id: string; source_id: string; role: string; record_type: string; content: string; summary_status: string; created_at: string }>; total: number }) => void) | null = null;

    listThreads.mockResolvedValue([threadA, threadB]);
    listThreadRecords.mockImplementation(
      (threadId: string) =>
        new Promise((resolve) => {
          if (threadId === 'thread-a') {
            resolveThreadARecords = resolve;
            return;
          }
          resolveThreadBRecords = resolve;
        }),
    );

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.threads).toHaveLength(2);
    });

    act(() => {
      void result.current.loadThreadDetails(threadB, { resetPage: true });
    });

    await waitFor(() => {
      expect(listThreadRecords).toHaveBeenCalledTimes(2);
    });

    await act(async () => {
      resolveThreadBRecords?.({
        items: [
          {
            id: 'record-b-1',
            thread_id: 'thread-b',
            tenant_id: 'tenant-a',
            source_id: 'source-a',
            role: 'assistant',
            record_type: 'message',
            content: 'thread b',
            summary_status: 'done',
            created_at: '2026-05-20T00:00:00Z',
          },
        ],
        total: 1,
      });
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-b');
      expect(result.current.threadRecords[0]?.thread_id).toBe('thread-b');
    });

    await act(async () => {
      resolveThreadARecords?.({
        items: [
          {
            id: 'record-a-1',
            thread_id: 'thread-a',
            tenant_id: 'tenant-a',
            source_id: 'source-a',
            role: 'user',
            record_type: 'message',
            content: 'thread a',
            summary_status: 'pending',
            created_at: '2026-05-20T00:00:00Z',
          },
        ],
        total: 1,
      });
      await Promise.resolve();
    });

    expect(result.current.selectedThread?.id).toBe('thread-b');
    expect(result.current.threadRecords[0]?.thread_id).toBe('thread-b');
  });

  it('ignores stale summary responses after the selected thread changes', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const listThreadSummaries = vi.mocked(api.listThreadSummaries);

    let resolveThreadASummaries: ((value: Array<{
      id: string;
      tenant_id: string;
      source_id: string;
      thread_id: string;
      subject_id: string;
      summary_type: string;
      level: number;
      summary_text: string;
      source_record_count: number;
      status: string;
      rollup_status: string;
      subject_memory_summarized: number;
      created_at: string;
      updated_at: string;
    }>) => void) | null = null;
    let resolveThreadBSummaries: ((value: Array<{
      id: string;
      tenant_id: string;
      source_id: string;
      thread_id: string;
      subject_id: string;
      summary_type: string;
      level: number;
      summary_text: string;
      source_record_count: number;
      status: string;
      rollup_status: string;
      subject_memory_summarized: number;
      created_at: string;
      updated_at: string;
    }>) => void) | null = null;

    listThreads.mockResolvedValue([threadA, threadB]);
    listThreadRecords.mockResolvedValue({
      items: [],
      total: 0,
    });
    listThreadSummaries.mockImplementation(
      (threadId: string) =>
        new Promise((resolve) => {
          if (threadId === 'thread-a') {
            resolveThreadASummaries = resolve;
            return;
          }
          resolveThreadBSummaries = resolve;
        }),
    );

    const { result } = renderHook(() => useThreadExplorer('data'));

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-a');
    });

    await act(async () => {
      result.current.setDetailTab('summaries');
    });

    await waitFor(() => {
      expect(listThreadSummaries).toHaveBeenCalledTimes(1);
    });

    act(() => {
      void result.current.loadThreadDetails(threadB, { resetPage: true });
    });

    await waitFor(() => {
      expect(result.current.selectedThread?.id).toBe('thread-b');
      expect(listThreadSummaries).toHaveBeenCalledTimes(2);
    });

    await act(async () => {
      resolveThreadBSummaries?.([
        {
          id: 'summary-thread-b',
          tenant_id: 'tenant-a',
          source_id: 'source-a',
          thread_id: 'thread-b',
          subject_id: 'subject-b',
          summary_type: 'thread_incremental',
          level: 0,
          summary_text: 'summary for thread b',
          source_record_count: 1,
          status: 'done',
          rollup_status: 'pending',
          subject_memory_summarized: 0,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ]);
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(result.current.threadSummaries[0]?.thread_id).toBe('thread-b');
    });

    await act(async () => {
      resolveThreadASummaries?.([
        {
          id: 'summary-thread-a',
          tenant_id: 'tenant-a',
          source_id: 'source-a',
          thread_id: 'thread-a',
          subject_id: 'subject-a',
          summary_type: 'thread_incremental',
          level: 0,
          summary_text: 'summary for thread a',
          source_record_count: 1,
          status: 'done',
          rollup_status: 'pending',
          subject_memory_summarized: 0,
          created_at: '2026-05-20T00:00:00Z',
          updated_at: '2026-05-20T00:00:00Z',
        },
      ]);
      await Promise.resolve();
    });

    expect(result.current.selectedThread?.id).toBe('thread-b');
    expect(result.current.threadSummaries[0]?.thread_id).toBe('thread-b');
  });

  it('reports initial thread load failures without leaving the page stuck loading', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const onError = vi.fn();

    listThreads.mockRejectedValue(new Error('list failed'));

    const { result } = renderHook(() => useThreadExplorer('data', { onError }));

    await waitFor(() => {
      expect(result.current.threadsLoading).toBe(false);
    });

    expect(onError).toHaveBeenCalledWith('加载线程列表失败：Error: list failed');
    expect(result.current.threads).toEqual([]);
  });
});

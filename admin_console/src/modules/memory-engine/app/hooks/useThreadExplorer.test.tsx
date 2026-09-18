// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../api';
import type { ThreadSummariesPage } from '../../types';
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

  it('uses the last visible record as the cursor for the next record page', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const record = (id: string, createdAt: string) => ({
      id,
      thread_id: threadA.id,
      tenant_id: threadA.tenant_id,
      source_id: threadA.source_id,
      role: 'user',
      record_type: 'message',
      content: id,
      summary_status: 'pending',
      created_at: createdAt,
    });
    const firstRecord = record('record-a', '2026-05-20T00:00:00Z');
    const secondRecord = record('record-b', '2026-05-20T00:00:01Z');
    const thirdRecord = record('record-c', '2026-05-20T00:00:02Z');
    const fourthRecord = record('record-d', '2026-05-20T00:00:03Z');

    listThreads.mockResolvedValue([threadA]);
    listThreadRecords
      .mockResolvedValueOnce({ items: [], total: 4, has_more: false })
      .mockResolvedValueOnce({
        items: [firstRecord, secondRecord],
        total: 4,
        has_more: true,
      })
      .mockResolvedValueOnce({
        items: [thirdRecord, fourthRecord],
        total: 4,
        has_more: false,
      });

    const { result } = renderHook(() => useThreadExplorer('data'));
    await waitFor(() => expect(result.current.selectedThread?.id).toBe(threadA.id));

    await act(async () => {
      await result.current.handleThreadRecordPageChange(1, 2);
    });
    await act(async () => {
      await result.current.handleThreadRecordPageChange(2, 2);
    });

    expect(listThreadRecords).toHaveBeenNthCalledWith(3, threadA.id, {
      tenant_id: threadA.tenant_id,
      source_id: threadA.source_id,
      order: 'asc',
      limit: 2,
      offset: 0,
      after_created_at: secondRecord.created_at,
      after_id: secondRecord.id,
    });
    expect(result.current.threadRecordPage).toBe(2);
    expect(result.current.threadRecords.map((item) => item.id)).toEqual([
      thirdRecord.id,
      fourthRecord.id,
    ]);
  });

  it('uses the last visible thread as the cursor for the next server page', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const threadC = {
      ...threadA,
      id: 'thread-c',
      subject_id: 'subject-c',
      created_at: '2026-05-19T00:00:00Z',
      updated_at: '2026-05-19T00:00:00Z',
    };
    const threadD = {
      ...threadC,
      id: 'thread-d',
      subject_id: 'subject-d',
      created_at: '2026-05-18T00:00:00Z',
      updated_at: '2026-05-18T00:00:00Z',
    };

    listThreads
      .mockResolvedValueOnce([threadA])
      .mockResolvedValueOnce([threadA, threadB, threadC])
      .mockResolvedValueOnce([threadC, threadD]);
    listThreadRecords.mockResolvedValue({ items: [], total: 0 });

    const { result } = renderHook(() => useThreadExplorer('data'));
    await waitFor(() => expect(result.current.selectedThread?.id).toBe('thread-a'));

    await act(async () => {
      await result.current.handleThreadPageChange(1, 2);
    });
    expect(result.current.threads.map((thread) => thread.id)).toEqual([
      'thread-a',
      'thread-b',
    ]);
    expect(result.current.threadHasMore).toBe(true);

    await act(async () => {
      await result.current.handleThreadPageChange(2, 2);
    });
    expect(listThreads).toHaveBeenNthCalledWith(
      3,
      expect.objectContaining({
        before_updated_at: threadB.updated_at,
        before_created_at: threadB.created_at,
        before_id: threadB.id,
        limit: 3,
        offset: 0,
      }),
    );
    expect(result.current.threadPage).toBe(2);
    expect(result.current.threads.map((thread) => thread.id)).toEqual([
      'thread-c',
      'thread-d',
    ]);
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
    listThreadSummaries.mockImplementation(async (threadId: string) => ({
      items: [{
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
      }],
      has_more: false,
    }));

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
      limit: 10,
      offset: 0,
    });
    expect(listThreadSummaries).toHaveBeenNthCalledWith(2, 'thread-b', {
      tenant_id: 'tenant-a',
      source_id: 'source-a',
      limit: 10,
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
      .mockResolvedValueOnce({
        items: [{
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
        }],
        has_more: false,
      })
      .mockResolvedValueOnce({
        items: [{
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
        }],
        has_more: false,
      });

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

  it('uses the last visible summary as the cursor for the next summary page', async () => {
    const listThreads = vi.mocked(api.listThreads);
    const listThreadRecords = vi.mocked(api.listThreadRecords);
    const listThreadSummaries = vi.mocked(api.listThreadSummaries);
    const summary = (id: string, level: number, createdAt: string) => ({
      id,
      tenant_id: threadA.tenant_id,
      source_id: threadA.source_id,
      thread_id: threadA.id,
      subject_id: threadA.subject_id,
      summary_type: 'thread_incremental',
      level,
      summary_text: id,
      source_record_count: 1,
      status: 'done',
      rollup_status: 'pending',
      subject_memory_summarized: 0,
      created_at: createdAt,
      updated_at: createdAt,
    });
    const firstSummary = summary('summary-a', 2, '2026-05-20T00:00:00Z');
    const secondSummary = summary('summary-b', 1, '2026-05-20T00:00:00Z');
    const thirdSummary = summary('summary-c', 1, '2026-05-20T00:00:01Z');

    listThreads.mockResolvedValue([threadA]);
    listThreadRecords.mockResolvedValue({ items: [], total: 0 });
    listThreadSummaries
      .mockResolvedValueOnce({
        items: [firstSummary, secondSummary],
        has_more: true,
      })
      .mockResolvedValueOnce({
        items: [thirdSummary],
        has_more: false,
      });

    const { result } = renderHook(() => useThreadExplorer('data'));
    await waitFor(() => expect(result.current.selectedThread?.id).toBe(threadA.id));
    await act(async () => {
      result.current.setDetailTab('summaries');
    });
    await waitFor(() => expect(result.current.threadSummaries).toHaveLength(2));

    await act(async () => {
      await result.current.handleThreadSummaryPageChange(2, 10);
    });

    expect(listThreadSummaries).toHaveBeenNthCalledWith(2, threadA.id, {
      tenant_id: threadA.tenant_id,
      source_id: threadA.source_id,
      limit: 10,
      offset: 0,
      after_level: secondSummary.level,
      after_created_at: secondSummary.created_at,
      after_id: secondSummary.id,
    });
    expect(result.current.threadSummaryPage).toBe(2);
    expect(result.current.threadSummaries.map((item) => item.id)).toEqual([
      thirdSummary.id,
    ]);
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

    let resolveThreadASummaries: ((value: ThreadSummariesPage) => void) | null = null;
    let resolveThreadBSummaries: ((value: ThreadSummariesPage) => void) | null = null;

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
      resolveThreadBSummaries?.({
        items: [{
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
        }],
        has_more: false,
      });
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(result.current.threadSummaries[0]?.thread_id).toBe('thread-b');
    });

    await act(async () => {
      resolveThreadASummaries?.({
        items: [{
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
        }],
        has_more: false,
      });
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

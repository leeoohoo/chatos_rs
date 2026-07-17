// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useContactMemoryContext } from './useContactMemoryContext';

afterEach(cleanup);

describe('useContactMemoryContext local recall', () => {
  it('loads session summaries and local recalls without a cloud contact', async () => {
    const apiClient = {
      getConversationSummaries: vi.fn().mockResolvedValue({
        items: [{
          id: 'summary-local',
          summary_text: 'Current session summary',
          status: 'completed',
          level: 0,
          created_at: '2026-07-15T00:00:00Z',
          updated_at: '2026-07-15T00:00:00Z',
        }],
        total: 1,
        has_summary: true,
      }),
      getConversationMemoryRecalls: vi.fn().mockResolvedValue([{
        id: 'recall-project',
        recall_key: 'session:previous',
        recall_text: 'Previous project decision',
        subject_type: 'project',
        level: 0,
        updated_at: '2026-07-15T00:00:01Z',
      }]),
      getContactAgentRecalls: vi.fn(() => {
        throw new Error('cloud contact recall should not be used');
      }),
    };
    const { result } = renderHook(() => useContactMemoryContext({
      apiClient,
      currentSessionId: 'lc_session_current',
      currentContactId: '',
      currentProjectIdForMemory: 'project-local',
    }));

    await act(async () => {
      await result.current.loadContactMemoryContext('lc_session_current', true);
    });

    expect(result.current.sessionMemorySummaries).toHaveLength(1);
    expect(result.current.agentRecalls).toHaveLength(1);
    expect(result.current.agentRecalls[0].subjectType).toBe('project');
    expect(apiClient.getConversationMemoryRecalls).toHaveBeenCalledWith('lc_session_current');
    expect(apiClient.getContactAgentRecalls).not.toHaveBeenCalled();
  });
});

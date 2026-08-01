// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { useContactMemoryContext } from './useContactMemoryContext';

afterEach(cleanup);

describe('useContactMemoryContext cloud memory', () => {
  it('loads cloud session summaries and contact recalls', async () => {
    const apiClient = {
      getConversationSummaries: vi.fn().mockResolvedValue({
        items: [{
          id: 'summary-cloud',
          summary_text: 'Current session summary',
          status: 'completed',
          level: 0,
          created_at: '2026-07-15T00:00:00Z',
          updated_at: '2026-07-15T00:00:00Z',
        }],
        total: 1,
        has_summary: true,
      }),
      getContactAgentRecalls: vi.fn().mockResolvedValue([{
        id: 'recall-contact',
        recall_key: 'contact:decision',
        recall_text: 'Previous contact decision',
        subject_type: 'contact',
        level: 0,
        updated_at: '2026-07-15T00:00:01Z',
      }]),
    };
    const { result } = renderHook(() => useContactMemoryContext({
      apiClient,
      currentSessionId: 'cloud_session_current',
      currentContactId: 'contact-1',
      currentProjectIdForMemory: 'project-cloud',
    }));

    await act(async () => {
      await result.current.loadContactMemoryContext('cloud_session_current', true);
    });

    expect(result.current.sessionMemorySummaries).toHaveLength(1);
    expect(result.current.agentRecalls).toHaveLength(1);
    expect(result.current.agentRecalls[0].subjectType).toBe('contact');
    expect(apiClient.getContactAgentRecalls).toHaveBeenCalledWith(
      'contact-1',
      { limit: 50, offset: 0 },
    );
  });
});

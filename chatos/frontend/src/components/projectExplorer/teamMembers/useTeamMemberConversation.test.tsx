// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useTeamMemberConversation } from './useTeamMemberConversation';

describe('useTeamMemberConversation', () => {
  it('persists the draft runtime after creating the first contact session and before sending', async () => {
    const ensureContactSession = vi.fn(async () => 'session-new');
    const prepareSessionRuntime = vi.fn(async () => undefined);
    const selectSession = vi.fn(async () => undefined);
    const sendMessage = vi.fn(async () => undefined);
    const contact = {
      id: 'contact-1',
      agentId: 'agent-1',
      name: 'Agent One',
    };
    const { result } = renderHook(() => useTeamMemberConversation({
      projectId: 'project-1',
      projectRootPath: '/tmp/project-1',
      currentSession: null,
      projectContacts: [{
        contact,
        session: null,
        latestSessionId: null,
        lastMessageAt: null,
        updatedAt: 1,
      }],
      normalizedContacts: [contact],
      summaryPaneSessionId: null,
      setSummaryPaneSessionId: vi.fn(),
      setSummaryError: vi.fn(),
      resetSummaryState: vi.fn(),
      openSummaryForSession: vi.fn(async () => undefined),
      deleteSummary: vi.fn(async () => undefined),
      clearSummaries: vi.fn(async () => undefined),
      cancelPendingSessionSummariesLoad: vi.fn(),
      ensureContactSession,
      selectSession,
      sendMessage,
      loadMoreMessages: vi.fn(async () => undefined),
    }));

    await waitFor(() => {
      expect(result.current.selectedContact?.id).toBe('contact-1');
    });

    await act(async () => {
      await result.current.handleSendMessage(
        'hello',
        [],
        undefined,
        prepareSessionRuntime,
      );
    });

    expect(ensureContactSession).toHaveBeenCalledWith(
      contact,
      { createIfMissing: true },
    );
    expect(prepareSessionRuntime).toHaveBeenCalledWith('session-new');
    expect(selectSession).toHaveBeenCalledWith('session-new', {
      keepActivePanel: true,
      skipBackgroundSync: true,
    });
    expect(sendMessage).toHaveBeenCalledWith('hello', [], {
      contactAgentId: 'agent-1',
      contactId: 'contact-1',
      projectId: 'project-1',
      projectRoot: '/tmp/project-1',
      workspaceRoot: null,
    });
    expect(prepareSessionRuntime.mock.invocationCallOrder[0]).toBeLessThan(
      sendMessage.mock.invocationCallOrder[0],
    );
  });
});

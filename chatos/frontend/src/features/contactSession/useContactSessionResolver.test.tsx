// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { Session } from '../../types';
import { useContactSessionResolver } from './useContactSessionResolver';

const legacySession = (contactId?: string): Session => ({
  id: 'lc_session_legacy',
  title: 'Legacy contact session',
  projectId: 'project-1',
  project_id: 'project-1',
  createdAt: new Date('2026-07-21T00:00:00Z'),
  updatedAt: new Date('2026-07-21T00:01:00Z'),
  messageCount: 2,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  status: 'active',
  metadata: {
    chat_runtime: {
      contact_agent_id: 'agent-1',
      project_id: 'project-1',
    },
    contact: {
      agent_id: 'agent-1',
      ...(contactId ? { contact_id: contactId } : {}),
    },
  },
});

describe('useContactSessionResolver legacy preferred sessions', () => {
  it('reuses an authoritative preferred local session that predates contact-id persistence', async () => {
    const createSession = vi.fn();
    const { result } = renderHook(() => useContactSessionResolver({
      sessions: [legacySession()],
      currentSession: null,
      createSession,
      includeApiLookup: false,
      defaultProjectId: 'project-1',
    }));

    let sessionId: string | null = null;
    await act(async () => {
      sessionId = await result.current.ensureContactSession(
        { id: 'contact-1', agentId: 'agent-1' },
        {
          projectId: 'project-1',
          preferredSessionId: 'lc_session_legacy',
          preferredSessionHasMessages: true,
          createIfMissing: true,
        },
      );
    });

    expect(sessionId).toBe('lc_session_legacy');
    expect(createSession).not.toHaveBeenCalled();
  });

  it('does not reuse a preferred session explicitly bound to another contact', async () => {
    const createSession = vi.fn();
    const { result } = renderHook(() => useContactSessionResolver({
      sessions: [legacySession('contact-other')],
      currentSession: null,
      createSession,
      includeApiLookup: false,
      defaultProjectId: 'project-1',
    }));

    let sessionId: string | null = null;
    await act(async () => {
      sessionId = await result.current.ensureContactSession(
        { id: 'contact-1', agentId: 'agent-1' },
        {
          projectId: 'project-1',
          preferredSessionId: 'lc_session_legacy',
          preferredSessionHasMessages: true,
          createIfMissing: false,
        },
      );
    });

    expect(sessionId).toBeNull();
    expect(createSession).not.toHaveBeenCalled();
  });
});

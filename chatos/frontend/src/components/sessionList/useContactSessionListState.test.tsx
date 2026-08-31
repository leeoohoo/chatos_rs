// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { Session } from '../../types';
import { useContactSessionListState } from './useContactSessionListState';

const contacts = [{
  id: 'contact-1',
  agentId: 'agent-1',
  name: 'Alice',
  createdAt: new Date('2026-08-01T00:00:00.000Z'),
  updatedAt: new Date('2026-08-01T00:00:00.000Z'),
}];

const buildSession = (projectId: string, contactId: string | null = 'contact-1'): Session => ({
  id: `${projectId}-session`,
  title: 'Alice',
  projectId,
  project_id: projectId,
  createdAt: new Date('2026-08-01T00:00:00.000Z'),
  updatedAt: new Date('2026-08-01T00:01:00.000Z'),
  messageCount: 1,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  status: 'active',
  metadata: {
    chat_runtime: {
      project_id: projectId,
      contact_agent_id: 'agent-1',
    },
    contact: {
      agent_id: 'agent-1',
      ...(contactId ? { contact_id: contactId } : {}),
    },
  },
});

const apiClient = {
  getSessions: vi.fn(),
  getConversationMessages: vi.fn(),
};

describe('useContactSessionListState project isolation', () => {
  it('does not highlight or map a project session as the public contact session', () => {
    const projectSession = buildSession('project-1');
    const { result } = renderHook(() => useContactSessionListState({
      contacts,
      sessions: [projectSession],
      currentSession: projectSession,
      activePanel: 'chat',
      createSession: vi.fn(),
      apiClient,
    }));

    expect(result.current.currentDisplaySessionId).toBeNull();
    expect(result.current.displaySessionRuntimeIdMap).toEqual({});
  });

  it('highlights and maps the public session for the contact', () => {
    const publicSession = buildSession('-1');
    const { result } = renderHook(() => useContactSessionListState({
      contacts,
      sessions: [publicSession],
      currentSession: publicSession,
      activePanel: 'chat',
      createSession: vi.fn(),
      apiClient,
    }));

    expect(result.current.currentDisplaySessionId).toBe('contact-placeholder:contact-1');
    expect(result.current.displaySessionRuntimeIdMap).toEqual({
      'contact-placeholder:contact-1': '-1-session',
    });
  });

  it('does not map one legacy agent-only session to multiple contacts', () => {
    const legacySession = buildSession('-1', null);
    const ambiguousContacts = [
      contacts[0],
      {
        ...contacts[0],
        id: 'contact-2',
        name: 'Bob',
      },
    ];
    const { result } = renderHook(() => useContactSessionListState({
      contacts: ambiguousContacts,
      sessions: [legacySession],
      currentSession: legacySession,
      activePanel: 'chat',
      createSession: vi.fn(),
      apiClient,
    }));

    expect(result.current.currentDisplaySessionId).toBeNull();
    expect(result.current.displaySessionRuntimeIdMap).toEqual({});
  });
});

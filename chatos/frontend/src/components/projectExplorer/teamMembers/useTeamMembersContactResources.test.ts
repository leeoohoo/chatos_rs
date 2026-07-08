// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Session } from '../../../types';
import type { ContactItem } from './types';
import { resolveProjectContactSession } from './useTeamMembersContactResources';

const buildContact = (): ContactItem => ({
  id: 'contact-1',
  agentId: 'agent-1',
  name: 'Alice',
});

const buildSession = (
  overrides: Partial<Session> = {},
): Session => ({
  id: 'session-1',
  title: '会话一',
  projectId: 'project-1',
  project_id: 'project-1',
  createdAt: new Date('2026-05-25T10:00:00.000Z'),
  updatedAt: new Date('2026-05-25T10:00:00.000Z'),
  messageCount: 2,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  metadata: {
    chat_runtime: {
      project_id: 'project-1',
      contact_agent_id: 'agent-1',
    },
    contact: {
      type: 'memory_agent',
      contact_id: 'contact-1',
      agent_id: 'agent-1',
    },
  },
  ...overrides,
});

describe('resolveProjectContactSession', () => {
  it('prefers the current matched session over an older session lookup result', () => {
    const contact = buildContact();
    const currentSession = buildSession({
      id: 'running-session',
      updatedAt: new Date('2026-05-25T10:05:00.000Z'),
    });
    const staleLookupSession = buildSession({
      id: 'stale-session',
      updatedAt: new Date('2026-05-25T10:00:00.000Z'),
    });

    const resolved = resolveProjectContactSession({
      currentSession,
      contact,
      normalizedProjectId: 'project-1',
      findProjectSessionForContact: () => staleLookupSession,
    });

    expect(resolved?.id).toBe('running-session');
  });

  it('falls back to the lookup result when current session does not match the member project scope', () => {
    const contact = buildContact();
    const currentSession = buildSession({
      id: 'other-project-session',
      projectId: 'project-2',
      project_id: 'project-2',
      metadata: {
        chat_runtime: {
          project_id: 'project-2',
          contact_agent_id: 'agent-1',
        },
        contact: {
          type: 'memory_agent',
          contact_id: 'contact-1',
          agent_id: 'agent-1',
        },
      },
    });
    const lookupSession = buildSession({
      id: 'project-1-session',
    });

    const resolved = resolveProjectContactSession({
      currentSession,
      contact,
      normalizedProjectId: 'project-1',
      findProjectSessionForContact: () => lookupSession,
    });

    expect(resolved?.id).toBe('project-1-session');
  });

  it('does not reuse the current session for a different contact sharing the same agent', () => {
    const contact = {
      ...buildContact(),
      id: 'contact-2',
    };
    const currentSession = buildSession({
      id: 'contact-1-session',
      metadata: {
        chat_runtime: {
          project_id: 'project-1',
          contact_agent_id: 'agent-1',
        },
        contact: {
          type: 'memory_agent',
          contact_id: 'contact-1',
          agent_id: 'agent-1',
        },
      },
    });
    const lookupSession = buildSession({
      id: 'contact-2-session',
      metadata: {
        chat_runtime: {
          project_id: 'project-1',
          contact_agent_id: 'agent-1',
        },
        contact: {
          type: 'memory_agent',
          contact_id: 'contact-2',
          agent_id: 'agent-1',
        },
      },
    });

    const resolved = resolveProjectContactSession({
      currentSession,
      contact,
      normalizedProjectId: 'project-1',
      findProjectSessionForContact: () => lookupSession,
    });

    expect(resolved?.id).toBe('contact-2-session');
  });
});

import { describe, expect, it } from 'vitest';

import type { Session } from '../../types';
import {
  findLatestMatchedSession,
  matchSessionContactProjectScope,
  normalizeContactSessions,
  normalizeMemoryContact,
  resolveSessionProjectScopeId,
  splitSessionsByMappedContacts,
} from './contactSessions';

const buildSession = (overrides: Partial<Session>): Session => ({
  id: 'session_1',
  title: 'Demo',
  createdAt: new Date('2026-04-01T00:00:00.000Z'),
  updatedAt: new Date('2026-04-01T00:00:00.000Z'),
  messageCount: 0,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  metadata: null,
  ...overrides,
});

describe('domain/contactSessions', () => {
  it('resolves project scope and contact identity from runtime metadata', () => {
    const session = buildSession({
      metadata: {
        chat_runtime: {
          project_id: 'project_1',
          contact_agent_id: 'agent_1',
        },
        contact: {
          contact_id: 'contact_1',
        },
      },
    });

    expect(resolveSessionProjectScopeId(session)).toBe('project_1');
    expect(matchSessionContactProjectScope(session, {
      contactId: 'contact_1',
      contactAgentId: 'agent_1',
      projectId: 'project_1',
    })).toBe(true);
  });

  it('dedupes contact sessions by contact/project and keeps the latest one', () => {
    const sessions = [
      buildSession({
        id: 'session_old',
        updatedAt: new Date('2026-04-01T00:00:00.000Z'),
        metadata: {
          chat_runtime: { project_id: 'project_1', contact_agent_id: 'agent_1' },
          contact: { contact_id: 'contact_1' },
        },
      }),
      buildSession({
        id: 'session_new',
        updatedAt: new Date('2026-04-02T00:00:00.000Z'),
        metadata: {
          chat_runtime: { project_id: 'project_1', contact_agent_id: 'agent_1' },
          contact: { contact_id: 'contact_1' },
        },
      }),
    ];

    const normalized = normalizeContactSessions(sessions);
    const matched = findLatestMatchedSession(normalized, {
      id: 'contact_1',
      agentId: 'agent_1',
    }, 'project_1');

    expect(normalized).toHaveLength(1);
    expect(normalized[0]?.id).toBe('session_new');
    expect(matched?.id).toBe('session_new');
  });

  it('does not match another contact just because the agent id is the same', () => {
    const session = buildSession({
      id: 'session_contact_1',
      metadata: {
        chat_runtime: { project_id: 'project_1', contact_agent_id: 'agent_shared' },
        contact: { contact_id: 'contact_1' },
      },
    });

    expect(matchSessionContactProjectScope(session, {
      contactId: 'contact_2',
      contactAgentId: 'agent_shared',
      projectId: 'project_1',
    })).toBe(false);

    expect(findLatestMatchedSession([session], {
      id: 'contact_2',
      agentId: 'agent_shared',
    }, 'project_1')).toBeNull();
  });

  it('keeps same-agent contacts missing until each has its own contact session', () => {
    const sessions = [
      buildSession({
        id: 'session_contact_1',
        metadata: {
          chat_runtime: { project_id: 'project_1', contact_agent_id: 'agent_shared' },
          contact: { contact_id: 'contact_1' },
        },
      }),
    ];
    const { matchedSessions, missingContacts } = splitSessionsByMappedContacts(sessions, [
      {
        id: 'contact_1',
        user_id: 'user_1',
        agent_id: 'agent_shared',
      },
      {
        id: 'contact_2',
        user_id: 'user_1',
        agent_id: 'agent_shared',
      },
    ]);

    expect(matchedSessions.map((session) => session.id)).toEqual(['session_contact_1']);
    expect(missingContacts.map((contact) => contact.id)).toEqual(['contact_2']);
  });

  it('does not assign an agent-only legacy session when same-agent contacts are ambiguous', () => {
    const sessions = [
      buildSession({
        id: 'session_agent_only',
        metadata: {
          chat_runtime: { project_id: 'project_1', contact_agent_id: 'agent_shared' },
        },
      }),
    ];
    const { matchedSessions, missingContacts } = splitSessionsByMappedContacts(sessions, [
      {
        id: 'contact_1',
        user_id: 'user_1',
        agent_id: 'agent_shared',
      },
      {
        id: 'contact_2',
        user_id: 'user_1',
        agent_id: 'agent_shared',
      },
    ]);

    expect(matchedSessions).toHaveLength(0);
    expect(missingContacts.map((contact) => contact.id)).toEqual(['contact_1', 'contact_2']);
  });

  it('normalizes memory contact payloads defensively', () => {
    expect(normalizeMemoryContact({
      id: 'contact_1',
      user_id: 'user_1',
      agent_id: 'agent_1',
      agent_name_snapshot: 'Agent One',
    })).toEqual({
      id: 'contact_1',
      user_id: 'user_1',
      agent_id: 'agent_1',
      agent_name_snapshot: 'Agent One',
      status: null,
      created_at: undefined,
      updated_at: undefined,
    });
    expect(normalizeMemoryContact({ id: 'broken' })).toBeNull();
  });
});

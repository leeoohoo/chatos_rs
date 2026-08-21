// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { Session } from '../../../../types';
import {
  buildSessionsListCacheKey,
  getOrCreateSessionsClientCacheState,
  syncLoadedSessions,
  upsertSessionCaches,
} from './cache';

const contact = {
  id: 'contact-1',
  agentId: 'agent-1',
  name: 'Alice',
  status: 'active' as const,
  createdAt: new Date('2026-08-01T00:00:00.000Z'),
  updatedAt: new Date('2026-08-01T00:00:00.000Z'),
};

const buildSession = (id: string, projectId: string, userId = 'user-1'): Session => ({
  id,
  title: id,
  userId,
  user_id: userId,
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
      contact_id: 'contact-1',
      agent_id: 'agent-1',
    },
  },
});

describe('session list cache scope isolation', () => {
  it('does not insert project sessions into the public cache or public sessions into project caches', () => {
    const client = {} as never;
    const publicSession = buildSession('public-session', '-1');
    const projectSession = buildSession('project-session', 'project-1');

    syncLoadedSessions(client, 'user-1', '-1', [publicSession], [contact]);
    syncLoadedSessions(client, 'user-1', 'project-1', [projectSession], [contact]);

    upsertSessionCaches(client, {
      ...projectSession,
      updatedAt: new Date('2026-08-01T00:02:00.000Z'),
    });
    upsertSessionCaches(client, {
      ...publicSession,
      updatedAt: new Date('2026-08-01T00:03:00.000Z'),
    });

    const cache = getOrCreateSessionsClientCacheState(client);
    expect(cache.listCache.get(buildSessionsListCacheKey('user-1', '-1'))?.sessions.map((item) => item.id))
      .toEqual(['public-session']);
    expect(cache.listCache.get(buildSessionsListCacheKey('user-1', 'project-1'))?.sessions.map((item) => item.id))
      .toEqual(['project-session']);
  });

  it('uses one canonical cache key for legacy and canonical public project ids', () => {
    expect(buildSessionsListCacheKey('user-1', '0'))
      .toBe(buildSessionsListCacheKey('user-1', '-1'));
  });

  it('does not insert one user session into another user list cache', () => {
    const client = {} as never;
    const userOneSession = buildSession('user-1-session', '-1', 'user-1');
    const userTwoSession = buildSession('user-2-session', '-1', 'user-2');

    syncLoadedSessions(client, 'user-1', '-1', [userOneSession], [contact]);
    syncLoadedSessions(client, 'user-2', '-1', [userTwoSession], [contact]);
    upsertSessionCaches(client, userOneSession);

    const cache = getOrCreateSessionsClientCacheState(client);
    expect(cache.listCache.get(buildSessionsListCacheKey('user-2', '-1'))?.sessions.map((item) => item.id))
      .toEqual(['user-2-session']);
  });
});

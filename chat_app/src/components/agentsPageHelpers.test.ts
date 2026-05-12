import { describe, expect, it } from 'vitest';

import { buildGroupedConversationSessions } from './agentManager/sessionHelpers';
import type { Session } from '../types';

const buildSession = (overrides: Partial<Session> = {}): Session => ({
  id: 'session_1',
  title: '',
  userId: 'user_1',
  user_id: 'user_1',
  projectId: null,
  project_id: null,
  messageCount: 0,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  status: 'active',
  createdAt: new Date('2026-04-01T10:00:00Z'),
  updatedAt: new Date('2026-04-01T10:00:00Z'),
  ...overrides,
});

describe('agentsPage helpers', () => {
  it('groups sessions by normalized project id and keeps latest session', () => {
    const groups = buildGroupedConversationSessions(
      [
        buildSession({
          id: 'session_old',
          projectId: 'project_1',
          project_id: 'project_1',
          updatedAt: new Date('2026-04-01T10:00:00Z'),
        }),
        buildSession({
          id: 'session_new',
          projectId: ' project_1 ',
          project_id: ' project_1 ',
          updatedAt: new Date('2026-04-01T12:00:00Z'),
        }),
        buildSession({
          id: 'session_unassigned',
          projectId: ' ',
          project_id: ' ',
          updatedAt: new Date('2026-04-01T11:00:00Z'),
        }),
      ],
      {
        project_1: '项目一',
      },
    );

    expect(groups).toHaveLength(2);
    expect(groups[0].projectId).toBe('project_1');
    expect(groups[0].session.id).toBe('session_new');
    expect(groups[1].projectId).toBe('0');
    expect(groups[1].projectName).toBe('未归属项目');
  });
});

import { describe, expect, it } from 'vitest';

import { buildGroupedConversationSessions } from '../../../memory_server/frontend/src/pages/agentsPage/helpers';
import type { Session } from '../../../memory_server/frontend/src/types';

const t = (key: string) => {
  if (key === 'memory.unassignedProject') return '未归属项目';
  if (key === 'memory.unnamedProject') return '未命名项目';
  return key;
};

const buildSession = (overrides: Partial<Session> = {}): Session => ({
  id: 'session_1',
  user_id: 'user_1',
  project_id: null,
  project_name: null,
  title: null,
  status: 'active',
  created_at: '2026-04-01T10:00:00Z',
  updated_at: '2026-04-01T10:00:00Z',
  ...overrides,
});

describe('agentsPage helpers', () => {
  it('groups sessions by normalized project id and keeps latest session', () => {
    const groups = buildGroupedConversationSessions(
      [
        buildSession({
          id: 'session_old',
          project_id: 'project_1',
          project_name: '项目一',
          updated_at: '2026-04-01T10:00:00Z',
        }),
        buildSession({
          id: 'session_new',
          project_id: ' project_1 ',
          project_name: '项目一',
          updated_at: '2026-04-01T12:00:00Z',
        }),
        buildSession({
          id: 'session_unassigned',
          project_id: ' ',
          project_name: '',
          updated_at: '2026-04-01T11:00:00Z',
        }),
      ],
      {},
      t,
    );

    expect(groups).toHaveLength(2);
    expect(groups[0].projectId).toBe('project_1');
    expect(groups[0].session.id).toBe('session_new');
    expect(groups[1].projectId).toBe('0');
    expect(groups[1].projectName).toBe('未归属项目');
  });
});

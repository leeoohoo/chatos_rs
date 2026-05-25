import { describe, expect, it } from 'vitest';

import { groupWorkbarTasks, selectCurrentWorkbarTask } from './helpers';
import type { TaskWorkbarItem } from './types';

const buildTask = (
  id: string,
  status: TaskWorkbarItem['status'],
  createdAt: string,
): TaskWorkbarItem => ({
  id,
  title: id,
  details: '',
  status,
  priority: 'medium',
  conversationTurnId: 'turn_1',
  createdAt,
  tags: [],
  outcomeSummary: '',
  outcomeItems: [],
  resumeHint: '',
  blockerReason: '',
  blockerNeeds: [],
  blockerKind: '',
  completedAt: null,
  lastOutcomeAt: null,
});

describe('taskWorkbar helpers', () => {
  it('selects the latest doing task as current before other unfinished tasks', () => {
    const items = [
      buildTask('todo_new', 'todo', '2026-05-21T10:01:00Z'),
      buildTask('doing_latest', 'doing', '2026-05-21T10:03:00Z'),
      buildTask('doing_old', 'doing', '2026-05-21T10:00:00Z'),
      buildTask('done_1', 'done', '2026-05-21T09:00:00Z'),
    ];

    const current = selectCurrentWorkbarTask(items);
    const groups = groupWorkbarTasks(items);

    expect(current?.id).toBe('doing_latest');
    expect(groups.current.map((task) => task.id)).toEqual(['doing_latest']);
    expect(groups.unfinished.map((task) => task.id)).toEqual(['todo_new', 'doing_old']);
  });

  it('keeps current task empty when no doing task exists', () => {
    const items = [
      buildTask('todo_latest', 'todo', '2026-05-21T10:03:00Z'),
      buildTask('todo_old', 'todo', '2026-05-21T10:00:00Z'),
      buildTask('blocked_1', 'blocked', '2026-05-21T09:00:00Z'),
    ];

    const current = selectCurrentWorkbarTask(items);
    const groups = groupWorkbarTasks(items);

    expect(current).toBeNull();
    expect(groups.current).toEqual([]);
    expect(groups.unfinished.map((task) => task.id)).toEqual(['todo_latest', 'todo_old']);
    expect(groups.blocked.map((task) => task.id)).toEqual(['blocked_1']);
  });
});

import { describe, expect, it, vi } from 'vitest';

import {
  getTaskManagerTasks,
} from './tasks';

describe('tasks api client helpers', () => {
  it('does not issue requests for blank conversation ids', async () => {
    const request = vi.fn();

    await expect(getTaskManagerTasks(request as never, '   ')).resolves.toEqual([]);

    expect(request).not.toHaveBeenCalled();
  });

  it('trims conversation ids before building request urls', async () => {
    const request = vi.fn().mockResolvedValue([]);

    await getTaskManagerTasks(request as never, '  session_123  ', {
      conversationTurnId: 'turn_1',
      includeDone: true,
      limit: 10,
    });

    expect(request).toHaveBeenCalledTimes(1);
    expect(request.mock.calls[0][0]).toBe(
      '/task-manager/tasks?conversation_id=session_123&conversation_turn_id=turn_1&include_done=true&limit=10',
    );
  });
});

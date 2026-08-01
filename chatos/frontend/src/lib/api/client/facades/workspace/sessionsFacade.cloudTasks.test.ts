// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { workspaceSessionFacade } from './sessionsFacade';

describe('workspaceSessionFacade cloud task routing', () => {
  it('loads user turns and active task state from cloud conversation APIs', async () => {
    const request = vi.fn().mockResolvedValue({ items: [] });
    const context = { getRequestFn: () => request };

    await workspaceSessionFacade.getConversationUserMessageTurns.call(
      context as never,
      'session-1',
      { limit: 10, before: null },
    );
    await workspaceSessionFacade.getConversationTaskRunnerActiveMessageTasks.call(
      context as never,
      'session-1',
      { sourceUserMessageIds: ['message-1'], sourceTurnIds: ['turn-1'] },
    );

    expect(request).toHaveBeenNthCalledWith(
      1,
      '/conversations/session-1/user-message-turns?limit=10',
    );
    expect(request).toHaveBeenNthCalledWith(
      2,
      '/conversations/session-1/task-runner/active-message-tasks',
      {
        method: 'POST',
        body: JSON.stringify({
          source_user_message_ids: ['message-1'],
          source_turn_ids: ['turn-1'],
        }),
      },
    );
  });
});

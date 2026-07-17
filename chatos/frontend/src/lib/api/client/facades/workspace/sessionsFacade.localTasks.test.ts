// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { workspaceSessionFacade } from './sessionsFacade';

describe('workspaceSessionFacade local task board routing', () => {
  it('loads local user turns and active task state without cloud Task Runner', async () => {
    const getUserMessageTurns = vi.fn().mockResolvedValue({ items: [] });
    const getActiveMessageTasks = vi.fn().mockResolvedValue({ items: [] });
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud Task Runner must not be called');
    });
    const context = {
      getLocalRuntimeClient: () => ({ getUserMessageTurns, getActiveMessageTasks }),
      getRequestFn: () => cloudRequest,
    };

    await workspaceSessionFacade.getConversationUserMessageTurns.call(
      context as never,
      'lc_session_tasks',
      { limit: 10, before: null },
    );
    await workspaceSessionFacade.getConversationTaskRunnerActiveMessageTasks.call(
      context as never,
      'lc_session_tasks',
      { sourceUserMessageIds: ['message-1'], sourceTurnIds: ['turn-1'] },
    );

    expect(getUserMessageTurns).toHaveBeenCalledWith(
      'lc_session_tasks',
      { limit: 10, before: null },
    );
    expect(getActiveMessageTasks).toHaveBeenCalledWith('lc_session_tasks');
    expect(cloudRequest).not.toHaveBeenCalled();
  });
});

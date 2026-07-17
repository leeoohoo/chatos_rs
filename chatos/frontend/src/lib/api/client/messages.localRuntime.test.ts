// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { beforeEach, describe, expect, it, vi } from 'vitest';

const localTasks = vi.hoisted(() => ({
  getLocalTaskBoardGraph: vi.fn(),
  getLocalTaskBoardTask: vi.fn(),
  getLocalTaskBoardTasks: vi.fn(),
}));

vi.mock('../localRuntime/taskBoard', () => localTasks);

import {
  getMessageTaskRunnerGraph,
  getMessageTaskRunnerTask,
  getMessageTaskRunnerTasks,
} from './messages';

describe('message task runner local routing', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localTasks.getLocalTaskBoardTasks.mockResolvedValue({ items: [] });
    localTasks.getLocalTaskBoardGraph.mockResolvedValue({
      root_task_ids: [],
      nodes: [],
      edges: [],
    });
    localTasks.getLocalTaskBoardTask.mockResolvedValue({ id: 'task-1', title: 'Task' });
  });

  it('routes local task list and graph reads to SQLite runtime APIs', async () => {
    const cloudRequest = vi.fn(() => {
      throw new Error('cloud task runner must not be called');
    });
    const lookup = {
      sessionId: 'lc_session_1',
      turnId: 'lc_turn_1',
      sourceUserMessageId: 'lc_message_1',
    };

    await getMessageTaskRunnerTasks(cloudRequest, 'lc_message_1', lookup);
    await getMessageTaskRunnerGraph(cloudRequest, 'lc_message_1', lookup);
    await getMessageTaskRunnerTask(cloudRequest, 'lc_message_1', 'task-1', lookup);

    expect(localTasks.getLocalTaskBoardTasks).toHaveBeenCalledWith(
      'lc_session_1',
      { ...lookup, includeDone: true },
    );
    expect(localTasks.getLocalTaskBoardGraph).toHaveBeenCalledWith('lc_session_1', lookup);
    expect(localTasks.getLocalTaskBoardTask).toHaveBeenCalledWith('lc_session_1', 'task-1');
    expect(cloudRequest).not.toHaveBeenCalled();
  });
});

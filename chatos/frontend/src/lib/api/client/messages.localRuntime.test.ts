// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { beforeEach, describe, expect, it, vi } from 'vitest';

const localTasks = vi.hoisted(() => ({
  getLocalTaskBoardGraph: vi.fn(),
  getLocalTaskBoardTask: vi.fn(),
  getLocalTaskBoardTasks: vi.fn(),
  getLocalTaskRunnerRunDetail: vi.fn(),
  getLocalTaskRunnerRunOutputChanges: vi.fn(),
  getLocalTaskRunnerRunOutputDiff: vi.fn(),
}));

vi.mock('../localRuntime/taskBoard', () => localTasks);

import {
  getMessageTaskRunnerGraph,
  getMessageTaskRunnerGraphRun,
  getMessageTaskRunnerRunOutputChanges,
  getMessageTaskRunnerRunOutputDiff,
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
    localTasks.getLocalTaskRunnerRunDetail.mockResolvedValue({ task: { id: 'task-1', title: 'Task' }, run: { id: 'run-1', task_id: 'task-1' }, events: [] });
    localTasks.getLocalTaskRunnerRunOutputChanges.mockResolvedValue({ run_id: 'run-1', counts: {}, files: [], total: 0, limit: 200, offset: 0, has_more: false });
    localTasks.getLocalTaskRunnerRunOutputDiff.mockResolvedValue({ run_id: 'run-1', path: 'a.txt', status: 'unavailable' });
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
    await getMessageTaskRunnerGraphRun(cloudRequest, 'lc_message_1', 'run-1', lookup);
    await getMessageTaskRunnerRunOutputChanges(cloudRequest, 'lc_message_1', 'run-1', lookup);
    await getMessageTaskRunnerRunOutputDiff(cloudRequest, 'lc_message_1', 'run-1', 'a.txt', lookup);

    expect(localTasks.getLocalTaskBoardTasks).toHaveBeenCalledWith(
      'lc_session_1',
      { ...lookup, includeDone: true },
    );
    expect(localTasks.getLocalTaskBoardGraph).toHaveBeenCalledWith('lc_session_1', lookup);
    expect(localTasks.getLocalTaskBoardTask).toHaveBeenCalledWith('lc_session_1', 'task-1');
    expect(localTasks.getLocalTaskRunnerRunDetail).toHaveBeenCalledWith('run-1', lookup);
    expect(localTasks.getLocalTaskRunnerRunOutputChanges).toHaveBeenCalledWith('run-1', lookup);
    expect(localTasks.getLocalTaskRunnerRunOutputDiff).toHaveBeenCalledWith('run-1', 'a.txt');
    expect(cloudRequest).not.toHaveBeenCalled();
  });
});

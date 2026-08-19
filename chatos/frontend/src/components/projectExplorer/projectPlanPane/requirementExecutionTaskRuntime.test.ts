// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { MessageTaskRunnerTask } from '../../../lib/api/client/types';
import {
  taskHasActiveRun,
  taskHasQueuedRun,
  taskHasRunningRun,
  taskHasTerminalStatus,
} from './requirementExecutionTaskRuntime';

const task = (overrides: Partial<MessageTaskRunnerTask>): MessageTaskRunnerTask => ({
  id: 'task-1',
  title: 'Task 1',
  status: 'ready',
  prerequisite_task_ids: [],
  ...overrides,
} as MessageTaskRunnerTask);

describe('requirement execution task runtime state', () => {
  it('treats terminal task status as final even when the last run snapshot is stale', () => {
    const staleRunningTask = task({
      status: 'cancelled',
      last_run_id: 'run-1',
      last_run: {
        id: 'run-1',
        status: 'running',
        workspace_execution: {
          integration_status: 'pending',
        },
      } as MessageTaskRunnerTask['last_run'],
    });

    expect(taskHasTerminalStatus(staleRunningTask)).toBe(true);
    expect(taskHasActiveRun(staleRunningTask)).toBe(false);
    expect(taskHasRunningRun(staleRunningTask)).toBe(false);
    expect(taskHasQueuedRun(staleRunningTask)).toBe(false);
  });

  it('still reads active run state when the task itself is not terminal', () => {
    const runningTask = task({
      status: 'ready',
      last_run_id: 'run-1',
      last_run: {
        id: 'run-1',
        status: 'running',
      } as MessageTaskRunnerTask['last_run'],
    });

    expect(taskHasTerminalStatus(runningTask)).toBe(false);
    expect(taskHasActiveRun(runningTask)).toBe(true);
    expect(taskHasRunningRun(runningTask)).toBe(true);
    expect(taskHasQueuedRun(runningTask)).toBe(false);
  });
});

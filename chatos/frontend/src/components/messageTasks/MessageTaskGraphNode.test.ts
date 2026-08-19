// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { displayStatusForTask } from './MessageTaskGraphNode';

type DisplayTask = Parameters<typeof displayStatusForTask>[0];

const task = (overrides: Partial<DisplayTask>): DisplayTask => ({
  id: 'task-1',
  title: 'Task 1',
  status: 'ready',
  prerequisite_task_ids: [],
  prerequisite_tasks: [],
  ...overrides,
} as DisplayTask);

describe('displayStatusForTask', () => {
  it('prefers terminal task status over stale workspace integration state', () => {
    expect(displayStatusForTask(task({
      status: 'cancelled',
      last_run: {
        id: 'run-1',
        status: 'running',
        workspace_execution: {
          integration_status: 'pending',
        },
      },
    }), 0)).toBe('cancelled');
  });

  it('keeps integration and prerequisite display states for non-terminal tasks', () => {
    expect(displayStatusForTask(task({
      status: 'running',
      last_run: {
        id: 'run-1',
        status: 'running',
        workspace_execution: {
          integration_status: 'pending',
        },
      },
    }), 0)).toBe('integration_pending');

    expect(displayStatusForTask(task({
      status: 'ready',
      prerequisite_tasks: [{ id: 'task-0', status: 'running' }],
    }), 1)).toBe('waiting_prerequisite');
  });
});

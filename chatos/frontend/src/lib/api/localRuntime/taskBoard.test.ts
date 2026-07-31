// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

const bridge = vi.hoisted(() => ({
  requestLocalRuntime: vi.fn(),
}));

vi.mock('./bridge', () => bridge);

import { getLocalTaskRunnerRunDetail } from './taskBoard';

describe('local runtime task board API', () => {
  it('passes include_events=false when loading run detail without raw diagnostics', async () => {
    bridge.requestLocalRuntime.mockResolvedValueOnce({
      task: { id: 'task-1', title: 'Task' },
      run: { id: 'run-1', task_id: 'task-1' },
      process_tasks: [],
      events: [],
    });

    await getLocalTaskRunnerRunDetail('run-1', {
      includeEvents: false,
      eventLimit: 1,
      eventOffset: 0,
    });

    expect(bridge.requestLocalRuntime).toHaveBeenCalledWith(
      '/api/local/runtime/task-runs/run-1/detail?event_limit=1&event_offset=0&include_events=false',
    );
  });
});

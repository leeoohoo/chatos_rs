// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import {
  getMessageTaskRunnerGraph,
  getMessageTaskRunnerGraphRun,
  getMessageTaskRunnerTasks,
  retryMessageTaskRunnerRun,
} from './messages';

describe('message Task Runner cloud routing', () => {
  it('keeps lookup identity in cloud task and graph requests', async () => {
    const request = vi.fn().mockResolvedValue({ items: [] });
    const lookup = {
      sessionId: 'session-1',
      turnId: 'turn-1',
      sourceUserMessageId: 'message-1',
    };

    await getMessageTaskRunnerTasks(request, 'message-1', lookup);
    await getMessageTaskRunnerGraph(request, 'message-1', lookup);

    expect(request.mock.calls[0][0]).toContain('/messages/message-1/task-runner/tasks?');
    expect(request.mock.calls[0][0]).toContain('session_id=session-1');
    expect(request.mock.calls[1][0]).toContain('/messages/message-1/task-runner/graph?');
  });

  it('sends retry guidance and execution service to the cloud run endpoint', async () => {
    const request = vi.fn().mockResolvedValue({ success: true });

    await retryMessageTaskRunnerRun(
      request,
      'message-1',
      'run-1',
      { sessionId: 'session-1' },
      '  环境变量已经补齐，请继续  ',
      'execution-service-1',
    );

    expect(request).toHaveBeenCalledWith(
      expect.stringContaining('/messages/message-1/task-runner/runs/run-1/retry?'),
      {
        method: 'POST',
        body: JSON.stringify({
          retry_instruction: '环境变量已经补齐，请继续',
          execution_service_id: 'execution-service-1',
        }),
      },
    );
  });

  it('preserves includeEvents=false in cloud run detail lookups', async () => {
    const request = vi.fn().mockResolvedValue({ events: [] });

    await getMessageTaskRunnerGraphRun(request, 'message-1', 'run-1', {
      sessionId: 'session-1',
      includeEvents: false,
      eventLimit: 1,
      eventOffset: 0,
    });

    expect(request.mock.calls[0][0]).toContain('include_events=false');
  });
});

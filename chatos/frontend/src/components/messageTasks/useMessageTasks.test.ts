// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import type ApiClient from '../../lib/api/client';
import type { MessageTaskRunnerTask } from '../../lib/api/client/types';
import { useMessageTasks } from './useMessageTasks';

const createApiWrapper = (request: ReturnType<typeof vi.fn>) => {
  const client = {
    getRequestFn: () => request,
  } as unknown as ApiClient;
  return ({ children }: { children: React.ReactNode }) => React.createElement(
    ApiClientProvider,
    { children, client },
  );
};

describe('useMessageTasks detail output', () => {
  it('loads the latest run report when task details are opened', async () => {
    const task: MessageTaskRunnerTask = {
      id: 'task-1',
      title: '检查项目',
      last_run_id: 'run-1',
      result_summary: '摘要',
    };
    const request = vi.fn((path: string) => {
      if (path.includes('/task-runner/runs/run-1')) {
        return Promise.resolve({
          task,
          run: {
            id: 'run-1',
            task_id: 'task-1',
            report: { content: '# 完整模型输出' },
          },
          events: [],
        });
      }
      if (path.includes('/task-runner/tasks/task-1')) {
        return Promise.resolve(task);
      }
      return Promise.resolve({
        items: [task],
        source_user_message_id: 'message-1',
      });
    });
    const lookup = {
      sessionId: 'session-1',
      sourceUserMessageId: 'message-1',
    };
    const { result } = renderHook(() => useMessageTasks({
      open: true,
      messageId: 'message-1',
      lookup,
    }), { wrapper: createApiWrapper(request) });

    await waitFor(() => expect(result.current.tasks).toHaveLength(1));
    await act(async () => {
      await result.current.openDetail(task);
    });

    expect(request).toHaveBeenCalledWith(expect.stringContaining(
      '/messages/message-1/task-runner/runs/run-1',
    ));
    expect(request).toHaveBeenCalledWith(expect.stringContaining('include_events=false'));
    expect(result.current.detailTask?.last_run?.report).toEqual({
      content: '# 完整模型输出',
    });
  });
});

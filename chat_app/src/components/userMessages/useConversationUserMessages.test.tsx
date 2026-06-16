// @vitest-environment jsdom

import { act, renderHook, waitFor } from '@testing-library/react';
import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import { getMessageTaskRunnerGraph } from '../../lib/api/client/messages';
import { useConversationUserMessages } from './useConversationUserMessages';

vi.mock('../../lib/api/client/messages', () => ({
  getMessageTaskRunnerGraph: vi.fn(),
}));

const mockedGetMessageTaskRunnerGraph = vi.mocked(getMessageTaskRunnerGraph);

const wrapperForClient = (client: unknown) => {
  const Wrapper: React.FC<{ children: React.ReactNode }> = ({ children }) => (
    <ApiClientProvider client={client as never}>
      <I18nProvider>
        {children}
      </I18nProvider>
    </ApiClientProvider>
  );
  return Wrapper;
};

const flushPromises = async (count = 5) => {
  for (let index = 0; index < count; index += 1) {
    await Promise.resolve();
  }
};

describe('useConversationUserMessages', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it('hydrates running task state from the live task graph', async () => {
    const request = vi.fn();
    const client = {
      getRequestFn: () => request,
      getConversationUserMessageTurns: vi.fn().mockResolvedValue({
        items: [{
          turn_id: 'turn-1',
          user_message: {
            id: 'user-1',
            conversation_id: 'session-1',
            role: 'user',
            content: '帮我继续处理这个任务',
            status: 'completed',
            created_at: '2026-06-16T06:43:48.000Z',
            metadata: {
              conversation_turn_id: 'turn-1',
              task_runner_async: {
                created_task_ids: ['task-1'],
                overall_status: 'completed',
              },
            },
          },
          final_assistant_message: null,
          has_process: true,
          process_message_count: 1,
        }],
        has_more: false,
        next_before: null,
      }),
    };
    mockedGetMessageTaskRunnerGraph.mockResolvedValue({
      root_task_ids: ['task-1'],
      nodes: [{
        depth: 0,
        is_root: true,
        is_current_message: true,
        task: {
          id: 'task-1',
          title: '继续处理',
          status: 'running',
          last_run: {
            id: 'run-1',
            task_id: 'task-1',
            status: 'running',
          },
        },
      }],
      edges: [],
    });

    const { result } = renderHook(
      () => useConversationUserMessages('session-1'),
      { wrapper: wrapperForClient(client) },
    );

    await waitFor(() => {
      expect(result.current.items).toHaveLength(1);
    });
    await waitFor(() => {
      expect(result.current.items[0]?.taskState.running).toBe(true);
    });
    expect(mockedGetMessageTaskRunnerGraph).toHaveBeenCalledWith(
      request,
      'user-1',
      {
        sessionId: 'session-1',
        sourceUserMessageId: 'user-1',
        turnId: 'turn-1',
      },
    );
  });

  it('clears stale running state when the live task graph is terminal', async () => {
    const client = {
      getRequestFn: () => vi.fn(),
      getConversationUserMessageTurns: vi.fn().mockResolvedValue({
        items: [{
          turn_id: 'turn-2',
          user_message: {
            id: 'user-2',
            conversation_id: 'session-1',
            role: 'user',
            content: '检查任务是否完成',
            status: 'completed',
            created_at: '2026-06-16T06:50:00.000Z',
            metadata: {
              conversation_turn_id: 'turn-2',
              task_runner_async: {
                running_task_ids: ['task-2'],
                overall_status: 'running',
              },
            },
          },
          final_assistant_message: null,
          has_process: true,
          process_message_count: 1,
        }],
        has_more: false,
        next_before: null,
      }),
    };
    mockedGetMessageTaskRunnerGraph.mockResolvedValue({
      root_task_ids: ['task-2'],
      nodes: [{
        depth: 0,
        is_root: true,
        is_current_message: true,
        task: {
          id: 'task-2',
          title: '检查完成',
          status: 'completed',
          last_run: {
            id: 'run-2',
            task_id: 'task-2',
            status: 'succeeded',
          },
        },
      }],
      edges: [],
    });

    const { result } = renderHook(
      () => useConversationUserMessages('session-1'),
      { wrapper: wrapperForClient(client) },
    );

    await waitFor(() => {
      expect(result.current.items).toHaveLength(1);
    });
    await waitFor(() => {
      expect(result.current.items[0]?.taskState.running).toBe(false);
    });
  });

  it('clears stale running state when the live task graph is empty', async () => {
    const client = {
      getRequestFn: () => vi.fn(),
      getConversationUserMessageTurns: vi.fn().mockResolvedValue({
        items: [{
          turn_id: 'turn-empty',
          user_message: {
            id: 'user-empty',
            conversation_id: 'session-1',
            role: 'user',
            content: '检查任务是否还在运行',
            status: 'completed',
            created_at: '2026-06-16T07:00:00.000Z',
            metadata: {
              conversation_turn_id: 'turn-empty',
              task_runner_async: {
                running_task_ids: ['missing-task'],
                overall_status: 'running',
              },
            },
          },
          final_assistant_message: null,
          has_process: true,
          process_message_count: 1,
        }],
        has_more: false,
        next_before: null,
      }),
    };
    mockedGetMessageTaskRunnerGraph.mockResolvedValue({
      root_task_ids: [],
      nodes: [],
      edges: [],
    });

    const { result } = renderHook(
      () => useConversationUserMessages('session-1'),
      { wrapper: wrapperForClient(client) },
    );

    await waitFor(() => {
      expect(result.current.items).toHaveLength(1);
    });
    await waitFor(() => {
      expect(result.current.items[0]?.taskState.running).toBe(false);
    });
  });

  it('does not keep polling terminal task graphs', async () => {
    vi.useFakeTimers();
    const client = {
      getRequestFn: () => vi.fn(),
      getConversationUserMessageTurns: vi.fn().mockResolvedValue({
        items: [{
          turn_id: 'turn-3',
          user_message: {
            id: 'user-3',
            conversation_id: 'session-1',
            role: 'user',
            content: '这个任务已经结束',
            status: 'completed',
            created_at: '2026-06-16T07:10:00.000Z',
            metadata: {
              conversation_turn_id: 'turn-3',
              task_runner_async: {
                created_task_ids: ['task-3'],
                overall_status: 'completed',
              },
            },
          },
          final_assistant_message: null,
          has_process: true,
          process_message_count: 1,
        }],
        has_more: false,
        next_before: null,
      }),
    };
    mockedGetMessageTaskRunnerGraph.mockResolvedValue({
      root_task_ids: ['task-3'],
      nodes: [{
        depth: 0,
        is_root: true,
        is_current_message: true,
        task: {
          id: 'task-3',
          title: '结束任务',
          status: 'completed',
          last_run: {
            id: 'run-3',
            task_id: 'task-3',
            status: 'succeeded',
          },
        },
      }],
      edges: [],
    });

    const { result, unmount } = renderHook(
      () => useConversationUserMessages('session-1'),
      { wrapper: wrapperForClient(client) },
    );

    try {
      await act(async () => {
        await flushPromises();
      });
      expect(result.current.items).toHaveLength(1);
      expect(result.current.items[0]?.taskState.running).toBe(false);
      expect(mockedGetMessageTaskRunnerGraph).toHaveBeenCalledTimes(1);

      await act(async () => {
        vi.advanceTimersByTime(12000);
        await flushPromises();
      });

      expect(mockedGetMessageTaskRunnerGraph).toHaveBeenCalledTimes(1);
    } finally {
      unmount();
      vi.useRealTimers();
    }
  });
});

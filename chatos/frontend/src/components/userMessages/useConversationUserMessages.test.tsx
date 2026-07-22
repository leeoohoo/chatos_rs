// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import { act, renderHook, waitFor } from '@testing-library/react';
import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import type { Message } from '../../types';
import {
  buildLiveUserMessageTurns,
  mergeLiveUserMessageTurns,
  useConversationUserMessages,
} from './useConversationUserMessages';

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

  it('shows a just-sent optimistic user message before the server turn endpoint catches up', () => {
    const liveMessage: Message = {
      id: 'temp_user_1',
      sessionId: 'session-1',
      role: 'user',
      content: 'new message',
      status: 'completed',
      createdAt: new Date('2026-07-21T08:48:00.000Z'),
      metadata: {
        clientOptimistic: true,
        conversation_turn_id: 'turn-new',
        task_runner_async: {
          mode: 'contact_async',
          overall_status: 'pending',
        },
      },
    };
    const liveTurns = buildLiveUserMessageTurns('session-1', [liveMessage]);
    const visible = mergeLiveUserMessageTurns([], liveTurns);

    expect(visible).toHaveLength(1);
    expect(visible[0]?.userMessage.id).toBe('temp_user_1');
    expect(visible[0]?.taskState.running).toBe(true);
  });

  it('deduplicates the optimistic turn after the server returns its persisted message id', () => {
    const liveMessage: Message = {
      id: 'temp_user_1',
      sessionId: 'session-1',
      role: 'user',
      content: 'new message',
      status: 'completed',
      createdAt: new Date('2026-07-21T08:48:00.000Z'),
      metadata: { clientOptimistic: true, conversation_turn_id: 'turn-new' },
    };
    const persisted = {
      turnId: 'turn-new',
      userMessage: { ...liveMessage, id: 'user-1', metadata: { conversation_turn_id: 'turn-new' } },
      finalAssistantMessage: null,
      hasProcess: false,
      toolCallCount: 0,
      thinkingCount: 0,
      processMessageCount: 0,
      taskState: { hasTask: false, running: false, label: null, runningCount: 0 },
    };

    const visible = mergeLiveUserMessageTurns(
      [persisted],
      buildLiveUserMessageTurns('session-1', [liveMessage]),
    );

    expect(visible).toHaveLength(1);
    expect(visible[0]?.userMessage.id).toBe('user-1');
  });

  it('hydrates active task state from the session task runner status endpoint', async () => {
    const client = {
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
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: ['user-1'],
        running_source_user_message_ids: ['user-1'],
        items: [{
          source_user_message_id: 'user-1',
          source_turn_id: 'turn-1',
          running_count: 1,
          active_count: 1,
        }],
      }),
    };

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
    expect(client.getConversationTaskRunnerActiveMessageTasks).toHaveBeenCalledWith('session-1', {
      sourceUserMessageIds: ['user-1'],
      sourceTurnIds: ['turn-1'],
    });
  });

  it('clears stale running state when the session task runner status has no active task', async () => {
    const client = {
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
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: [],
        running_source_user_message_ids: [],
        items: [],
      }),
    };

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

  it('does not treat ready-only active task state as running', async () => {
    const client = {
      getConversationUserMessageTurns: vi.fn().mockResolvedValue({
        items: [{
          turn_id: 'turn-ready',
          user_message: {
            id: 'user-ready',
            conversation_id: 'session-1',
            role: 'user',
            content: '这个任务现在只是待运行',
            status: 'completed',
            created_at: '2026-06-16T06:55:00.000Z',
            metadata: {
              conversation_turn_id: 'turn-ready',
              task_runner_async: {
                running_task_ids: ['task-ready'],
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
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: ['user-ready'],
        running_source_user_message_ids: [],
        items: [{
          source_user_message_id: 'user-ready',
          source_turn_id: 'turn-ready',
          running_count: 0,
          active_count: 1,
        }],
      }),
    };

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

  it('maps active task status by source turn id when message id is missing', async () => {
    const client = {
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
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: [],
        running_source_user_message_ids: [],
        items: [{
          source_user_message_id: null,
          source_turn_id: 'turn-empty',
          running_count: 1,
          active_count: 1,
        }],
      }),
    };

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
  });

  it('does not keep polling terminal task states', async () => {
    vi.useFakeTimers();
    const client = {
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
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: [],
        running_source_user_message_ids: [],
        items: [],
      }),
    };

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
      expect(client.getConversationTaskRunnerActiveMessageTasks).toHaveBeenCalledTimes(1);

      await act(async () => {
        vi.advanceTimersByTime(12000);
        await flushPromises();
      });

      expect(client.getConversationTaskRunnerActiveMessageTasks).toHaveBeenCalledTimes(1);
    } finally {
      unmount();
      vi.useRealTimers();
    }
  });

  it('reloads when the external user message refresh key changes', async () => {
    const client = {
      getConversationUserMessageTurns: vi.fn()
        .mockResolvedValueOnce({
          items: [{
            turn_id: 'turn-1',
            user_message: {
              id: 'user-1',
              conversation_id: 'session-1',
              role: 'user',
              content: '旧消息',
              status: 'completed',
              created_at: '2026-06-16T07:20:00.000Z',
              metadata: { conversation_turn_id: 'turn-1' },
            },
            final_assistant_message: null,
            has_process: false,
            process_message_count: 0,
          }],
          has_more: false,
          next_before: null,
        })
        .mockResolvedValueOnce({
          items: [{
            turn_id: 'turn-2',
            user_message: {
              id: 'user-2',
              conversation_id: 'session-1',
              role: 'user',
              content: '新消息',
              status: 'completed',
              created_at: '2026-06-16T07:21:00.000Z',
              metadata: { conversation_turn_id: 'turn-2' },
            },
            final_assistant_message: null,
            has_process: false,
            process_message_count: 0,
          }, {
            turn_id: 'turn-1',
            user_message: {
              id: 'user-1',
              conversation_id: 'session-1',
              role: 'user',
              content: '旧消息',
              status: 'completed',
              created_at: '2026-06-16T07:20:00.000Z',
              metadata: { conversation_turn_id: 'turn-1' },
            },
            final_assistant_message: null,
            has_process: false,
            process_message_count: 0,
          }],
          has_more: false,
          next_before: null,
        }),
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: [],
        running_source_user_message_ids: [],
        items: [],
      }),
    };

    const { result, rerender, unmount } = renderHook(
      ({ refreshKey }) => useConversationUserMessages('session-1', {
        refreshKey,
        refreshDelayMs: 0,
      }),
      {
        wrapper: wrapperForClient(client),
        initialProps: { refreshKey: 'user-1' },
      },
    );

    try {
      await waitFor(() => {
        expect(result.current.items.map((item) => item.userMessage.id)).toEqual(['user-1']);
      });

      rerender({ refreshKey: 'user-2' });

      await waitFor(() => {
        expect(result.current.items.map((item) => item.userMessage.id)).toEqual(['user-2', 'user-1']);
      });
      expect(client.getConversationUserMessageTurns).toHaveBeenCalledTimes(2);
    } finally {
      unmount();
    }
  });

  it('retries external refresh when the new user message is not visible yet', async () => {
    const oldTurn = {
      turn_id: 'turn-1',
      user_message: {
        id: 'user-1',
        conversation_id: 'session-1',
        role: 'user',
        content: 'old message',
        status: 'completed',
        created_at: '2026-06-16T07:20:00.000Z',
        metadata: { conversation_turn_id: 'turn-1' },
      },
      final_assistant_message: null,
      has_process: false,
      process_message_count: 0,
    };
    const newTurn = {
      turn_id: 'turn-2',
      user_message: {
        id: 'user-2',
        conversation_id: 'session-1',
        role: 'user',
        content: 'new message',
        status: 'completed',
        created_at: '2026-06-16T07:21:00.000Z',
        metadata: { conversation_turn_id: 'turn-2' },
      },
      final_assistant_message: null,
      has_process: false,
      process_message_count: 0,
    };
    const client = {
      getConversationUserMessageTurns: vi.fn()
        .mockResolvedValueOnce({
          items: [oldTurn],
          has_more: false,
          next_before: null,
        })
        .mockResolvedValueOnce({
          items: [oldTurn],
          has_more: false,
          next_before: null,
        })
        .mockResolvedValueOnce({
          items: [newTurn, oldTurn],
          has_more: false,
          next_before: null,
        }),
      getConversationTaskRunnerActiveMessageTasks: vi.fn().mockResolvedValue({
        active_source_user_message_ids: [],
        running_source_user_message_ids: [],
        items: [],
      }),
    };

    const { result, rerender, unmount } = renderHook(
      ({ refreshKey }) => useConversationUserMessages('session-1', {
        refreshKey,
      }),
      {
        wrapper: wrapperForClient(client),
        initialProps: { refreshKey: 'user-1' },
      },
    );

    try {
      await waitFor(() => {
        expect(result.current.items.map((item) => item.userMessage.id)).toEqual(['user-1']);
      });

      vi.useFakeTimers();
      rerender({ refreshKey: 'user-2' });

      await act(async () => {
        vi.advanceTimersByTime(350);
        await flushPromises();
      });
      expect(client.getConversationUserMessageTurns).toHaveBeenCalledTimes(2);
      expect(result.current.items.map((item) => item.userMessage.id)).toEqual(['user-1']);

      await act(async () => {
        vi.advanceTimersByTime(600);
        await flushPromises();
      });
      expect(client.getConversationUserMessageTurns).toHaveBeenCalledTimes(3);
      expect(result.current.items.map((item) => item.userMessage.id)).toEqual(['user-2', 'user-1']);
    } finally {
      unmount();
      vi.useRealTimers();
    }
  });
});

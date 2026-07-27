// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import type ApiClient from '../../lib/api/client';
import type { MessageTaskRunnerGraphResponse, MessageTaskRunnerTask } from '../../lib/api/client/types';
import { buildTaskSourceLookup, useMessageTaskGraph } from './useMessageTaskGraph';

const emptyGraph = (): MessageTaskRunnerGraphResponse => ({
  root_task_ids: [],
  nodes: [],
  edges: [],
  source_session_id: null,
  source_turn_id: null,
  source_user_message_id: null,
});

const graphWithTask = (taskId: string): MessageTaskRunnerGraphResponse => ({
  root_task_ids: [taskId],
  nodes: [{
    task: { id: taskId, title: taskId },
    depth: 0,
    is_root: true,
    is_current_message: true,
  }],
  edges: [],
  source_session_id: 'session-1',
  source_turn_id: `turn-${taskId}`,
  source_user_message_id: `message-${taskId}`,
});

const createApiWrapper = (request: ReturnType<typeof vi.fn>) => {
  const client = {
    getRequestFn: () => request,
  } as unknown as ApiClient;
  return ({ children }: { children: React.ReactNode }) => React.createElement(
    ApiClientProvider,
    { children, client },
  );
};

const createDeferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
};

describe('buildTaskSourceLookup', () => {
  it('uses the clicked task source message instead of the current graph message', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['current'],
      source_session_id: 'session-1',
      source_turn_id: 'turn-current',
      source_user_message_id: 'user-current',
      nodes: [],
      edges: [],
    };
    const task: MessageTaskRunnerTask = {
      id: 'prereq-task',
      title: '前置任务',
      source_session_id: 'session-1',
      source_turn_id: 'turn-prereq',
      source_user_message_id: 'user-prereq',
    };

    expect(buildTaskSourceLookup({
      task,
      graph,
      fallbackMessageId: 'user-current',
      fallbackLookup: {
        sessionId: 'session-1',
        turnId: 'turn-current',
        sourceUserMessageId: 'user-current',
      },
    })).toEqual({
      messageId: 'user-prereq',
      lookup: {
        sessionId: 'session-1',
        turnId: 'turn-prereq',
        sourceUserMessageId: 'user-prereq',
      },
    });
  });

  it('falls back to session and turn lookup when a task has no source message id', () => {
    const graph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['current'],
      source_session_id: 'session-1',
      source_turn_id: 'turn-current',
      source_user_message_id: 'user-current',
      nodes: [],
      edges: [],
    };
    const task: MessageTaskRunnerTask = {
      id: 'turn-only-task',
      title: '轮次任务',
      source_session_id: 'session-1',
      source_turn_id: 'turn-prereq',
    };

    expect(buildTaskSourceLookup({
      task,
      graph,
      fallbackMessageId: 'user-current',
      fallbackLookup: {
        sessionId: 'session-1',
        turnId: 'turn-current',
        sourceUserMessageId: 'user-current',
      },
    })).toEqual({
      messageId: 'task-source-turn-only-task',
      lookup: {
        sessionId: 'session-1',
        turnId: 'turn-prereq',
        sourceUserMessageId: null,
      },
    });
  });
});

describe('useMessageTaskGraph', () => {
  it('suppresses a transient missing planning message while the new plan is being persisted', async () => {
    const request = vi.fn().mockRejectedValue(new Error('需求执行规划消息不存在'));
    const isTransientError = (error: unknown) => (
      error instanceof Error && error.message.includes('需求执行规划消息不存在')
    );
    const lookup = {
      sessionId: 'session-1',
      turnId: 'turn-1',
      sourceUserMessageId: 'message-1',
    };
    const { result } = renderHook(() => useMessageTaskGraph({
      open: true,
      messageId: 'message-1',
      lookup,
      isTransientError,
    }), { wrapper: createApiWrapper(request) });

    await waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBeNull();
    expect(result.current.graph).toEqual(emptyGraph());
  });

  it('still exposes non-transient graph failures', async () => {
    const request = vi.fn().mockRejectedValue(new Error('任务图服务不可用'));
    const isTransientError = () => false;
    const lookup = {
      sessionId: 'session-1',
      turnId: 'turn-1',
      sourceUserMessageId: 'message-1',
    };
    const { result } = renderHook(() => useMessageTaskGraph({
      open: true,
      messageId: 'message-1',
      lookup,
      isTransientError,
    }), { wrapper: createApiWrapper(request) });

    await waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe('任务图服务不可用');
  });

  it('does not let a late response from an old execution group overwrite the new graph', async () => {
    const oldRequest = createDeferred<MessageTaskRunnerGraphResponse>();
    const newGraph = graphWithTask('new-task');
    const request = vi.fn((path: string) => (
      path.includes('/messages/message-old/')
        ? oldRequest.promise
        : Promise.resolve(newGraph)
    ));
    const { result, rerender } = renderHook(
      ({ messageId, turnId }: { messageId: string; turnId: string }) => {
        const lookup = React.useMemo(() => ({
          sessionId: 'session-1',
          turnId,
          sourceUserMessageId: messageId,
        }), [messageId, turnId]);
        return useMessageTaskGraph({ open: true, messageId, lookup });
      },
      {
        initialProps: { messageId: 'message-old', turnId: 'turn-old' },
        wrapper: createApiWrapper(request),
      },
    );

    await waitFor(() => expect(request).toHaveBeenCalledTimes(1));

    rerender({ messageId: 'message-new', turnId: 'turn-new' });

    await waitFor(() => expect(request).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.graph).toEqual(newGraph));

    await act(async () => {
      oldRequest.resolve(graphWithTask('old-task'));
      await oldRequest.promise;
    });

    expect(result.current.graph).toEqual(newGraph);
    expect(result.current.error).toBeNull();
  });

  it('retries only the clicked failed task run and reloads the graph', async () => {
    const failedTask: MessageTaskRunnerTask = {
      id: 'task-failed',
      title: '失败任务',
      status: 'failed',
      last_run_id: 'run-failed',
      source_session_id: 'session-1',
      source_turn_id: 'turn-1',
      source_user_message_id: 'message-1',
    };
    const graph = (status: string, runId: string): MessageTaskRunnerGraphResponse => ({
      root_task_ids: ['task-failed'],
      nodes: [{
        task: { ...failedTask, status, last_run_id: runId },
        depth: 0,
        is_root: true,
        is_current_message: true,
      }],
      edges: [],
      source_session_id: 'session-1',
      source_turn_id: 'turn-1',
      source_user_message_id: 'message-1',
    });
    let graphReads = 0;
    const request = vi.fn((path: string, init?: RequestInit) => {
      if (path.includes('/retry')) {
        expect(init?.method).toBe('POST');
        return Promise.resolve({
          success: true,
          run: { id: 'run-retry', task_id: 'task-failed', status: 'queued' },
        });
      }
      graphReads += 1;
      return Promise.resolve(graph(
        graphReads === 1 ? 'failed' : 'queued',
        graphReads === 1 ? 'run-failed' : 'run-retry',
      ));
    });
    const lookup = {
      sessionId: 'session-1',
      turnId: 'turn-1',
      sourceUserMessageId: 'message-1',
    };
    const { result } = renderHook(() => useMessageTaskGraph({
      open: true,
      messageId: 'message-1',
      lookup,
    }), { wrapper: createApiWrapper(request) });

    await waitFor(() => expect(result.current.allTasks[0]?.status).toBe('failed'));
    act(() => result.current.openDetail(failedTask));

    await act(async () => {
      await result.current.retryTask(failedTask);
    });

    expect(request).toHaveBeenCalledWith(
      '/messages/message-1/task-runner/runs/run-failed/retry?session_id=session-1&turn_id=turn-1&source_user_message_id=message-1',
      { method: 'POST' },
    );
    expect(result.current.detailTask).toMatchObject({
      id: 'task-failed',
      status: 'queued',
      last_run_id: 'run-retry',
    });
    expect(result.current.allTasks[0]).toMatchObject({
      id: 'task-failed',
      status: 'queued',
      last_run_id: 'run-retry',
    });
    expect(result.current.retryingTaskId).toBeNull();
    expect(result.current.error).toBeNull();
  });

  it('retries a blocked node with the user handling instruction', async () => {
    const blockedTask: MessageTaskRunnerTask = {
      id: 'task-blocked',
      title: '阻塞任务',
      status: 'blocked',
      last_run_id: 'run-blocked',
      source_session_id: 'session-1',
      source_turn_id: 'turn-1',
      source_user_message_id: 'message-1',
    };
    const lookup = {
      sessionId: 'session-1',
      turnId: 'turn-1',
      sourceUserMessageId: 'message-1',
    };
    let graphReads = 0;
    const request = vi.fn((path: string, init?: RequestInit) => {
      if (path.includes('/retry')) {
        expect(init).toEqual({
          method: 'POST',
          body: JSON.stringify({ retry_instruction: '配置已补齐，请继续' }),
        });
        return Promise.resolve({
          success: true,
          run: { id: 'run-retry', task_id: blockedTask.id, status: 'queued' },
        });
      }
      graphReads += 1;
      return Promise.resolve({
        root_task_ids: [blockedTask.id],
        nodes: [{
          task: {
            ...blockedTask,
            status: graphReads === 1 ? 'blocked' : 'queued',
            last_run_id: graphReads === 1 ? 'run-blocked' : 'run-retry',
          },
          depth: 0,
          is_root: true,
          is_current_message: true,
        }],
        edges: [],
        source_session_id: 'session-1',
        source_turn_id: 'turn-1',
        source_user_message_id: 'message-1',
      });
    });
    const { result } = renderHook(() => useMessageTaskGraph({
      open: true,
      messageId: 'message-1',
      lookup,
    }), { wrapper: createApiWrapper(request) });

    await waitFor(() => expect(result.current.allTasks[0]?.status).toBe('blocked'));
    await act(async () => {
      await result.current.retryTask(blockedTask, '配置已补齐，请继续');
    });

    expect(result.current.allTasks[0]).toMatchObject({
      id: 'task-blocked',
      status: 'queued',
      last_run_id: 'run-retry',
    });
  });

  it('exposes a retry request failure to the open task detail modal', async () => {
    const blockedTask: MessageTaskRunnerTask = {
      id: 'task-blocked',
      title: '阻塞任务',
      status: 'blocked',
      last_run_id: 'run-blocked',
      source_session_id: 'session-1',
      source_turn_id: 'turn-1',
      source_user_message_id: 'message-1',
    };
    const request = vi.fn((path: string) => {
      if (path.includes('/retry')) {
        return Promise.reject(new Error('模型配置不可用'));
      }
      return Promise.resolve({
        root_task_ids: [blockedTask.id],
        nodes: [{
          task: blockedTask,
          depth: 0,
          is_root: true,
          is_current_message: true,
        }],
        edges: [],
        source_session_id: 'session-1',
        source_turn_id: 'turn-1',
        source_user_message_id: 'message-1',
      });
    });
    const lookup = {
      sessionId: 'session-1',
      turnId: 'turn-1',
      sourceUserMessageId: 'message-1',
    };
    const { result } = renderHook(() => useMessageTaskGraph({
      open: true,
      messageId: 'message-1',
      lookup,
    }), { wrapper: createApiWrapper(request) });

    await waitFor(() => expect(result.current.allTasks[0]?.status).toBe('blocked'));

    let retried = true;
    await act(async () => {
      retried = await result.current.retryTask(blockedTask);
    });

    expect(retried).toBe(false);
    expect(result.current.retryError).toBe('模型配置不可用');
    expect(result.current.retryingTaskId).toBeNull();
  });
});

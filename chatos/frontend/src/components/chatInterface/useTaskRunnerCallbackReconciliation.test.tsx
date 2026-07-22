// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Message } from '../../types';
import {
  hasOutstandingTaskRunnerCallbacks,
  useTaskRunnerCallbackReconciliation,
} from './useTaskRunnerCallbackReconciliation';

const userMessage = (taskRunnerAsync: Record<string, unknown>): Message => ({
  id: 'user-1',
  sessionId: 'session-1',
  role: 'user',
  content: 'run task',
  status: 'completed',
  createdAt: new Date('2026-07-19T00:00:00Z'),
  metadata: {
    task_runner_async: {
      mode: 'contact_async',
      ...taskRunnerAsync,
    },
  },
});

const projectExecutionUserMessage = (taskRunnerAsync: Record<string, unknown>): Message => ({
  ...userMessage(taskRunnerAsync),
  metadata: {
    task_runner_async: {
      mode: 'project_requirement_execution',
      ...taskRunnerAsync,
    },
  },
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('task runner callback reconciliation', () => {
  it('tracks created tasks until every task has a terminal callback', () => {
    expect(hasOutstandingTaskRunnerCallbacks([
      userMessage({
        created_task_ids: ['task-1', 'task-2'],
        terminal_task_ids: ['task-1'],
        overall_status: 'completed',
      }),
    ])).toBe(true);
    expect(hasOutstandingTaskRunnerCallbacks([
      userMessage({
        created_task_ids: ['task-1', 'task-2'],
        terminal_task_ids: ['task-1', 'task-2'],
        overall_status: 'completed',
      }),
    ])).toBe(false);
  });

  it('also reconciles project requirement execution before the first callback arrives', () => {
    expect(hasOutstandingTaskRunnerCallbacks([
      projectExecutionUserMessage({
        created_task_ids: ['task-1', 'task-2'],
        terminal_task_ids: [],
        overall_status: 'processing',
      }),
    ])).toBe(true);
  });

  it('polls while a task callback is outstanding and stops when disabled', async () => {
    vi.useFakeTimers();
    const syncSessionMessages = vi.fn().mockResolvedValue(undefined);
    const { rerender } = renderHook(
      ({ enabled }) => useTaskRunnerCallbackReconciliation({
        enabled,
        sessionId: 'session-1',
        syncSessionMessages,
        intervalMs: 1_000,
      }),
      { initialProps: { enabled: true } },
    );

    expect(syncSessionMessages).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_100);
    });
    expect(syncSessionMessages).toHaveBeenCalledTimes(3);

    rerender({ enabled: false });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(syncSessionMessages).toHaveBeenCalledTimes(3);
  });
});

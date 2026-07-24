// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { canRetryMessageTask, MessageTaskDetailModal } from './MessageTaskDetailModal';

afterEach(() => cleanup());

describe('message task node retry', () => {
  it('shows retry only for a failed task with a run and retries that task', async () => {
    const user = userEvent.setup();
    const task = {
      id: 'task-failed',
      title: '失败任务',
      status: 'failed',
      last_run_id: 'run-failed',
    };
    const onRetry = vi.fn();

    expect(canRetryMessageTask(task)).toBe(true);
    expect(canRetryMessageTask({ ...task, status: 'blocked' })).toBe(false);
    expect(canRetryMessageTask({ ...task, last_run_id: null })).toBe(false);

    render(
      <MessageTaskDetailModal
        task={task}
        onRetry={onRetry}
        onClose={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '重试此任务' }));

    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onRetry).toHaveBeenCalledWith(task);
    expect(screen.getByText('仅重新运行此节点；成功后，满足依赖条件的后续节点会继续调度。')).toBeTruthy();
  });
});

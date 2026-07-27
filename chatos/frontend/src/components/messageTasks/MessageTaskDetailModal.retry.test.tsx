// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { canRetryMessageTask, MessageTaskDetailModal } from './MessageTaskDetailModal';

afterEach(() => cleanup());

describe('message task node retry', () => {
  it('shows retry for a failed task with a run and retries that task', async () => {
    const user = userEvent.setup();
    const task = {
      id: 'task-failed',
      title: '失败任务',
      status: 'failed',
      last_run_id: 'run-failed',
    };
    const onRetry = vi.fn();

    expect(canRetryMessageTask(task)).toBe(true);
    expect(canRetryMessageTask({ ...task, status: 'blocked' })).toBe(true);
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

  it('shows the blocker and submits user guidance when retrying a blocked node', async () => {
    const user = userEvent.setup();
    const task = {
      id: 'task-blocked',
      title: '阻塞任务',
      status: 'blocked',
      last_run_id: 'run-blocked',
      last_run: {
        id: 'run-blocked',
        error_message: '缺少生产环境回调地址',
      },
    };
    const onRetry = vi.fn();

    render(
      <MessageTaskDetailModal
        task={task}
        onRetry={onRetry}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('缺少生产环境回调地址')).toBeTruthy();
    await user.type(
      screen.getByRole('textbox', { name: '补充处理意见（可选）' }),
      '回调地址已经补齐，请重新验证',
    );
    await user.click(screen.getByRole('button', { name: '重新处理此节点' }));

    expect(onRetry).toHaveBeenCalledWith(task, '回调地址已经补齐，请重新验证');
  });

  it('shows a retry request failure inside the task detail modal', () => {
    render(
      <MessageTaskDetailModal
        task={{
          id: 'task-blocked',
          title: '阻塞任务',
          status: 'blocked',
          last_run_id: 'run-blocked',
        }}
        retryError="模型配置不可用，无法重新处理此节点"
        onRetry={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole('alert').textContent).toContain('模型配置不可用，无法重新处理此节点');
  });

  it('requires an execution service before retrying a multi-application task', async () => {
    const user = userEvent.setup();
    const task = {
      id: 'task-multi-app',
      title: '多应用任务',
      status: 'failed',
      last_run_id: 'run-multi-app',
    };
    const onRetry = vi.fn();
    const onExecutionServiceChange = vi.fn();
    const { rerender } = render(
      <MessageTaskDetailModal
        task={task}
        executionServiceOptions={[
          { id: 'mdm-service', label: 'MDM Service (mdm-service)' },
          { id: 'web-prototype', label: 'Web Prototype (web-prototype)' },
        ]}
        executionServiceRequired
        selectedExecutionServiceId=""
        onExecutionServiceChange={onExecutionServiceChange}
        onRetry={onRetry}
        onClose={vi.fn()}
      />,
    );

    expect((screen.getByRole('button', { name: '重试此任务' }) as HTMLButtonElement).disabled)
      .toBe(true);
    await user.selectOptions(screen.getByRole('combobox', { name: '执行服务' }), 'mdm-service');
    expect(onExecutionServiceChange).toHaveBeenCalledWith('mdm-service');

    rerender(
      <MessageTaskDetailModal
        task={task}
        executionServiceOptions={[
          { id: 'mdm-service', label: 'MDM Service (mdm-service)' },
          { id: 'web-prototype', label: 'Web Prototype (web-prototype)' },
        ]}
        executionServiceRequired
        selectedExecutionServiceId="mdm-service"
        onExecutionServiceChange={onExecutionServiceChange}
        onRetry={onRetry}
        onClose={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: '重试此任务' }));
    expect(onRetry).toHaveBeenCalledWith(task, undefined, 'mdm-service');
  });
});

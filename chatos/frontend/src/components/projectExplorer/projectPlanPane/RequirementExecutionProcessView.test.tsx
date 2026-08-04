// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  RequirementExecutionProcessActions,
  RequirementExecutionProcessSidebar,
} from './RequirementExecutionProcessView';

afterEach(() => cleanup());

const noop = vi.fn();

describe('requirement execution process actions', () => {
  it('keeps rerun disabled and visibly loading while old batch cancellation is settling', () => {
    render(
      <RequirementExecutionProcessActions
        actuallyStarted
        canRegenerate={false}
        canRerun
        cancellationSettling
        confirming={false}
        executionPaused={false}
        graphReady={false}
        hasActiveRuns
        runtimeEnvironmentReady
        runtimeEnvironmentStatus="ready"
        onCancelRequirementExecution={noop}
        onClose={noop}
        onConfirmExecution={noop}
        onOpenCancelConfirm={noop}
        onOpenDiscardConfirm={noop}
        onOpenFailedTaskRetry={noop}
        onOpenRerunConfirm={noop}
        onRegenerate={noop}
        onTogglePause={noop}
        pausing={false}
        phase="stopped"
        queuedTaskCount={1}
        rerunSettling
        rerunning={false}
        retryableFailedTaskCount={0}
        retryingTaskId={null}
        revising={false}
        runningTaskCount={1}
        stopping={false}
      />,
    );

    const rerunButton = screen.getByRole('button', { name: '等待取消完成' });
    expect((rerunButton as HTMLButtonElement).disabled).toBe(true);
    expect(rerunButton.getAttribute('aria-busy')).toBe('true');
  });

  it('keeps execution disabled until the project sandbox environment is ready', () => {
    render(
      <RequirementExecutionProcessActions
        actuallyStarted={false}
        canRegenerate={false}
        canRerun={false}
        cancellationSettling={false}
        confirming={false}
        executionPaused={false}
        graphReady
        hasActiveRuns={false}
        runtimeEnvironmentReady={false}
        runtimeEnvironmentStatus="analyzing"
        onCancelRequirementExecution={noop}
        onClose={noop}
        onConfirmExecution={noop}
        onOpenCancelConfirm={noop}
        onOpenDiscardConfirm={noop}
        onOpenFailedTaskRetry={noop}
        onOpenRerunConfirm={noop}
        onRegenerate={noop}
        onTogglePause={noop}
        pausing={false}
        phase="awaiting_confirmation"
        queuedTaskCount={0}
        rerunSettling={false}
        rerunning={false}
        retryableFailedTaskCount={0}
        retryingTaskId={null}
        revising={false}
        runningTaskCount={0}
        stopping={false}
      />,
    );

    const executeButton = screen.getByRole('button', { name: '初始化环境中' });
    expect((executeButton as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText('流程图已就绪，执行环境正在初始化，完成后自动开放执行。')).toBeTruthy();
  });
});

describe('requirement execution process sidebar', () => {
  it('hides replanning controls after execution starts', () => {
    render(
      <RequirementExecutionProcessSidebar
        canRevise={false}
        cancellationSettling={false}
        feedback=""
        onFeedbackChange={noop}
        onSubmitFeedback={noop}
        phase="running"
        phaseText={{ title: '任务正在执行', detail: '按依赖顺序运行' }}
        processEntries={[]}
        revising={false}
        taskCount={2}
        terminal={false}
      />,
    );

    expect(screen.getByText('执行计划已冻结')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '发送并调整' })).toBeNull();
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it('shows replanning controls before execution or after the batch stops', () => {
    render(
      <RequirementExecutionProcessSidebar
        canRevise
        cancellationSettling={false}
        feedback="补充验证"
        onFeedbackChange={noop}
        onSubmitFeedback={noop}
        phase="stopped"
        phaseText={{ title: '执行已停止', detail: '可以调整执行计划' }}
        processEntries={[]}
        revising={false}
        taskCount={2}
        terminal
      />,
    );

    expect(screen.getByRole('textbox')).toBeTruthy();
    expect(screen.getByRole('button', { name: '发送并调整' })).toBeTruthy();
    expect(screen.queryByText('执行计划已冻结')).toBeNull();
  });
});

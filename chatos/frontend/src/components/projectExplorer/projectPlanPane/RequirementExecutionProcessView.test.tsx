// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  RequirementExecutionGraphSurface,
  RequirementExecutionPlannerProcessModal,
  RequirementExecutionProcessActions,
  RequirementExecutionProcessSidebar,
} from './RequirementExecutionProcessView';

vi.mock('../../messageTasks/MessageTaskGraphPanel', () => ({
  MessageTaskGraphPanel: () => <div>任务依赖图内容</div>,
}));

vi.mock('../../messageTasks/RunProcessTimeline', () => ({
  RunProcessTimeline: () => <div>规划运行时间线内容</div>,
}));

afterEach(() => cleanup());

const noop = vi.fn();

describe('requirement execution process actions', () => {
  it('offers both regeneration and rerun after an executed batch is fully cancelled', () => {
    render(
      <RequirementExecutionProcessActions
        actuallyStarted
        canRegenerate
        canRerun
        cancellationSettling={false}
        confirming={false}
        executionPaused={false}
        graphReady={false}
        hasActiveRuns={false}
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
        isLocalExecution={false}
        queuedTaskCount={0}
        rerunSettling={false}
        rerunning={false}
        retryableFailedTaskCount={3}
        retryingTaskId={null}
        revising={false}
        runningTaskCount={0}
        stopping={false}
      />,
    );

    expect(screen.getByRole('button', { name: '重新生成执行流程' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '重新执行' })).toBeTruthy();
  });

  it('offers regeneration while a completed graph is still awaiting confirmation', () => {
    render(
      <RequirementExecutionProcessActions
        actuallyStarted={false}
        canRegenerate
        canRerun={false}
        cancellationSettling={false}
        confirming={false}
        executionPaused={false}
        graphReady
        hasActiveRuns={false}
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
        isLocalExecution={false}
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

    expect(screen.getByRole('button', { name: '重新生成执行流程' })).toBeTruthy();
  });

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
        isLocalExecution={false}
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

});

describe('requirement execution process sidebar', () => {
  it('hides replanning controls after execution starts', () => {
    render(
      <RequirementExecutionProcessSidebar
        canRevise={false}
        cancellationSettling={false}
        feedback=""
        onFeedbackChange={noop}
        onOpenPlannerProcess={noop}
        onSubmitFeedback={noop}
        phase="running"
        phaseText={{ title: '任务正在执行', detail: '按依赖顺序运行' }}
        plannerActive={false}
        plannerProcessMessageCount={4}
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
        onOpenPlannerProcess={noop}
        onSubmitFeedback={noop}
        phase="stopped"
        phaseText={{ title: '执行已停止', detail: '可以调整执行计划' }}
        plannerActive={false}
        plannerProcessMessageCount={4}
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

  it('opens detailed planner process without replacing the graph surface', () => {
    const onOpenPlannerProcess = vi.fn();
    render(
      <RequirementExecutionProcessSidebar
        canRevise
        cancellationSettling={false}
        feedback=""
        onFeedbackChange={noop}
        onOpenPlannerProcess={onOpenPlannerProcess}
        onSubmitFeedback={noop}
        phase="building_graph"
        phaseText={{ title: '正在生成完整执行流程', detail: '正在创建任务节点' }}
        plannerActive
        plannerProcessMessageCount={6}
        processEntries={[]}
        revising={false}
        taskCount={2}
        terminal={false}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /详细过程 6/ }));
    expect(onOpenPlannerProcess).toHaveBeenCalledTimes(1);
  });
});

describe('requirement execution workspace surface', () => {
  it('keeps the graph visible even before the first graph node exists', () => {
    render(
      <RequirementExecutionGraphSurface
        actionError={null}
        actionMessage={null}
        containerRef={{ current: null }}
        dependencyCount={0}
        graphPanelProps={{} as never}
        runRecordCount={0}
        syncError={null}
        taskCount={0}
      />,
    );

    expect(screen.getByText('任务依赖图内容')).toBeTruthy();
    expect(screen.getByText('实时执行流程图')).toBeTruthy();
  });

  it('renders persisted planner timeline records in a separate modal', () => {
    render(
      <RequirementExecutionPlannerProcessModal
        active
        error={null}
        items={[{
          content: '正在读取需求文档',
          createdAt: new Date('2026-08-14T07:00:00Z'),
          id: 'model-1',
          label: '模型过程',
          type: 'model',
        }]}
        loading={false}
        onClose={noop}
        processMessageCount={3}
      />,
    );

    expect(screen.getByText('规划运行时间线内容')).toBeTruthy();
    expect(screen.getByText(/已显示 3 条规划过程/)).toBeTruthy();
  });
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { MessageTaskRunnerGraphResponse } from '../../lib/api/client/types';
import { MessageTaskGraphPanel } from './MessageTaskGraphPanel';

afterEach(cleanup);

const graph: MessageTaskRunnerGraphResponse = {
  root_task_ids: ['task-1'],
  source_session_id: 'session-1',
  source_turn_id: 'turn-1',
  source_user_message_id: 'message-1',
  nodes: [
    {
      depth: 0,
      is_root: true,
      is_current_message: true,
      task: {
        id: 'task-1',
        title: '测试任务',
        status: 'ready',
        prerequisite_task_ids: [],
      },
    },
  ],
  edges: [],
};

const connectedGraph: MessageTaskRunnerGraphResponse = {
  root_task_ids: ['task-d'],
  source_session_id: 'session-1',
  source_turn_id: 'turn-1',
  source_user_message_id: 'message-1',
  nodes: [
    {
      depth: 3,
      is_root: false,
      is_current_message: true,
      task: { id: 'task-a', title: '任务 A', status: 'completed', prerequisite_task_ids: [] },
    },
    {
      depth: 2,
      is_root: false,
      is_current_message: true,
      task: { id: 'task-b', title: '任务 B', status: 'completed', prerequisite_task_ids: ['task-a'] },
    },
    {
      depth: 1,
      is_root: false,
      is_current_message: true,
      task: { id: 'task-c', title: '任务 C', status: 'ready', prerequisite_task_ids: ['task-b'] },
    },
    {
      depth: 0,
      is_root: true,
      is_current_message: true,
      task: { id: 'task-d', title: '任务 D', status: 'ready', prerequisite_task_ids: ['task-c'] },
    },
  ],
  edges: [],
};

describe('MessageTaskGraphPanel zoom controls', () => {
  it('zooms the nodes and edges canvas through buttons and modified wheel input', () => {
    render(
      <MessageTaskGraphPanel
        graph={graph}
        loading={false}
        error={null}
        loadingRunId={null}
        loadingChangesRunId={null}
        panelWidth={1_200}
        loadingProcessTaskId={null}
        onOpenDetail={vi.fn()}
        onOpenProcessLog={vi.fn()}
        onOpenRun={vi.fn()}
        onOpenChanges={vi.fn()}
      />,
    );

    const canvas = screen.getByTestId('message-task-graph-canvas');
    expect(canvas).toHaveStyle({ transform: 'scale(1)' });

    fireEvent.click(screen.getByRole('button', { name: '放大流程图' }));
    expect(canvas).toHaveStyle({ transform: 'scale(1.1)' });
    expect(screen.getByRole('button', { name: '重置流程图缩放' })).toHaveTextContent('110%');

    fireEvent.wheel(canvas, { ctrlKey: true, deltaY: 120 });
    expect(canvas).toHaveStyle({ transform: 'scale(1)' });

    fireEvent.click(screen.getByRole('button', { name: '缩小流程图' }));
    expect(canvas).toHaveStyle({ transform: 'scale(0.9)' });

    fireEvent.click(screen.getByRole('button', { name: '重置流程图缩放' }));
    expect(canvas).toHaveStyle({ transform: 'scale(1)' });
  });

  it('animates only the selected task and its directly connected tasks and edges', () => {
    render(
      <MessageTaskGraphPanel
        graph={connectedGraph}
        loading={false}
        error={null}
        loadingRunId={null}
        loadingChangesRunId={null}
        panelWidth={1_200}
        loadingProcessTaskId={null}
        onOpenDetail={vi.fn()}
        onOpenProcessLog={vi.fn()}
        onOpenRun={vi.fn()}
        onOpenChanges={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId('message-task-node-task-b'));

    expect(screen.getByTestId('message-task-node-task-a')).toHaveClass('message-task-focus-card');
    expect(screen.getByTestId('message-task-node-task-b')).toHaveClass(
      'message-task-focus-card',
      'message-task-focus-card-active',
    );
    expect(screen.getByTestId('message-task-node-task-c')).toHaveClass('message-task-focus-card');
    expect(screen.getByTestId('message-task-node-task-d')).not.toHaveClass('message-task-focus-card');

    expect(screen.getByTestId('message-task-edge-task-a->task-b')).toHaveClass('message-task-focus-edge');
    expect(screen.getByTestId('message-task-edge-task-b->task-c')).toHaveClass('message-task-focus-edge');
    expect(screen.getByTestId('message-task-edge-task-c->task-d')).not.toHaveClass('message-task-focus-edge');

    fireEvent.click(screen.getByTestId('message-task-node-task-b'));
    expect(screen.getByTestId('message-task-node-task-a')).not.toHaveClass('message-task-focus-card');
  });

  it('keeps card action buttons from toggling node focus', () => {
    const onOpenDetail = vi.fn();
    render(
      <MessageTaskGraphPanel
        graph={graph}
        loading={false}
        error={null}
        loadingRunId={null}
        loadingChangesRunId={null}
        panelWidth={1_200}
        loadingProcessTaskId={null}
        onOpenDetail={onOpenDetail}
        onOpenProcessLog={vi.fn()}
        onOpenRun={vi.fn()}
        onOpenChanges={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '详情' }));

    expect(onOpenDetail).toHaveBeenCalledOnce();
    expect(screen.getByTestId('message-task-node-task-1')).not.toHaveClass('message-task-focus-card');
  });

  it('selects the blocked stage automatically and exposes its handling action', () => {
    const onOpenDetail = vi.fn();
    const blockedStageGraph: MessageTaskRunnerGraphResponse = {
      root_task_ids: ['review'],
      nodes: [
        {
          depth: 1,
          is_root: false,
          is_current_message: true,
          task: {
            id: 'implement',
            title: '实现订单校验',
            status: 'succeeded',
            prerequisite_task_ids: [],
            input_payload: { project_task_id: 'project-task-1' },
          },
        },
        {
          depth: 0,
          is_root: true,
          is_current_message: true,
          task: {
            id: 'review',
            title: 'Review 订单校验',
            status: 'blocked',
            last_run_id: 'run-review',
            prerequisite_task_ids: ['implement'],
            input_payload: { project_task_id: 'project-task-1' },
          },
        },
      ],
      edges: [],
    };

    render(
      <MessageTaskGraphPanel
        graph={blockedStageGraph}
        loading={false}
        error={null}
        loadingRunId={null}
        loadingChangesRunId={null}
        panelWidth={1_200}
        loadingProcessTaskId={null}
        onOpenDetail={onOpenDetail}
        onOpenProcessLog={vi.fn()}
        onOpenRun={vi.fn()}
        onOpenChanges={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '处理阻塞' }));
    expect(onOpenDetail).toHaveBeenCalledWith(expect.objectContaining({ id: 'review' }));
  });
});

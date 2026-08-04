// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MessageTaskRunnerRunDetailResponse } from '../../lib/api/client/types';
import { MessageTaskRunDetailModal } from './MessageTaskRunDetailModal';
import { MessageTaskProcessLogModal } from './MessageTaskDetailModal';

const detail: MessageTaskRunnerRunDetailResponse = {
  task: {
    id: 'task-1',
    title: '整理需求',
  },
  run: {
    id: 'run-1',
    task_id: 'task-1',
    status: 'running',
    started_at: '2026-07-21T08:00:00Z',
  },
  events: [{
    id: 'event-start',
    run_id: 'run-1',
    event_type: 'tools_start',
    created_at: '2026-07-21T08:00:00Z',
    payload: [{
      id: 'call-search',
      function: {
        name: 'code_maintainer_read_search_text',
        arguments: JSON.stringify({ path: 'src', pattern: 'completed' }),
      },
    }],
  }],
  events_total: 60,
  events_limit: 40,
  events_offset: 0,
  events_has_more: true,
};

const processDetail: MessageTaskRunnerRunDetailResponse = {
  ...detail,
  task: {
    id: 'task-1',
    title: '整理需求',
    process_log: '',
  },
  process_tasks: [{
    id: 'checklist-1',
    title: '确认页面初始状态',
    status: 'doing',
    process_log: '使用浏览器工具检查当前页，不展示底层工具事件。',
  }],
};

describe('MessageTaskRunDetailModal', () => {
  afterEach(cleanup);

  it('labels run events as diagnostics and keeps raw events collapsed by default', () => {
    const onLoadMoreEvents = vi.fn();
    render(
      <MessageTaskRunDetailModal
        detail={detail}
        onClose={vi.fn()}
        onLoadMoreEvents={onLoadMoreEvents}
      />,
    );

    expect(screen.getByText('运行事件时间线（诊断）')).toBeInTheDocument();
    expect(screen.getByText('正在 src 中搜索「completed」')).toBeInTheDocument();
    expect(screen.getByText('原始运行事件（诊断）')).toBeInTheDocument();
    expect(screen.queryByText('开始调用工具')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '加载更多诊断事件（剩余 59）' }));
    expect(onLoadMoreEvents).toHaveBeenCalledTimes(1);
  });

  it('shows the program-resolved sandbox and Harness execution location', () => {
    render(
      <MessageTaskRunDetailModal
        detail={{
          ...detail,
          run: {
            ...detail.run,
            input_snapshot: {
              execution_environment_mode: 'cloud',
              sandbox_enabled: true,
              sandbox: {
                provider: 'cloud',
                sandbox_id: 'sandbox-1',
                lease_id: 'lease-1',
                expires_at: '2026-07-21T10:00:00Z',
              },
              harness: {
                repo_path: 'projects/game-1',
                base_branch: 'main',
                run_branch: 'chatos/runs/run-1',
                status: 'prepared',
              },
            },
            report: {
              output: {
                harness: {
                  status: 'committed',
                  result_commit: 'abc123',
                },
              },
            },
          },
        }}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('执行位置')).toBeInTheDocument();
    expect(screen.getByText('云端')).toBeInTheDocument();
    expect(screen.getByText('已准备')).toBeInTheDocument();
    expect(screen.getByText('sandbox-1')).toBeInTheDocument();
    expect(screen.getByText('lease-1')).toBeInTheDocument();
    expect(screen.getByText('projects/game-1')).toBeInTheDocument();
    expect(screen.getByText('chatos/runs/run-1')).toBeInTheDocument();
    expect(screen.getByText('committed')).toBeInTheDocument();
    expect(screen.getByText('abc123')).toBeInTheDocument();
  });
});

describe('MessageTaskProcessLogModal', () => {
  afterEach(cleanup);

  it('renders only AI-authored process task notes, not raw run events', () => {
    render(
      <MessageTaskProcessLogModal
        task={{ id: 'task-1', title: '整理需求', process_log: '' }}
        runDetail={processDetail}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('执行过程')).toBeInTheDocument();
    expect(screen.getByText('确认页面初始状态')).toBeInTheDocument();
    expect(screen.getByText('使用浏览器工具检查当前页，不展示底层工具事件。')).toBeInTheDocument();
    expect(screen.queryByText('正在 src 中搜索「completed」')).not.toBeInTheDocument();
    expect(screen.queryByText('暂无执行过程')).not.toBeInTheDocument();
  });

  it('renders task process_log together with AI-authored process task notes', () => {
    render(
      <MessageTaskProcessLogModal
        task={{ id: 'task-1', title: '整理需求', process_log: '' }}
        runDetail={{
          ...processDetail,
          task: {
            id: 'task-1',
            title: '整理需求',
            process_log: '父任务写入的过程说明。',
          },
        }}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('父任务写入的过程说明。')).toBeInTheDocument();
    expect(screen.getByText('确认页面初始状态')).toBeInTheDocument();
    expect(screen.getByText('使用浏览器工具检查当前页，不展示底层工具事件。')).toBeInTheDocument();
  });

  it('shows empty state when neither task process_log nor AI-authored process tasks exist', () => {
    render(
      <MessageTaskProcessLogModal
        task={{ id: 'task-1', title: '整理需求', process_log: '' }}
        runDetail={detail}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('暂无执行过程')).toBeInTheDocument();
    expect(screen.queryByText('正在 src 中搜索「completed」')).not.toBeInTheDocument();
  });
});

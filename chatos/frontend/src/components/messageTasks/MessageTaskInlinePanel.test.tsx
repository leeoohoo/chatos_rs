// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import type { Message } from '../../types';
import { MessageTaskInlinePanel } from './MessageTaskInlinePanel';

const useMessageTasksMock = vi.fn();

vi.mock('./useMessageTasks', () => ({
  useMessageTasks: (...args: unknown[]) => useMessageTasksMock(...args),
}));

vi.mock('../LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => <div>{content}</div>,
}));

const baseTask: MessageTaskRunnerTask = {
  id: 'task-1',
  title: '真实检查当前项目实现进度',
  status: 'completed',
  description: '检查前后端和数据库实现情况。',
  objective: '确认当前项目的真实实现状态。',
  result_summary: '已经完成真实性检查。',
  process_log: '检查仓库结构。\n核对运行结果。',
  last_run_id: 'run-1',
  updated_at: '2026-08-10T07:31:00Z',
};

const processDetail: MessageTaskRunnerRunDetailResponse = {
  task: baseTask,
  run: {
    id: 'run-1',
    task_id: 'task-1',
    status: 'completed',
  },
  process_tasks: [{
    id: 'process-1',
    title: '确认仓库结构',
    status: 'completed',
    process_log: '已确认前后端目录和关键配置文件。',
  }],
  events: [],
};

const message: Message = {
  id: 'assistant-msg-1',
  role: 'assistant',
  content: '任务检查已完成。',
  createdAt: new Date('2026-08-10T07:31:00Z'),
  sessionId: 'session-1',
  status: 'completed',
  metadata: {
    conversation_turn_id: 'turn-1',
    task_runner_async: {
      source_user_message_id: 'user-msg-1',
      source_turn_id: 'turn-1',
    },
  },
};

describe('MessageTaskInlinePanel', () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  beforeEach(() => {
    useMessageTasksMock.mockReturnValue({
      tasks: [baseTask],
      loading: false,
      error: null,
      detailTask: {
        ...baseTask,
        result_summary: '已经完成真实性检查。',
      },
      runDetail: processDetail,
      loadingDetailId: null,
      loadingRunId: null,
      openDetail: vi.fn(),
      openRun: vi.fn(),
      closeDetail: vi.fn(),
      closeRun: vi.fn(),
    });
  });

  it('expands inline process details when the process button is clicked', () => {
    render(<MessageTaskInlinePanel message={message} />);

    fireEvent.click(screen.getByRole('button', { name: /查看过程/i }));

    expect(screen.getByText('执行时间线')).toBeInTheDocument();
    expect(screen.getByText('确认仓库结构')).toBeInTheDocument();
    expect(screen.getByText('已确认前后端目录和关键配置文件。')).toBeInTheDocument();
    expect(useMessageTasksMock.mock.results[0]?.value.openRun).toHaveBeenCalledWith(baseTask);
  });

  it('expands inline task details when the detail button is clicked', () => {
    render(<MessageTaskInlinePanel message={message} />);

    fireEvent.click(screen.getByRole('button', { name: /查看详情/i }));

    expect(screen.getByText('执行结果')).toBeInTheDocument();
    expect(screen.getByText('已经完成真实性检查。')).toBeInTheDocument();
    expect(screen.getByText('目标')).toBeInTheDocument();
    expect(screen.getByText('确认当前项目的真实实现状态。')).toBeInTheDocument();
    expect(useMessageTasksMock.mock.results[0]?.value.openDetail).toHaveBeenCalledWith(baseTask);
  });

  it('does not render action buttons when the message has no task state', () => {
    useMessageTasksMock.mockReturnValue({
      tasks: [],
      loading: false,
      error: null,
      detailTask: null,
      runDetail: null,
      loadingDetailId: null,
      loadingRunId: null,
      openDetail: vi.fn(),
      openRun: vi.fn(),
      closeDetail: vi.fn(),
      closeRun: vi.fn(),
    });

    render(<MessageTaskInlinePanel message={{
      ...message,
      metadata: {
        conversation_turn_id: 'turn-1',
        task_runner_async: {
          source_user_message_id: 'user-msg-1',
          source_turn_id: 'turn-1',
        },
      },
    }} />);

    expect(screen.queryByRole('button', { name: /查看过程/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /查看详情/i })).not.toBeInTheDocument();
  });
});

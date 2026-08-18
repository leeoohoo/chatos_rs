// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { MessageTaskDetailModal } from './MessageTaskDetailModal';

vi.mock('../LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => <div>{content}</div>,
}));

afterEach(() => cleanup());

describe('MessageTaskDetailModal model output', () => {
  it('shows the complete run report before the shorter result summary', () => {
    render(
      <MessageTaskDetailModal
        task={{
          id: 'task-1',
          title: '检查项目',
          result_summary: '已完成检查。',
          last_run_id: 'run-1',
          last_run: {
            id: 'run-1',
            report: { content: '# 完整报告\n\n这里是模型的全部输出。' },
          },
        }}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('模型输出')).toBeInTheDocument();
    expect(screen.getByText(/# 完整报告/)).toBeInTheDocument();
    expect(screen.getByText('执行结果摘要')).toBeInTheDocument();
    expect(screen.getByText('已完成检查。')).toBeInTheDocument();
  });

  it('shows identical report and summary only once', () => {
    render(
      <MessageTaskDetailModal
        task={{
          id: 'task-1',
          title: '检查项目',
          result_summary: '同一份输出',
          last_run_id: 'run-1',
          last_run: {
            id: 'run-1',
            report: { output: '同一份输出' },
          },
        }}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('模型输出')).toBeInTheDocument();
    expect(screen.queryByText('执行结果摘要')).not.toBeInTheDocument();
    expect(screen.getAllByText('同一份输出')).toHaveLength(1);
  });
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MessageTaskRunnerRunDetailResponse } from '../../lib/api/client/types';
import { MessageTaskRunDetailModal } from './MessageTaskRunDetailModal';

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

describe('MessageTaskRunDetailModal', () => {
  afterEach(cleanup);

  it('uses the process timeline and keeps raw events collapsed by default', () => {
    const onLoadMoreEvents = vi.fn();
    render(
      <MessageTaskRunDetailModal
        detail={detail}
        onClose={vi.fn()}
        onLoadMoreEvents={onLoadMoreEvents}
      />,
    );

    expect(screen.getByText('执行过程')).toBeInTheDocument();
    expect(screen.getByText('正在 src 中搜索「completed」')).toBeInTheDocument();
    expect(screen.getByText('原始运行事件（诊断）')).toBeInTheDocument();
    expect(screen.queryByText('开始调用工具')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '加载更多过程（剩余 59）' }));
    expect(onLoadMoreEvents).toHaveBeenCalledTimes(1);
  });
});

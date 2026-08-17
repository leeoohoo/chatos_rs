// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import { RunProcessTimeline } from './RunProcessTimeline';
import { buildRunProcessTimelineItems } from './runProcessTimelineModel';

const events: MessageTaskRunnerRunEvent[] = [
  {
    id: 'event-start',
    run_id: 'run-1',
    event_type: 'tools_start',
    created_at: '2026-07-21T08:00:00Z',
    payload: [{
      id: 'call-read',
      function: {
        name: 'code_maintainer_read_read_file_raw',
        arguments: JSON.stringify({ path: 'src/model.ts' }),
      },
    }],
  },
  {
    id: 'event-result',
    run_id: 'run-1',
    event_type: 'tool_stream',
    created_at: '2026-07-21T08:00:01Z',
    payload: {
      tool_call_id: 'call-read',
      name: 'code_maintainer_read_read_file_raw',
      success: true,
      is_error: false,
      is_stream: false,
      content: 'file content',
    },
  },
];

describe('RunProcessTimeline', () => {
  afterEach(cleanup);

  it('renders semantic actions and reveals parameters and results on demand', async () => {
    render(<RunProcessTimeline items={buildRunProcessTimelineItems(events)} />);

    expect(screen.getByText('已读取 src/model.ts')).toBeInTheDocument();
    expect(screen.queryByText(/code_maintainer_read_read_file_raw/)).not.toBeInTheDocument();
    expect(screen.queryByText(/call-read/)).not.toBeInTheDocument();
    expect(screen.queryByText('file content')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /已读取 src\/model\.ts/ }));

    expect(screen.getByText('主要参数')).toBeInTheDocument();
    expect(screen.getByText('返回结果')).toBeInTheDocument();
    expect(await screen.findByText('file content')).toBeInTheDocument();
  });

  it('shows tool execution as the current activity while a process wait call is pending', () => {
    const processWaitEvents: MessageTaskRunnerRunEvent[] = [{
      id: 'event-wait',
      run_id: 'run-1',
      event_type: 'tools_start',
      created_at: '2026-07-21T08:00:00Z',
      payload: [{
        id: 'call-wait',
        function: {
          name: 'sandbox_terminal_controller_process_wait',
          arguments: JSON.stringify({ process_id: 'process-1', timeout_ms: 600000 }),
        },
      }],
    }];

    render(<RunProcessTimeline items={buildRunProcessTimelineItems(processWaitEvents)} />);

    expect(screen.getByRole('status')).toHaveTextContent('当前正在等待工具进程完成');
    expect(screen.getByText('进行中')).toBeInTheDocument();
  });

  it('does not keep a completed background command marked as running', () => {
    const backgroundEvents: MessageTaskRunnerRunEvent[] = [{
      id: 'event-background-start',
      run_id: 'run-1',
      event_type: 'tools_start',
      created_at: '2026-07-21T08:00:00Z',
      payload: [{
        id: 'call-background',
        function: {
          name: 'sandbox_terminal_controller_execute_command',
          arguments: JSON.stringify({ command: 'npm run build', background: true }),
        },
      }],
    }, {
      id: 'event-background-result',
      run_id: 'run-1',
      event_type: 'tool_stream',
      created_at: '2026-07-21T08:00:01Z',
      payload: {
        tool_call_id: 'call-background',
        name: 'sandbox_terminal_controller_execute_command',
        success: true,
        is_error: false,
        is_stream: false,
        result: {
          background: true,
          busy: true,
          process_id: 'process-1',
        },
      },
    }];

    render(<RunProcessTimeline items={buildRunProcessTimelineItems(backgroundEvents)} />);

    expect(screen.queryByText('进行中')).not.toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(screen.getByText('已执行 npm run build')).toBeInTheDocument();
  });
});

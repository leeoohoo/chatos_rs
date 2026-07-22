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
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { renderTimelineCard } from './ConversationProcessTimelineCards';
import type { TimelineItem } from './ConversationProcessTimelineModel';

const toolCallItem = (id: string, path: string): TimelineItem => ({
  createdAt: new Date('2026-07-21T00:00:00Z'),
  error: '',
  hasResult: true,
  id,
  result: `content of ${path}`,
  status: 'completed',
  toolCall: {
    id: `internal-${id}`,
    messageId: 'assistant-process',
    name: 'code_maintainer_read_read_file_raw',
    arguments: { path },
    createdAt: new Date('2026-07-21T00:00:00Z'),
  },
  type: 'tool_call',
});

describe('ConversationProcessTimelineCards', () => {
  afterEach(cleanup);

  it('shows user-facing actions and keeps internal tool details hidden', async () => {
    render(<div>{renderTimelineCard(toolCallItem('tool-1', 'src/model.ts'))}</div>);

    expect(screen.getByText('已读取 src/model.ts')).toBeInTheDocument();
    expect(screen.queryByText(/code_maintainer_read_read_file_raw/)).not.toBeInTheDocument();
    expect(screen.queryByText(/internal-tool-1/)).not.toBeInTheDocument();
    expect(screen.queryByText(/调用 ID/)).not.toBeInTheDocument();
    expect(screen.queryByText('content of src/model.ts')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /已读取 src\/model\.ts/ }));

    expect(screen.getByText('主要参数')).toBeInTheDocument();
    expect(screen.getByText('返回结果')).toBeInTheDocument();
    expect(await screen.findByText('content of src/model.ts')).toBeInTheDocument();
  });

  it('renders multiple tool calls as separate action rows', () => {
    render(
      <div>
        {renderTimelineCard(toolCallItem('tool-1', 'src/a.ts'))}
        {renderTimelineCard(toolCallItem('tool-2', 'src/b.ts'))}
      </div>,
    );

    expect(screen.getByText('已读取 src/a.ts')).toBeInTheDocument();
    expect(screen.getByText('已读取 src/b.ts')).toBeInTheDocument();
  });
});

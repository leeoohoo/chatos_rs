// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Message } from '../../types';
import type { RenderSegment } from './types';
import { MessageContentRenderer } from './MessageContentRenderer';

vi.mock('../LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({
    content,
    isStreaming,
  }: {
    content: string;
    isStreaming?: boolean;
  }) => (
    <div data-streaming={isStreaming === true ? 'true' : 'false'} data-testid="markdown">
      {content}
    </div>
  ),
}));

vi.mock('./ToolCallTimeline', () => ({
  ToolCallTimeline: () => <div data-testid="tool-call-timeline" />,
}));

const buildMessage = (overrides: Partial<Message> = {}): Message => ({
  id: 'assistant-1',
  sessionId: 'session-1',
  role: 'assistant',
  content: '重复内容重复内容',
  status: 'completed',
  createdAt: new Date('2026-05-07T08:00:00Z'),
  ...overrides,
});

describe('MessageContentRenderer', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders a single collapsed body for repeated text segments split by process segments', () => {
    const renderContentSegments: RenderSegment[] = [
      { type: 'text', content: '最终回答' },
      { type: 'thinking', content: '推理片段' },
      { type: 'text', content: '最终回答' },
      { type: 'tool_call', toolCallId: 'tool-1' },
      { type: 'text', content: '最终回答，补充说明' },
    ];

    render(
      <MessageContentRenderer
        message={buildMessage()}
        isLast
        isStreaming={false}
        renderContentSegments={renderContentSegments}
        toolCalls={[]}
        toolCallsById={new Map()}
        collapseAssistantProcessByDefault
        onApplyCode={() => {}}
      />,
    );

    const markdownBlocks = screen.getAllByTestId('markdown');
    expect(markdownBlocks).toHaveLength(1);
    expect(markdownBlocks[0]).toHaveTextContent('最终回答，补充说明');
    expect(screen.queryByText('最终回答最终回答')).not.toBeInTheDocument();
    expect(screen.queryByTestId('tool-call-timeline')).not.toBeInTheDocument();
  });
});

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ToolCall } from '../../types';
import { ToolCallTimeline } from './ToolCallTimeline';

vi.mock('../ToolCallRenderer', () => ({
  ToolCallRenderer: () => <div data-testid="tool-call-renderer" />,
}));

const buildToolCall = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  id: 'tool_1',
  messageId: 'msg_1',
  name: 'web_extract',
  arguments: {},
  result: {},
  createdAt: new Date('2026-04-15T10:00:00Z'),
  ...overrides,
});

describe('ToolCallTimeline', () => {
  afterEach(() => {
    cleanup();
  });

  it('uses shortened display names in the collapsed summary', () => {
    render(
      <ToolCallTimeline
        toolCalls={[
          buildToolCall({ id: 'tool_1', name: 'code_maintainer_read_search_text' }),
          buildToolCall({ id: 'tool_2', name: 'code_maintainer_read_read_file_range' }),
        ]}
      />,
    );

    expect(screen.getByText('@search_text · @read_file_range')).toBeInTheDocument();
    expect(screen.queryByText('@code_maintainer_read_search_text · @code_maintainer_read_read_file_range')).not.toBeInTheDocument();
  });
});

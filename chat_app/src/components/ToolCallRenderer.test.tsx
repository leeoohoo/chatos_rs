// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ToolCall } from '../types';
import ToolCallRenderer from './ToolCallRenderer';

vi.mock('./LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => (
    <div data-testid="lazy-markdown">{content}</div>
  ),
}));

const buildToolCall = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  id: 'tool_1',
  messageId: 'msg_1',
  name: 'web_extract',
  arguments: { url: 'https://example.com' },
  result: {},
  createdAt: new Date('2026-04-15T10:00:00Z'),
  ...overrides,
});

describe('ToolCallRenderer summaries', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders web backend and extract summary cards from structured result', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          result: {
            backend: 'jina',
            fallback_used: true,
            provider_attempts: [{ provider: 'jina' }, { provider: 'scrape' }],
            extract_summary: {
              page_count: 3,
              truncated_page_count: 1,
              total_omitted_chars: 5000,
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const webCard = screen
      .getByText('Web backend')
      .closest('.tool-summary-card') as HTMLElement;
    expect(webCard).toBeInTheDocument();
    expect(within(webCard).getByText('jina')).toBeInTheDocument();
    expect(within(webCard).getByText('yes')).toBeInTheDocument();
    expect(within(webCard).getByText('2')).toBeInTheDocument();

    const extractCard = screen
      .getByText('Extract summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(extractCard).toBeInTheDocument();
    expect(within(extractCard).getByText('3')).toBeInTheDocument();
    expect(within(extractCard).getByText('1')).toBeInTheDocument();
    expect(within(extractCard).getByText('5000')).toBeInTheDocument();
  });

  it('renders process summary card with extended state fields', () => {
    render(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'process',
          result: {
            wait_status: 'completed',
            terminal_id: 'terminal-123',
            process_id: 'process-123',
            busy: false,
            completed: true,
            timed_out: false,
            processes: [{ id: 'p1' }, { id: 'p2' }],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看详情' }));

    const processCard = screen
      .getByText('Process summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(processCard).toBeInTheDocument();
    const statusRow = within(processCard)
      .getByText('status')
      .closest('.tool-summary-row') as HTMLElement;
    expect(statusRow).toBeInTheDocument();
    expect(within(statusRow).getByText('completed')).toBeInTheDocument();
    expect(within(processCard).getByText('terminal-123')).toBeInTheDocument();
    expect(within(processCard).getByText('process-123')).toBeInTheDocument();
    expect(within(processCard).getAllByText('no').length).toBeGreaterThan(0);
    expect(within(processCard).getAllByText('yes').length).toBeGreaterThan(0);
    expect(within(processCard).getByText('2')).toBeInTheDocument();
  });
});

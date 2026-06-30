import { fireEvent, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  buildToolCall,
  buildToolResultMessage,
  renderWithEnglishI18n,
  ToolCallRenderer,
  type Message,
  type ToolCall,
} from './helpers';

describe('ToolCallRenderer summaries', () => {
  it('renders extract summary while hiding backend execution metadata', () => {
    renderWithEnglishI18n(
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

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.queryByText('Web backend')).not.toBeInTheDocument();

    const extractCard = screen
      .getByText('Extract summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(extractCard).toBeInTheDocument();
    expect(within(extractCard).getByText('3')).toBeInTheDocument();
    expect(within(extractCard).getByText('1')).toBeInTheDocument();
    expect(within(extractCard).queryByText('5000')).not.toBeInTheDocument();
  });

  it('prefers structured_result from tool message metadata and renders summary text', () => {
    const toolResultById = new Map<string, Message>([
      ['tool_1', buildToolResultMessage({
        metadata: {
          structured_result: {
            _summary_text: 'Loaded page summary',
            backend: 'chatos_native_extract',
            extract_summary: {
              page_count: 1,
              truncated_page_count: 0,
              total_omitted_chars: 0,
            },
          },
        },
      })],
    ]);

    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          result: undefined,
          finalResult: undefined,
        } as Partial<ToolCall> & { finalResult?: string })}
        toolResultById={toolResultById}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getAllByText('Loaded page summary')).toHaveLength(1);
    expect(screen.queryByText('_summary_text')).not.toBeInTheDocument();
    expect(screen.queryByText('Web backend')).not.toBeInTheDocument();
    const extractCard = screen
      .getByText('Extract summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(extractCard).toBeInTheDocument();
    expect(within(extractCard).getByText('1')).toBeInTheDocument();
    expect(within(extractCard).getByText('0')).toBeInTheDocument();
  });
});

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { fireEvent, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  buildToolCall,
  renderWithEnglishI18n,
  ToolCallRenderer,
} from './helpers';

describe('ToolCallRenderer browser and research cards', () => {
  it('renders console summary card with message counters', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_console',
          result: {
            total_messages: 4,
            total_errors: 1,
            clear_applied: true,
            message_count_by_type: {
              log: 2,
              warn: 1,
              error: 1,
            },
            messages_brief: [
              { type: 'warn', text_preview: 'Deprecated API usage' },
            ],
            errors_brief: [
              { message_preview: 'Uncaught TypeError' },
            ],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const consoleCard = screen
      .getByText('Console summary')
      .closest('.tool-summary-card') as HTMLElement;
    expect(consoleCard).toBeInTheDocument();
    expect(within(consoleCard).getByText('4')).toBeInTheDocument();
    expect(within(consoleCard).getByText('1')).toBeInTheDocument();
    expect(within(consoleCard).queryByText('clear applied')).not.toBeInTheDocument();
    expect(within(consoleCard).queryByText('warn')).not.toBeInTheDocument();
    expect(screen.getByText('Console messages')).toBeInTheDocument();
    expect(screen.getByText('JavaScript errors')).toBeInTheDocument();
  });

  it('renders vision analysis without exposing transport metadata', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_vision',
          result: {
            analysis: 'The page shows a pricing table.',
            vision: {
              enabled: true,
              mode: 'user_model',
              prompt_source: 'contact_agent',
              provider: 'gpt',
              model: 'gpt-4o',
              transport: 'responses',
              fallback_used: true,
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.queryByText('Vision summary')).not.toBeInTheDocument();
    expect(screen.getByText('Vision analysis')).toBeInTheDocument();
    expect(screen.getByText('The page shows a pricing table.')).toBeInTheDocument();
    expect(screen.queryByText('responses')).not.toBeInTheDocument();
  });

  it('renders research summary card from nested research payload', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'web_research',
          result: {
            _summary_text: 'Research bundle completed.',
            research_findings: {
              answer_frame: 'Web research found enough signal to compare the target topic across sources.',
              web_findings: [
                'Search returned 5 result(s).',
                'Extraction reviewed 3 selected URL(s) and returned 3 page(s).',
              ],
              source_highlights: [
                {
                  kind: 'extract',
                  title: 'Competitor pricing breakdown',
                  url: 'https://example.com/competitor-pricing',
                  status: 'ok',
                  note: 'Includes concrete plan names and monthly pricing.',
                },
              ],
              recommended_next_steps: [
                'Open the strongest source and compare its claims against the current product positioning.',
              ],
            },
            research_summary: {
              search_backend: 'chatos_native_search',
              extract_backend: 'chatos_native_extract',
              search_result_count: 5,
              extracted_page_count: 3,
              selected_url_count: 3,
              total_omitted_chars: 1200,
              warning: 'extract fallback used',
            },
            search: {
              backend: 'chatos_native_search',
              result_count: 5,
            },
            extract: {
              backend: 'chatos_native_extract',
              extract_summary: {
                page_count: 3,
                total_omitted_chars: 1200,
              },
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const findingsCard = screen
      .getByText('Research findings')
      .closest('.tool-summary-card') as HTMLElement;
    expect(findingsCard).toBeInTheDocument();
    expect(within(findingsCard).getByText('Web research found enough signal to compare the target topic across sources.')).toBeInTheDocument();
    expect(within(findingsCard).getByText('Competitor pricing breakdown')).toBeInTheDocument();
    expect(within(findingsCard).getByText('Open the strongest source and compare its claims against the current product positioning.')).toBeInTheDocument();

    const researchCard = screen
      .getByText('Research overview')
      .closest('.tool-summary-card') as HTMLElement;
    expect(researchCard).toBeInTheDocument();
    expect(within(researchCard).getByText('5')).toBeInTheDocument();
    expect(within(researchCard).getAllByText('3').length).toBeGreaterThan(0);
    expect(within(researchCard).getByText('extract fallback used')).toBeInTheDocument();
    expect(within(researchCard).queryByText('firecrawl')).not.toBeInTheDocument();
    expect(within(researchCard).queryByText('direct_http')).not.toBeInTheDocument();
  });

  it('renders inspect summary card for browser_inspect results', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_inspect',
          result: {
            _summary_text: 'Observed the current browser page.',
            inspection_mode: 'read_only_observe',
            title: 'Pricing',
            url: 'https://example.com/pricing',
            element_count: 18,
            inspection_steps: {
              snapshot: 'ok',
              console: 'ok',
              vision: 'skipped',
            },
            total_messages: 4,
            total_errors: 1,
            page_state_available: true,
            inspection_warning: 'console: one warning was captured',
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const inspectCard = screen
      .getByText('Current page')
      .closest('.tool-summary-card') as HTMLElement;
    expect(inspectCard).toBeInTheDocument();
    expect(within(inspectCard).getByText('Pricing [https://example.com/pricing]')).toBeInTheDocument();
    expect(within(inspectCard).getByText('4')).toBeInTheDocument();
    expect(within(inspectCard).getByText('1')).toBeInTheDocument();
    expect(within(inspectCard).getByText('console: one warning was captured')).toBeInTheDocument();
    expect(within(inspectCard).queryByText('read_only_observe')).not.toBeInTheDocument();
  });

  it('renders browser_inspect blank page state without surfacing about blank as an active page', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_inspect',
          result: {
            _summary_text: 'No active browser page was available.',
            success: false,
            inspection_mode: 'read_only_observe',
            url: 'about:blank',
            inspection_steps: {
              snapshot: 'ok',
              console: 'ok',
              vision: 'skipped',
            },
            total_messages: 0,
            total_errors: 0,
            page_state_available: false,
            inspection_warning: 'page: no active browser page was available; open a page before running browser_inspect',
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const inspectCard = screen
      .getByText('Current page')
      .closest('.tool-summary-card') as HTMLElement;
    expect(inspectCard).toBeInTheDocument();
    expect(within(inspectCard).getByText('No open page')).toBeInTheDocument();
    expect(within(inspectCard).getByText('page: no active browser page was available; open a page before running browser_inspect')).toBeInTheDocument();
    expect(within(inspectCard).queryByText('about:blank')).not.toBeInTheDocument();
    expect(screen.getByText('Inspection warning')).toBeInTheDocument();
    expect(screen.queryByText('Vision analysis')).not.toBeInTheDocument();
  });

  it('renders inspect and research summaries for browser_research results', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_research',
          result: {
            _summary_text: 'Researched the current browser page.',
            page: {
              inspection_mode: 'read_only_observe',
              title: 'Docs',
              url: 'https://example.com/docs',
              element_count: 12,
              snapshot: 'VERY_LONG_SNAPSHOT_BLOCK',
              inspection_steps: {
                snapshot: 'ok',
                console: 'ok',
                vision: 'ok',
              },
              total_messages: 2,
              total_errors: 0,
              page_state_available: true,
              console_messages: [
                { text: 'RAW_CONSOLE_ENTRY' },
              ],
              js_errors: [
                { message: 'RAW_JS_ERROR_ENTRY' },
              ],
            },
            selected_urls: [
              'https://example.com/release-notes',
              'https://example.com/extra-source',
            ],
            research_findings: {
              answer_frame: 'Combined page and web research completed for "What changed?".',
              page_findings: [
                'Current page: Docs [https://example.com/docs].',
                'Inspection steps finished with snapshot=ok, console=ok, vision=ok.',
              ],
              web_findings: [
                'External search for "docs changelog" returned 4 result(s).',
              ],
              source_highlights: [
                {
                  kind: 'extract',
                  title: 'Release notes',
                  url: 'https://example.com/release-notes',
                  status: 'ok',
                  note: 'Highlights API and UI changes from the last release.',
                },
              ],
              recommended_next_steps: [
                'Open the release notes source and compare it against the current page.',
              ],
            },
            research_summary: {
              search_backend: 'chatos_native_search',
              extract_backend: 'chatos_native_extract',
              search_result_count: 4,
              extracted_page_count: 2,
              selected_url_count: 2,
              total_omitted_chars: 900,
              warning: 'web_extract: fallback used',
            },
            search: {
              backend: 'chatos_native_search',
              result_count: 4,
              provider_attempts: [],
              data: {
                web: [
                  {
                    title: 'RAW_SEARCH_HIT',
                  },
                ],
              },
              results_brief: [
                {
                  title: 'Release notes brief',
                  url: 'https://example.com/release-notes',
                  description_preview: 'Visible brief summary',
                },
              ],
            },
            extract: {
              backend: 'chatos_native_extract',
              provider_attempts: [],
              extract_summary: {
                page_count: 2,
                total_omitted_chars: 900,
              },
              results_brief: [
                {
                  title: 'Release notes extract',
                  url: 'https://example.com/release-notes',
                  status: 'ok',
                  content_preview: 'Visible extract summary',
                },
              ],
              results: [
                {
                  content: 'VERY_LONG_RAW_SOURCE_TEXT',
                },
              ],
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const findingsCard = screen
      .getByText('Research findings')
      .closest('.tool-summary-card') as HTMLElement;
    expect(findingsCard).toBeInTheDocument();
    expect(within(findingsCard).getByText('Current page: Docs [https://example.com/docs].')).toBeInTheDocument();
    expect(within(findingsCard).getByText('Release notes')).toBeInTheDocument();

    const inspectCard = screen
      .getByText('Current page')
      .closest('.tool-summary-card') as HTMLElement;
    expect(inspectCard).toBeInTheDocument();
    expect(within(inspectCard).getByText('Docs [https://example.com/docs]')).toBeInTheDocument();

    const researchCard = screen
      .getByText('Research overview')
      .closest('.tool-summary-card') as HTMLElement;
    expect(researchCard).toBeInTheDocument();
    expect(within(researchCard).getByText('4')).toBeInTheDocument();
    expect(within(researchCard).getAllByText('2').length).toBeGreaterThan(0);
    expect(within(researchCard).queryByText('firecrawl')).not.toBeInTheDocument();
    expect(within(researchCard).queryByText('direct_http')).not.toBeInTheDocument();
    expect(screen.getByText('Selected URLs')).toBeInTheDocument();
    expect(screen.getByText('https://example.com/extra-source')).toBeInTheDocument();
    expect(screen.getByText('Search hits')).toBeInTheDocument();
    expect(screen.getByText('Release notes brief')).toBeInTheDocument();
    expect(screen.getByText('Extracted sources')).toBeInTheDocument();
    expect(screen.getByText('Release notes extract')).toBeInTheDocument();
    expect(screen.queryByText('VERY_LONG_SNAPSHOT_BLOCK')).not.toBeInTheDocument();
    expect(screen.queryByText('RAW_CONSOLE_ENTRY')).not.toBeInTheDocument();
    expect(screen.queryByText('RAW_JS_ERROR_ENTRY')).not.toBeInTheDocument();
    expect(screen.queryByText('RAW_SEARCH_HIT')).not.toBeInTheDocument();
    expect(screen.queryByText('VERY_LONG_RAW_SOURCE_TEXT')).not.toBeInTheDocument();
    expect(screen.queryByText('provider_attempts')).not.toBeInTheDocument();
    expect(screen.queryByText('字段')).not.toBeInTheDocument();
  });
});

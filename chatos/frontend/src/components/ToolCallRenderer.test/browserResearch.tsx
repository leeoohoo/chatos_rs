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
  it('renders sanitized browser tabs with stable IDs', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_tabs',
          result: {
            tab_count: 2,
            active_tab_id: 't2',
            tabs: [
              { tab_id: 't1', active: false, title: 'App', url: 'https://example.com/app?token=%5BREDACTED%5D' },
              { tab_id: 't2', active: true, title: 'Docs', url: 'https://example.com/docs' },
            ],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Browser tabs')).toBeInTheDocument();
    expect(screen.getByText('t1 · https://example.com/app?token=%5BREDACTED%5D')).toBeInTheDocument();
    expect(screen.getByText('● Docs')).toBeInTheDocument();
    expect(screen.queryByText(/token=secret/)).not.toBeInTheDocument();
  });

  it('renders the browser session identity and status', () => {
    window.chatosLocalRuntime = {
      apiRequest: async () => ({ ok: true, status: 200, body: '{}' }),
    };
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_navigate',
          result: {
            success: true,
            url: 'https://example.com',
            browser_session: {
              id: 'h_session_123',
              mode: 'managed',
              status: 'active',
              event: 'started',
              workspace_id: 'workspace-1',
              device_id: 'device-1',
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    const sessionCard = screen
      .getByText('Browser session')
      .closest('.tool-detail-card') as HTMLElement;
    expect(sessionCard).toBeInTheDocument();
    expect(within(sessionCard).getByText('h_session_123')).toBeInTheDocument();
    expect(within(sessionCard).getByText('managed')).toBeInTheDocument();
    expect(within(sessionCard).getByText('active')).toBeInTheDocument();
    expect(within(sessionCard).getByText('started')).toBeInTheDocument();
    expect(within(sessionCard).getByText('workspace-1')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open browser in ChatOS' })).toBeInTheDocument();
  });

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

  it('renders sanitized browser network timing without headers or query secrets', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_network',
          result: {
            resource_count: 2,
            returned_count: 1,
            omitted_count: 1,
            truncated: true,
            query_and_fragment_redacted: true,
            request_headers_included: false,
            response_headers_included: false,
            resources: [
              {
                url: 'https://example.com/api/data',
                initiator_type: 'fetch',
                duration_ms: 18.5,
                transfer_size: 512,
              },
            ],
            navigation: {
              url: 'https://example.com/app',
              type: 'navigate',
              duration_ms: 120,
              transfer_size: 2048,
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));

    expect(screen.getByText('Network summary')).toBeInTheDocument();
    expect(screen.getByText('Navigation timing')).toBeInTheDocument();
    expect(screen.getByText('Network resources')).toBeInTheDocument();
    expect(screen.getByText('https://example.com/api/data')).toBeInTheDocument();
    expect(screen.queryByText(/token=secret/)).not.toBeInTheDocument();
    expect(screen.queryByText(/authorization/i)).not.toBeInTheDocument();
  });

  it('renders sanitized CDP request lists and explicit request detail bodies', () => {
    const { unmount } = renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_network',
          result: {
            request_count: 1,
            returned_count: 1,
            requests: [{
              request_id: '7253.2',
              url: 'https://example.com/api?token=%5BREDACTED%5D',
              method: 'POST',
              status: 200,
              resource_type: 'Fetch',
            }],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Network requests')).toBeInTheDocument();
    expect(screen.getByText('https://example.com/api?token=%5BREDACTED%5D')).toBeInTheDocument();
    expect(screen.getByText(/POST · 200 · Fetch · 7253\.2/)).toBeInTheDocument();
    unmount();

    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_network_request',
          result: {
            request: {
              request_id: '7253.2',
              url: 'https://example.com/api?token=%5BREDACTED%5D',
              method: 'POST',
              status: 200,
              resource_type: 'Fetch',
              request_headers: { 'content-type': 'application/json', authorization: '[REDACTED]' },
              response_headers: { 'content-type': 'application/json', 'set-cookie': '[REDACTED]' },
              request_body: { text: '{\n  "password": "[REDACTED]",\n  "safe": "visible"\n}' },
              response_body: { text: '{\n  "result": "ok"\n}' },
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Network request detail')).toBeInTheDocument();
    expect(screen.getByText('Request headers')).toBeInTheDocument();
    expect(screen.getByText('Response headers')).toBeInTheDocument();
    expect(screen.getByText(/visible/)).toBeInTheDocument();
    expect(screen.queryByText(/body-secret/)).not.toBeInTheDocument();
  });

  it('renders bounded browser upload and download results', () => {
    const { unmount } = renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_upload',
          result: {
            file_count: 2,
            total_bytes: 1536,
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Uploaded files')).toBeInTheDocument();
    expect(screen.getByText('1536')).toBeInTheDocument();
    unmount();

    const browserDownload = renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_download',
          result: {
            path: 'browser-download.bin',
            bytes: 2048,
            overwrote_existing: false,
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Downloaded file')).toBeInTheDocument();
    expect(screen.getByText('browser-download.bin')).toBeInTheDocument();
    expect(screen.getByText('2048')).toBeInTheDocument();
    browserDownload.unmount();

    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'chrome_tab_download',
          result: {
            path: 'downloads/report.pdf',
            size_bytes: 4096,
            sha256: 'a'.repeat(64),
            chunk_count: 1,
            mime_type: 'application/pdf',
            source_kind: 'https',
            source_url: 'https://example.com/report.pdf?token=%5BREDACTED%5D',
            overwritten: false,
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Downloaded file')).toBeInTheDocument();
    expect(screen.getByText('downloads/report.pdf')).toBeInTheDocument();
    expect(screen.getByText('application/pdf')).toBeInTheDocument();
  });

  it('renders sanitized HAR export metadata without raw traffic', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_har_stop',
          result: {
            path: 'diagnostics/network.har',
            bytes: 4096,
            raw_capture_deleted: true,
            request_bodies_included: false,
            response_bodies_included: false,
            overwrote_existing: false,
            sanitization: {
              exported_entries: 7,
            },
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('Sanitized HAR export')).toBeInTheDocument();
    expect(screen.getByText('diagnostics/network.har')).toBeInTheDocument();
    expect(screen.getByText('4096')).toBeInTheDocument();
    expect(screen.getByText('7')).toBeInTheDocument();
    expect(screen.queryByText(/authorization/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/cookie-secret/i)).not.toBeInTheDocument();
  });

  it('renders bounded WebSocket metadata and only explicitly returned redacted text', () => {
    renderWithEnglishI18n(
      <ToolCallRenderer
        toolCall={buildToolCall({
          name: 'browser_websocket_frames',
          result: {
            status: 'active',
            active: true,
            socket_count: 1,
            open_socket_count: 1,
            total_frame_count: 2,
            returned_count: 2,
            text_payloads_included: true,
            binary_payloads_included: false,
            frames: [
              {
                sequence: 1,
                request_id: '7253.7',
                url: 'wss://example.com/socket?token=%5BREDACTED%5D',
                direction: 'sent',
                frame_type: 'text',
                payload_bytes: 48,
                text_payload: '{"token":"[REDACTED]","event":"ready"}',
              },
              {
                sequence: 2,
                request_id: '7253.7',
                direction: 'received',
                frame_type: 'binary',
                payload_bytes: 512,
                text_payload: null,
              },
            ],
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'View details' }));
    expect(screen.getByText('WebSocket observation')).toBeInTheDocument();
    expect(screen.getByText('WebSocket frames')).toBeInTheDocument();
    expect(screen.getByText('wss://example.com/socket?token=%5BREDACTED%5D')).toBeInTheDocument();
    expect(screen.getByText(/"event":"ready"/)).toBeInTheDocument();
    expect(screen.queryByText(/socket-secret/)).not.toBeInTheDocument();
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

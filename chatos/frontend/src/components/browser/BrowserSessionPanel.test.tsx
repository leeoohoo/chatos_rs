// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { sendBrowserSessionCommand } from '../../lib/api/localRuntime/browserSession';
import { openBrowserSessionPanel } from '../../lib/browserSessionUi';
import BrowserSessionPanel from './BrowserSessionPanel';

vi.mock('../../lib/api/localRuntime/browserSession', () => ({
  sendBrowserSessionCommand: vi.fn(),
}));

vi.mock('./BrowserPdfPreview', () => ({
  default: () => null,
}));

const sendCommand = vi.mocked(sendBrowserSessionCommand);

describe('BrowserSessionPanel diagnostics', () => {
  beforeEach(() => {
    window.localStorage.setItem('chat_ui_locale', 'en-US');
    sendCommand.mockImplementation(async (_sessionId, payload) => {
      if (payload.action === 'stream_frame') {
        return {
          status: 'active',
          action: 'stream_frame',
          frame_data_url: 'data:image/jpeg;base64,/9j/2Q==',
          frame: {
            media_type: 'image/jpeg',
            sequence: 1,
            width: 1280,
            height: 720,
            source: 'screencast',
          },
        };
      }
      if (payload.action === 'tabs') {
        return {
          status: 'active',
          action: 'tabs',
          page: {
            title: 'Example page',
            url: 'https://example.com/app',
            snapshot: 'button "Continue" [ref=e12]',
            tabs: [
              { tab_id: 't1', active: true, title: 'Example page', url: 'https://example.com/app' },
              { tab_id: 't2', active: false, title: 'Documentation', url: 'https://example.com/docs' },
            ],
          },
        };
      }
      if (payload.action === 'tab_switch') {
        return {
          status: 'active',
          action: 'tab_switch',
          page: {
            title: 'Documentation',
            url: 'https://example.com/docs',
            snapshot: 'heading "Documentation"',
            tabs: [
              { tab_id: 't1', active: false, title: 'Example page', url: 'https://example.com/app' },
              { tab_id: 't2', active: true, title: 'Documentation', url: 'https://example.com/docs' },
            ],
          },
        };
      }
      if (payload.action === 'console') {
        return {
          status: 'active',
          action: 'console',
          page: {
            console_messages: [{ type: 'warn', text: 'Deprecated API usage' }],
            js_errors: [{ message: 'Uncaught TypeError' }],
          },
        };
      }
      if (payload.action === 'network') {
        return {
          status: 'active',
          action: 'network',
          page: {
            resource_count: 1,
            returned_count: 1,
            resources: [{
              url: 'https://example.com/api/data',
              initiator_type: 'fetch',
              duration_ms: 18.5,
              transfer_size: 512,
            }],
          },
        };
      }
      if (payload.action === 'har_start') {
        return {
          success: true,
          status: 'active',
          action: 'har_start',
          page: { status: 'recording' },
        };
      }
      if (payload.action === 'har_stop') {
        return {
          success: true,
          status: 'active',
          action: 'har_stop',
          page: {
            status: 'stopped',
            path: 'browser-network.har',
            bytes: 2048,
            sanitization: { exported_entries: 3 },
          },
        };
      }
      if (payload.action === 'websocket_start') {
        return {
          success: true,
          status: 'active',
          action: 'websocket_start',
          page: { status: 'active', socket_count: 0, total_frame_count: 0 },
        };
      }
      if (payload.action === 'websocket_frames') {
        return {
          success: true,
          status: 'active',
          action: 'websocket_frames',
          page: {
            status: 'active',
            active: true,
            socket_count: 1,
            total_frame_count: 2,
            returned_count: 2,
            text_payloads_included: payload.include_text_payloads === true,
            binary_payloads_included: false,
            frames: [
              {
                sequence: 1,
                request_id: '7253.7',
                url: 'wss://example.com/socket?token=%5BREDACTED%5D',
                direction: 'sent',
                frame_type: 'text',
                payload_bytes: 48,
                text_payload: payload.include_text_payloads
                  ? '{"token":"[REDACTED]","event":"ready"}'
                  : null,
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
        };
      }
      if (payload.action === 'websocket_stop') {
        return {
          success: true,
          status: 'active',
          action: 'websocket_stop',
          page: { status: 'stopped', socket_count: 1, total_frame_count: 2 },
        };
      }
      return {
        status: 'active',
        action: payload.action,
        page: {
          title: 'Example page',
          url: 'https://example.com/app',
          snapshot: 'button "Continue" [ref=e12]',
        },
      };
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    window.localStorage.clear();
  });

  it('opens console and network tabs through fixed read-only actions', async () => {
    render(
      <I18nProvider>
        <BrowserSessionPanel />
      </I18nProvider>,
    );
    openBrowserSessionPanel({
      id: 'h_session_123',
      workspaceId: 'workspace-1',
      mode: 'managed',
      status: 'active',
    });

    expect(await screen.findByRole('button', { name: 'Switch to tab Example page' })).toBeInTheDocument();
    expect(await screen.findByText('Live screencast')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Switch to tab Documentation' }));
    expect(await screen.findByText('heading "Documentation"')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Console' }));
    expect(await screen.findByText('Deprecated API usage')).toBeInTheDocument();
    expect(screen.getByText('Uncaught TypeError')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Network' }));
    expect(await screen.findByText('https://example.com/api/data')).toBeInTheDocument();
    expect(screen.getByText(/fetch · 18\.5 ms · 512 B/)).toBeInTheDocument();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Start HAR' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Start HAR' }));
    expect(await screen.findByText('Recording')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop and export' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Stop and export' }));
    expect(await screen.findByText(/Exported 3 sanitized entries to browser-network\.har/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'WebSocket' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start observation' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Start observation' }));
    expect(await screen.findByText('Observing')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Refresh frames' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Refresh frames' }));
    expect(await screen.findByText('wss://example.com/socket?token=%5BREDACTED%5D')).toBeInTheDocument();
    expect(screen.queryByText(/"event":"ready"/)).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Load redacted text payloads' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Load redacted text payloads' }));
    expect(await screen.findByText(/"event":"ready"/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop observation' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Stop observation' }));
    expect(await screen.findByText('Stopped')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'New tab' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'New tab' }));

    await waitFor(() => {
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'stream_frame',
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'tab_switch',
          tab_id: 't2',
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'stream_stop',
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'tab_new',
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'console',
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'network',
          limit: 100,
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'har_stop',
          path: 'browser-network.har',
          include_request_bodies: false,
          include_response_bodies: false,
          max_entries: 500,
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'websocket_frames',
          include_text_payloads: true,
          max_payload_chars: 1024,
        }),
      );
      expect(sendCommand).toHaveBeenCalledWith(
        'h_session_123',
        expect.objectContaining({
          workspace_id: 'workspace-1',
          action: 'websocket_stop',
        }),
      );
    });
    expect(sendCommand.mock.calls.some(([, payload]) => (
      payload.action === 'stream_frame' && payload.after_frame_sequence === 1
    ))).toBe(true);
    expect(sendCommand.mock.calls.some(([, payload]) => 'expression' in payload)).toBe(false);
  });
});

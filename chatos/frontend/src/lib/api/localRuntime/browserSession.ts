// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { requestLocalRuntime } from './bridge';

export interface BrowserSessionCommandPayload {
  workspace_id: string;
  action: 'stream_frame' | 'stream_stop' | 'tabs' | 'tab_new' | 'tab_switch' | 'tab_close' | 'snapshot' | 'refresh' | 'navigate' | 'back' | 'scroll' | 'press' | 'click' | 'type' | 'upload' | 'download' | 'console' | 'network' | 'network_request' | 'har_start' | 'har_stop' | 'websocket_start' | 'websocket_frames' | 'websocket_stop' | 'close';
  after_frame_sequence?: number;
  url?: string;
  tab_id?: string;
  direction?: 'up' | 'down' | 'sent' | 'received';
  key?: string;
  ref?: string;
  text?: string;
  path?: string;
  paths?: string[];
  clear?: boolean;
  limit?: number;
  filter?: string;
  resource_types?: string[];
  method?: string;
  status?: string;
  request_id?: string;
  include_request_body?: boolean;
  include_response_body?: boolean;
  max_body_chars?: number;
  include_text_payloads?: boolean;
  max_payload_chars?: number;
  include_request_bodies?: boolean;
  include_response_bodies?: boolean;
  max_entries?: number;
}

export interface BrowserSessionCommandResponse {
  success?: boolean;
  session_id?: string;
  workspace_id?: string;
  status?: string;
  action?: string;
  page?: Record<string, unknown>;
  frame_data_url?: string | null;
  frame?: Record<string, unknown>;
  frame_warning?: string | null;
  unchanged?: boolean;
  stream_stopped?: boolean;
  screenshot_data_url?: string | null;
  screenshot_error?: string | null;
  captured_at?: string;
  result?: unknown;
}

export const sendBrowserSessionCommand = (
  sessionId: string,
  payload: BrowserSessionCommandPayload,
): Promise<BrowserSessionCommandResponse> => requestLocalRuntime<BrowserSessionCommandResponse>(
  `/api/local/runtime/browser/sessions/${encodeURIComponent(sessionId)}/command`,
  {
    method: 'POST',
    body: JSON.stringify(payload),
  },
);

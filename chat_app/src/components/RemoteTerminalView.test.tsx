// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import RemoteTerminalView from './RemoteTerminalView';
import { I18nProvider } from '../i18n/I18nProvider';
import type { RemoteConnection } from '../types';
import { apiClient } from '../lib/api/client';

const mockConnection: RemoteConnection = {
  id: 'conn_1',
  name: 'Aliyun Bastion',
  host: 'mwpxljyjsn-public.bastionhost.aliyuncs.com',
  port: 60022,
  username: 'v-v8g22u',
  authType: 'password',
  password: null,
  privateKeyPath: null,
  certificatePath: null,
  defaultRemotePath: null,
  hostKeyPolicy: 'strict',
  jumpEnabled: false,
  jumpConnectionId: null,
  jumpHost: null,
  jumpPort: null,
  jumpUsername: null,
  jumpPrivateKeyPath: null,
  jumpCertificatePath: null,
  jumpPassword: null,
  userId: 'user_1',
  createdAt: new Date('2026-05-28T07:57:00Z'),
  updatedAt: new Date('2026-05-28T07:57:00Z'),
  lastActiveAt: new Date('2026-05-28T07:57:00Z'),
};

const openRemoteSftp = vi.fn(async () => undefined);
const disconnectRemoteTerminal = vi.fn(async () => ({ success: true }));

vi.mock('@xterm/xterm', () => {
  class MockTerminal {
    cols = 80;
    rows = 24;
    options = {};
    open = vi.fn();
    loadAddon = vi.fn();
    focus = vi.fn();
    reset = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
  }

  return { Terminal: MockTerminal };
});

vi.mock('@xterm/addon-fit', () => {
  class MockFitAddon {
    fit = vi.fn();
  }

  return { FitAddon: MockFitAddon };
});

vi.mock('../hooks/useTheme', () => ({
  useTheme: () => ({ actualTheme: 'light' }),
}));

vi.mock('../lib/api/client', async () => {
  const actual = await vi.importActual<typeof import('../lib/api/client')>('../lib/api/client');
  const client = actual.apiClient;
  return {
    ...actual,
    apiClient: {
      getBaseUrl: () => client.getBaseUrl(),
      setAccessToken: (token?: string | null) => client.setAccessToken(token),
      getAccessToken: () => client.getAccessToken(),
      getUserSettings: vi.fn(async () => ({ settings: { UI_LOCALE: 'zh-CN' } })),
    },
  };
});

vi.mock('../lib/store/ChatStoreContext', () => ({
  useChatStoreSelector: (selector: (state: {
    currentRemoteConnection: RemoteConnection | null;
    openRemoteSftp: typeof openRemoteSftp;
  }) => unknown) => selector({
    currentRemoteConnection: mockConnection,
    openRemoteSftp,
  }),
  useChatApiClientFromContext: () => ({
    getBaseUrl: () => '/api',
    disconnectRemoteTerminal,
  }),
}));

vi.mock('../lib/auth/authStore', () => ({
  useAuthStore: (selector?: (state: {
    accessToken: string | null;
    user: { id: string; username: string } | null;
    initialized: boolean;
  }) => unknown) => {
    const state = {
      accessToken: 'token_1',
      user: { id: 'user_1', username: 'user_1' },
      initialized: true,
    };
    return typeof selector === 'function' ? selector(state) : state;
  },
}));

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  url: string;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  send = vi.fn();
  close = vi.fn(() => {
    this.readyState = MockWebSocket.CLOSED;
  });

  constructor(url: string) {
    this.url = url;
    sockets.push(this);
  }

  emitOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.(new Event('open'));
  }

  emitMessage(data: unknown) {
    this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(data) }));
  }

  emitError() {
    this.onerror?.(new Event('error'));
  }

  emitClose() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.(new CloseEvent('close'));
  }
}

const sockets: MockWebSocket[] = [];

describe('RemoteTerminalView', () => {
  beforeEach(() => {
    sockets.length = 0;
    openRemoteSftp.mockClear();
    disconnectRemoteTerminal.mockClear();
    apiClient.setAccessToken('token_1');
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      disconnect() {}
    });
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
  });

  afterEach(() => {
    cleanup();
    apiClient.setAccessToken(null);
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('keeps the concrete auth failure message after websocket error and close', async () => {
    render(
      <I18nProvider>
        <RemoteTerminalView />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getByText('Aliyun Bastion')).toBeInTheDocument();
    });

    const reconnectButton = screen.getByRole('button', { name: '重连' });
    reconnectButton.click();

    await waitFor(() => {
      expect(sockets.length).toBe(1);
    });

    const ws = sockets[0];
    ws.emitOpen();
    ws.emitMessage({
      type: 'error',
      code: 'auth_failed',
      error: '密码认证失败: Authentication failed',
    });
    ws.emitError();
    ws.emitClose();

    await waitFor(() => {
      expect(screen.getByText(/SSH 认证失败: 密码认证失败: Authentication failed；建议：/)).toBeInTheDocument();
    });

    expect(screen.queryByText('远端终端连接异常，请重试')).not.toBeInTheDocument();
  });
});

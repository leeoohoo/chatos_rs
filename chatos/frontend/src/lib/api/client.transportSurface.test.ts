// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import { downloadFsEntry } from './client/fs';
import { sendChatCommand } from './client/stream';

const requestedHeaders = (fetchMock: ReturnType<typeof vi.fn>): Headers => {
  const options = fetchMock.mock.calls[0]?.[1] as RequestInit | undefined;
  return new Headers(options?.headers);
};

const enableDesktopBridge = (): void => {
  vi.stubGlobal('window', {
    chatosLocalRuntime: { apiRequest: vi.fn() },
  });
};

describe('direct API transport surface header', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('marks cloud chat command requests from the desktop client', async () => {
    enableDesktopBridge();
    const fetchMock = vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ accepted: true }),
      { status: 200 },
    ));
    vi.stubGlobal('fetch', fetchMock);

    await sendChatCommand(
      {
        baseUrl: 'https://api.example.com/api',
        accessToken: 'token-1',
        applyRefreshedAccessToken: vi.fn(),
      },
      'conversation-1',
      'hello',
      { id: 'model-1', model_name: 'gpt-test', provider: 'openai' },
    );

    expect(requestedHeaders(fetchMock).has('X-Chatos-Client-Surface')).toBe(false);
    expect(requestedHeaders(fetchMock).get('X-Requested-With'))
      .toBe('local-connector-desktop');
  });

  it('marks cloud file downloads from the desktop client', async () => {
    enableDesktopBridge();
    const fetchMock = vi.fn().mockResolvedValue(new Response('file-content', {
      status: 200,
      headers: { 'Content-Type': 'text/plain' },
    }));
    vi.stubGlobal('fetch', fetchMock);

    await downloadFsEntry(
      {
        baseUrl: 'https://api.example.com/api',
        accessToken: 'token-1',
        applyRefreshedAccessToken: vi.fn(),
      },
      '/workspace/file.txt',
    );

    expect(requestedHeaders(fetchMock).has('X-Chatos-Client-Surface')).toBe(false);
    expect(requestedHeaders(fetchMock).get('X-Requested-With'))
      .toBe('local-connector-desktop');
  });
});

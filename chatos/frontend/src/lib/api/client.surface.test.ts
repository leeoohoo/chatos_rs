// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

import ApiClient from './client';

const successfulLoginResponse = (): Response => new Response(
  JSON.stringify({ access_token: 'token-1', user: { id: 'user-1' } }),
  { status: 200, headers: { 'Content-Type': 'application/json' } },
);

const requestedHeaders = (fetchMock: ReturnType<typeof vi.fn>): Headers => {
  const options = fetchMock.mock.calls[0]?.[1] as RequestInit | undefined;
  return new Headers(options?.headers);
};

describe('ApiClient surface header', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('does not identify a normal browser as the desktop client', async () => {
    vi.stubGlobal('window', {});
    const fetchMock = vi.fn().mockResolvedValue(successfulLoginResponse());
    vi.stubGlobal('fetch', fetchMock);

    await new ApiClient('https://api.example.com/api').login({
      username: 'tester@example.com',
      password: 'secret',
    });

    expect(requestedHeaders(fetchMock).has('X-Chatos-Client-Surface')).toBe(false);
    expect(requestedHeaders(fetchMock).has('X-Requested-With')).toBe(false);
  });

  it('identifies requests made by the Local Connector desktop client', async () => {
    vi.stubGlobal('window', {
      chatosLocalRuntime: { apiRequest: vi.fn() },
    });
    const fetchMock = vi.fn().mockResolvedValue(successfulLoginResponse());
    vi.stubGlobal('fetch', fetchMock);

    await new ApiClient('https://api.example.com/api').login({
      username: 'tester@example.com',
      password: 'secret',
    });

    expect(requestedHeaders(fetchMock).has('X-Chatos-Client-Surface')).toBe(false);
    expect(requestedHeaders(fetchMock).get('X-Requested-With'))
      .toBe('local-connector-desktop');
  });
});

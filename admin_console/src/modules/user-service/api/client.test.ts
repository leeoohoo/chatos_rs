// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => vi.unstubAllGlobals());

describe('user list API', () => {
  it('encodes paged search and resolves exact IDs with POST', async () => {
    vi.resetModules();
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ items: [], total: 0 })))
      .mockResolvedValueOnce(new Response(JSON.stringify([])));
    vi.stubGlobal('fetch', fetchMock);
    const { api } = await import('./client');

    await api.listUserOptions(20, 40, 'a&b');
    await api.resolveUserOptions(['user-1', 'user-2']);

    expect(fetchMock.mock.calls[0][0]).toBe(
      '/api/admin/user-service/users/options?limit=20&offset=40&query=a%26b',
    );
    expect(fetchMock.mock.calls[1][0]).toBe('/api/admin/user-service/users/options/resolve');
    expect(fetchMock.mock.calls[1][1]).toMatchObject({
      method: 'POST',
      body: JSON.stringify({ ids: ['user-1', 'user-2'] }),
    });
  });
});

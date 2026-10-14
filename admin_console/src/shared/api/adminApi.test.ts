import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => vi.unstubAllGlobals());

describe('administrator session API', () => {
  it('uses same-origin cookie POSTs for login, recovery and logout', async () => {
    vi.resetModules();
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: 'memory-token', user: { role: 'admin' } })))
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: 'memory-token', user: { role: 'admin' } })))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal('fetch', fetchMock);
    const { adminApi } = await import('./adminApi');
    await adminApi.login({ username: 'admin', password: 'test-only-password' });
    await adminApi.restoreSession();
    await adminApi.logout();
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      '/api/admin/user-service/auth/admin/login',
      '/api/admin/user-service/auth/admin/session',
      '/api/admin/user-service/auth/admin/logout',
    ]);
    for (const [, init] of fetchMock.mock.calls) {
      expect(init.method).toBe('POST');
      expect(init.credentials).toBe('same-origin');
      expect(init.headers.get('X-Chatos-Admin-Session')).toBe('1');
      expect(init.headers.has('Authorization')).toBe(false);
    }
  });

  it('preserves HTTP status so recovery can distinguish expiry from server failure', async () => {
    vi.resetModules();
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'temporarily unavailable' }), { status: 503 })));
    const { adminApi } = await import('./adminApi');
    await expect(adminApi.restoreSession()).rejects.toMatchObject({ status: 503 });
  });
});

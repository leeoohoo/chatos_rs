import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createAuthStore, useAuthStore } from './authStore';

const buildFakeAuthClient = () => ({
  setAccessToken: vi.fn(),
  getAccessToken: vi.fn(() => null),
  onAccessTokenRefresh: vi.fn(() => () => undefined),
  getMe: vi.fn(),
  login: vi.fn(),
  register: vi.fn(),
});

const resetAuthStoreState = () => {
  useAuthStore.setState({
    accessToken: null,
    user: null,
    initialized: false,
    loading: false,
    error: null,
  });
};

describe('authStore client injection', () => {
  let consoleWarnSpy: { mockRestore: () => void };
  let consoleErrorSpy: { mockRestore: () => void };

  beforeEach(() => {
    consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    resetAuthStoreState();
  });

  afterEach(() => {
    consoleWarnSpy.mockRestore();
    consoleErrorSpy.mockRestore();
    resetAuthStoreState();
  });

  it('uses the injected api client for login', async () => {
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-login' });
    client.login.mockResolvedValue({
      access_token: 'token_custom',
      user: {
        id: 'user_custom',
        username: 'tester',
      },
    });

    await authStore.getState().login('tester', 'secret');

    expect(client.login).toHaveBeenCalledWith({ username: 'tester', password: 'secret' });
    expect(client.setAccessToken).toHaveBeenCalledWith('token_custom');
    expect(authStore.getState().accessToken).toBe('token_custom');
    expect(authStore.getState().user?.id).toBe('user_custom');
  });

  it('uses the injected api client for bootstrap token validation', async () => {
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-bootstrap' });
    client.getMe.mockResolvedValue({
      user: {
        id: 'persisted_user',
        username: 'persisted',
      },
    });
    authStore.setState({
      accessToken: 'persisted_token',
      initialized: false,
    });

    await authStore.getState().bootstrap();

    expect(client.setAccessToken).toHaveBeenCalledWith('persisted_token');
    expect(client.getMe).toHaveBeenCalledTimes(1);
    expect(authStore.getState().user?.id).toBe('persisted_user');
    expect(authStore.getState().initialized).toBe(true);
  });
});

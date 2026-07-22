// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createAuthStore, useAuthStore } from './authStore';
import { ApiRequestError } from '../api/client/shared';

const buildFakeAuthClient = () => ({
  setAccessToken: vi.fn(),
  getAccessToken: vi.fn(() => null),
  onAccessTokenRefresh: vi.fn(() => () => undefined),
  getMe: vi.fn(),
  login: vi.fn(),
  register: vi.fn(),
  issueLocalConnectorTicket: vi.fn(),
});

const desktopWindow = (authenticateDesktopTicket: (ticket: string) => Promise<unknown>) => {
  const sessionValues = new Map<string, string>();
  const localValues = new Map<string, string>();
  const storage = (values: Map<string, string>) => ({
    getItem: (key: string) => values.get(key) || null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  });
  return {
    location: { search: '?desktop=local-connector' },
    sessionStorage: storage(sessionValues),
    localStorage: storage(localValues),
    chatosLocalRuntime: { authenticateDesktopTicket },
  };
};

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
    vi.unstubAllGlobals();
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

  it('waits for the desktop Core ticket handshake before committing login', async () => {
    let completeHandshake: (() => void) | undefined;
    const handshake = new Promise<void>((resolve) => {
      completeHandshake = resolve;
    });
    const authenticateDesktopTicket = vi.fn(() => handshake);
    vi.stubGlobal('window', desktopWindow(authenticateDesktopTicket));
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-desktop-login' });
    client.login.mockResolvedValue({
      access_token: 'desktop-token',
      user: { id: 'desktop-user', username: 'desktop' },
    });
    client.issueLocalConnectorTicket.mockResolvedValue({ ticket: 'desktop-ticket' });

    const loginPromise = authStore.getState().login('desktop', 'secret');
    await vi.waitFor(() => expect(authenticateDesktopTicket).toHaveBeenCalledWith('desktop-ticket'));

    expect(authStore.getState().user).toBeNull();
    expect(authStore.getState().loading).toBe(true);
    completeHandshake?.();
    await loginPromise;

    expect(authStore.getState().accessToken).toBe('desktop-token');
    expect(authStore.getState().user?.id).toBe('desktop-user');
    expect(authStore.getState().loading).toBe(false);
  });

  it('keeps the app logged out when the desktop Core handshake fails', async () => {
    const authenticateDesktopTicket = vi.fn(async () => {
      throw new Error('Core rejected the ticket');
    });
    vi.stubGlobal('window', desktopWindow(authenticateDesktopTicket));
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-desktop-failure' });
    client.login.mockResolvedValue({
      access_token: 'desktop-token',
      user: { id: 'desktop-user', username: 'desktop' },
    });
    client.issueLocalConnectorTicket.mockResolvedValue({ ticket: 'desktop-ticket' });

    await expect(authStore.getState().login('desktop', 'secret')).rejects.toThrow(
      'Local Connector 登录同步失败',
    );

    expect(authStore.getState().accessToken).toBeNull();
    expect(authStore.getState().user).toBeNull();
    expect(authStore.getState().error).toContain('Core rejected the ticket');
    expect(client.setAccessToken).toHaveBeenLastCalledWith(null);
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

  it('deduplicates concurrent desktop bootstrap handshakes', async () => {
    const authenticateDesktopTicket = vi.fn(async () => ({ configured: true }));
    vi.stubGlobal('window', desktopWindow(authenticateDesktopTicket));
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-bootstrap-single-flight' });
    client.getMe.mockResolvedValue({
      user: { id: 'persisted_user', username: 'persisted' },
    });
    client.issueLocalConnectorTicket.mockResolvedValue({ ticket: 'bootstrap-ticket' });
    authStore.setState({
      accessToken: 'persisted_token',
      initialized: false,
    });

    await Promise.all([
      authStore.getState().bootstrap(),
      authStore.getState().bootstrap(),
    ]);

    expect(client.getMe).toHaveBeenCalledTimes(1);
    expect(client.issueLocalConnectorTicket).toHaveBeenCalledTimes(1);
    expect(authenticateDesktopTicket).toHaveBeenCalledTimes(1);
    expect(authStore.getState().user?.id).toBe('persisted_user');
  });

  it('keeps the persisted login when bootstrap validation fails transiently', async () => {
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-transient' });
    client.getMe.mockRejectedValue(new ApiRequestError('service unavailable', { status: 503 }));
    authStore.setState({
      accessToken: 'persisted_token',
      user: { id: 'persisted_user', username: 'persisted' },
      initialized: false,
    });

    await authStore.getState().bootstrap();

    expect(authStore.getState().accessToken).toBe('persisted_token');
    expect(authStore.getState().user?.id).toBe('persisted_user');
    expect(authStore.getState().initialized).toBe(true);
    expect(client.setAccessToken).not.toHaveBeenCalledWith(null);
  });

  it('clears the persisted login when bootstrap validation is unauthorized', async () => {
    const client = buildFakeAuthClient();
    const authStore = createAuthStore(client as never, { storageKey: 'test-auth-store-unauthorized' });
    client.getMe.mockRejectedValue(new ApiRequestError('unauthorized', { status: 401 }));
    authStore.setState({
      accessToken: 'expired_token',
      user: { id: 'expired_user', username: 'expired' },
      initialized: false,
    });

    await authStore.getState().bootstrap();

    expect(authStore.getState().accessToken).toBeNull();
    expect(authStore.getState().user).toBeNull();
    expect(authStore.getState().initialized).toBe(true);
    expect(client.setAccessToken).toHaveBeenCalledWith(null);
  });
});

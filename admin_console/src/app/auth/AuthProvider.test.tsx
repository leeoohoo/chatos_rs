import { App as AntdApp, Button } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { adminApi, AdminSessionError } from '../../shared/api/adminApi';
import { clearAuthToken, getAuthToken, ADMIN_AUTH_TOKEN_KEY } from '../../shared/auth/tokenStore';
import { AdminI18nProvider } from '../i18n/AdminI18nProvider';
import { AdminAuthProvider, useAdminAuth } from './AuthProvider';

vi.mock('../../shared/api/adminApi', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../shared/api/adminApi')>();
  return { ...original, adminApi: { restoreSession: vi.fn(), login: vi.fn(), logout: vi.fn() } };
});

const session = { token: 'memory-only-token', user: { id: 'admin-id', username: 'admin', display_name: 'Admin', role: 'super_admin' } };

function ProtectedPage() {
  const auth = useAdminAuth();
  return <div>Protected content<Button onClick={() => void auth.logout()}>Sign out test</Button></div>;
}

function mount() {
  return render(<AntdApp><QueryClientProvider client={new QueryClient()}><AdminI18nProvider>
    <AdminAuthProvider><ProtectedPage /></AdminAuthProvider>
  </AdminI18nProvider></QueryClientProvider></AntdApp>);
}

describe('administrator browser session recovery', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    clearAuthToken();
    localStorage.clear();
    vi.mocked(adminApi.restoreSession).mockResolvedValue(session);
  });

  it('restores login after a page reload without any token in browser storage', async () => {
    const page = mount();
    await screen.findByText('Protected content');
    expect(getAuthToken()).toBe(session.token);
    expect(localStorage.getItem(ADMIN_AUTH_TOKEN_KEY)).toBeNull();
    page.unmount();
    clearAuthToken(); // A new document starts without the old page's JavaScript memory.
    mount();
    await screen.findByText('Protected content');
    expect(adminApi.restoreSession).toHaveBeenCalledTimes(2);
    expect(getAuthToken()).toBe(session.token);
    expect(localStorage.getItem(ADMIN_AUTH_TOKEN_KEY)).toBeNull();
  });

  it('shows login only when the server confirms the session has expired', async () => {
    vi.mocked(adminApi.restoreSession).mockRejectedValue(new AdminSessionError(401, 'expired'));
    mount();
    await screen.findByText('ChatOS 统一管理端');
    expect(getAuthToken()).toBeNull();
  });

  it('allows retry after a temporary server failure instead of treating it as logout', async () => {
    vi.mocked(adminApi.restoreSession).mockRejectedValueOnce(new AdminSessionError(503, 'unavailable'));
    mount();
    await screen.findByText('暂时无法恢复登录状态，请检查网络后重试');
    expect(screen.queryByText('ChatOS 统一管理端')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    await screen.findByText('Protected content');
  });

  it('does not pretend to sign out when the server cannot clear the cookie', async () => {
    vi.mocked(adminApi.logout).mockRejectedValueOnce(new Error('offline')).mockResolvedValue(undefined);
    mount();
    await screen.findByText('Protected content');
    fireEvent.click(screen.getByRole('button', { name: 'Sign out test' }));
    await screen.findByText('退出登录失败，请检查网络后重试');
    expect(getAuthToken()).toBe(session.token);
    fireEvent.click(screen.getByRole('button', { name: 'Sign out test' }));
    await waitFor(() => expect(getAuthToken()).toBeNull());
    await screen.findByText('ChatOS 统一管理端');
  });
});

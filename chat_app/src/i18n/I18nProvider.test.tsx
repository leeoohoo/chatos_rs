// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiClientProvider } from '../lib/api/ApiClientContext';
import {
  AuthStoreProvider,
  useAuthStore,
  useAuthStoreContext,
} from '../lib/auth/authStore';
import { I18nProvider, useI18n } from './I18nProvider';

const resetAuthStoreState = () => {
  useAuthStore.setState({
    accessToken: null,
    user: null,
    initialized: false,
    loading: false,
    error: null,
  });
};

const LocaleProbe = () => {
  const { locale } = useI18n();
  return <div data-testid="locale-probe">{locale}</div>;
};

const AuthStateSeeder = ({
  initialized,
  userId,
}: {
  initialized: boolean;
  userId: string | null;
}) => {
  const store = useAuthStoreContext();

  React.useEffect(() => {
    store.setState({
      initialized: false,
      user: null,
      accessToken: null,
      loading: false,
      error: null,
    });

    store.setState({
      initialized,
      user: userId ? { id: userId, username: userId } : null,
    });
  }, [initialized, store, userId]);

  return null;
};

const buildProviderClient = (
  overrides: Partial<{
    getUserSettings: ReturnType<typeof vi.fn>;
  }> = {},
) => ({
  getBaseUrl: vi.fn(() => 'http://127.0.0.1:3997/api'),
  setAccessToken: vi.fn(),
  onAccessTokenRefresh: vi.fn(() => () => undefined),
  getUserSettings: vi.fn().mockResolvedValue({
    effective: {
      UI_LOCALE: 'zh-CN',
    },
  }),
  ...overrides,
});

describe('I18nProvider api client awareness', () => {
  beforeEach(() => {
    window.localStorage.clear();
    resetAuthStoreState();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    window.localStorage.clear();
    resetAuthStoreState();
  });

  it('keeps the local fallback when no ApiClientProvider is mounted', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    window.localStorage.setItem('chat_ui_locale', 'en-US');
    useAuthStore.setState({
      initialized: true,
      user: { id: 'user_1', username: 'tester' },
    });

    render(
      <I18nProvider>
        <LocaleProbe />
      </I18nProvider>,
    );

    expect(screen.getByTestId('locale-probe')).toHaveTextContent('en-US');

    await waitFor(() => {
      expect(fetchSpy).not.toHaveBeenCalled();
    });
  });

  it('keeps the local fallback when auth provider is missing even if api client exists', async () => {
    const getUserSettings = vi.fn().mockResolvedValue({
      effective: {
        UI_LOCALE: 'en-US',
      },
    });
    const client = buildProviderClient({ getUserSettings });
    window.localStorage.setItem('chat_ui_locale', 'zh-CN');

    useAuthStore.setState({
      initialized: true,
      user: { id: 'user_2', username: 'tester' },
    });

    render(
      <ApiClientProvider client={client as never}>
        <I18nProvider>
          <LocaleProbe />
        </I18nProvider>
      </ApiClientProvider>,
    );

    await waitFor(() => {
      expect(getUserSettings).not.toHaveBeenCalled();
      expect(screen.getByTestId('locale-probe')).toHaveTextContent('zh-CN');
    });
  });

  it('loads remote settings when both api client and auth provider are available', async () => {
    const getUserSettings = vi.fn().mockResolvedValue({
      effective: {
        UI_LOCALE: 'en-US',
      },
    });
    const client = buildProviderClient({ getUserSettings });

    render(
      <ApiClientProvider client={client as never}>
        <AuthStoreProvider customApiClient={client as never} storageKey="test-i18n-auth-store">
          <AuthStateSeeder initialized userId="user_3" />
          <I18nProvider>
            <LocaleProbe />
          </I18nProvider>
        </AuthStoreProvider>
      </ApiClientProvider>,
    );

    await waitFor(() => {
      expect(getUserSettings).toHaveBeenCalledWith('user_3');
      expect(screen.getByTestId('locale-probe')).toHaveTextContent('en-US');
    });
  });
});

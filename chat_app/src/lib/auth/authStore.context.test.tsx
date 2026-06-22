// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { ApiClientProvider } from '../api/ApiClientContext';
import {
  AuthStoreProvider,
  useAuthStore,
  useAuthStoreContext,
  useAuthStoreSelector,
} from './authStore';

const resetGlobalAuthStoreState = () => {
  useAuthStore.setState({
    accessToken: null,
    user: null,
    initialized: false,
    loading: false,
    error: null,
  });
};

const ErrorWriter = ({ value }: { value: string }) => {
  const store = useAuthStoreContext();

  React.useEffect(() => {
    store.setState({ error: value });
  }, [store, value]);

  return null;
};

const ErrorReader = ({ testId }: { testId: string }) => {
  const error = useAuthStoreSelector((state) => state.error);
  return <div data-testid={testId}>{error || 'empty'}</div>;
};

describe('AuthStoreProvider isolation', () => {
  beforeEach(() => {
    window.localStorage.clear();
    resetGlobalAuthStoreState();
  });

  afterEach(() => {
    cleanup();
    window.localStorage.clear();
    resetGlobalAuthStoreState();
  });

  it('creates an isolated store per provider instance', async () => {
    render(
      <ApiClientProvider>
        <>
          <AuthStoreProvider storageKey="test-auth-provider-a">
            <ErrorWriter value="provider-a-error" />
            <ErrorReader testId="reader-a" />
          </AuthStoreProvider>
          <AuthStoreProvider storageKey="test-auth-provider-b">
            <ErrorReader testId="reader-b" />
          </AuthStoreProvider>
        </>
      </ApiClientProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId('reader-a')).toHaveTextContent('provider-a-error');
      expect(screen.getByTestId('reader-b')).toHaveTextContent('empty');
    });

    expect(useAuthStore.getState().error).toBeNull();
  });
});

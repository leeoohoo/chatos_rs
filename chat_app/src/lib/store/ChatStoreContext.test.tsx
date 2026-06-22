// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, describe, expect, it } from 'vitest';

import { ApiClientProvider } from '../api/ApiClientContext';
import { useChatStore } from './index';
import { ChatStoreProvider, useChatStoreFromContext } from './ChatStoreContext';

const ErrorWriter = ({ value }: { value: string }) => {
  const { setError } = useChatStoreFromContext();

  React.useEffect(() => {
    setError(value);
  }, [setError, value]);

  return null;
};

const ErrorReader = ({ testId }: { testId: string }) => {
  const { error } = useChatStoreFromContext();
  return <div data-testid={testId}>{error || 'empty'}</div>;
};

describe('ChatStoreProvider isolation', () => {
  afterEach(() => {
    cleanup();
    useChatStore.setState({ error: null });
  });

  it('creates an isolated store per provider instance', async () => {
    render(
      <ApiClientProvider>
        <>
          <ChatStoreProvider>
            <ErrorWriter value="provider-a-error" />
            <ErrorReader testId="reader-a" />
          </ChatStoreProvider>
          <ChatStoreProvider>
            <ErrorReader testId="reader-b" />
          </ChatStoreProvider>
        </>
      </ApiClientProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId('reader-a')).toHaveTextContent('provider-a-error');
      expect(screen.getByTestId('reader-b')).toHaveTextContent('empty');
    });
  });
});

// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Message } from '../types';
import { I18nProvider } from '../i18n/I18nProvider';
import MessageList from './MessageList';

const buildMessage = (id: string, role: Message['role'], content: string): Message => ({
  id,
  sessionId: 'session-1',
  role,
  content,
  status: 'completed',
  createdAt: new Date('2026-05-28T00:00:00.000Z'),
  metadata: {
    conversation_turn_id: 'turn-1',
    ...(role === 'assistant'
      ? {
        historyFinalForUserMessageId: 'user-1',
        historyFinalForTurnId: 'turn-1',
      }
      : {}),
  },
});

describe('MessageList initial scroll', () => {
  const originalRaf = globalThis.requestAnimationFrame;
  const originalCancelRaf = globalThis.cancelAnimationFrame;
  const originalResizeObserver = globalThis.ResizeObserver;
  const originalIntersectionObserver = globalThis.IntersectionObserver;

  beforeEach(() => {
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      disconnect() {}
    });
    vi.stubGlobal('IntersectionObserver', class {
      constructor(private readonly callback: IntersectionObserverCallback) {}

      observe() {
        this.callback([{ isIntersecting: true } as IntersectionObserverEntry], this as unknown as IntersectionObserver);
      }

      disconnect() {}
      unobserve() {}
      takeRecords() {
        return [];
      }
      root = null;
      rootMargin = '';
      thresholds = [0.98];
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    globalThis.requestAnimationFrame = originalRaf;
    globalThis.cancelAnimationFrame = originalCancelRaf;
    globalThis.ResizeObserver = originalResizeObserver;
    globalThis.IntersectionObserver = originalIntersectionObserver;
  });

  it('waits for actual messages before consuming the initial scroll-to-bottom pass', () => {
    const { container, rerender } = render(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[]}
          isLoading
          hasMore={false}
        />
      </I18nProvider>,
    );

    const scrollContainer = container.querySelector('.overflow-y-auto') as HTMLDivElement;
    expect(scrollContainer).toBeTruthy();

    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 640,
    });
    Object.defineProperty(scrollContainer, 'scrollTop', {
      configurable: true,
      writable: true,
      value: 0,
    });

    rerender(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[
            buildMessage('user-1', 'user', 'hello'),
            buildMessage('assistant-1', 'assistant', 'hi'),
          ]}
          isLoading={false}
          hasMore={false}
        />
      </I18nProvider>,
    );

    expect(scrollContainer.scrollTop).toBe(640);
  });

  it('keeps the list pinned to the bottom when a new message arrives', () => {
    const { container, rerender } = render(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[]}
          isLoading
          hasMore={false}
        />
      </I18nProvider>,
    );

    const scrollContainer = container.querySelector('.overflow-y-auto') as HTMLDivElement;
    expect(scrollContainer).toBeTruthy();

    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 640,
    });
    Object.defineProperty(scrollContainer, 'clientHeight', {
      configurable: true,
      value: 420,
    });
    Object.defineProperty(scrollContainer, 'scrollTop', {
      configurable: true,
      writable: true,
      value: 0,
    });

    rerender(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[
            buildMessage('user-1', 'user', 'hello'),
            buildMessage('assistant-1', 'assistant', 'hi'),
          ]}
          isLoading={false}
          hasMore={false}
        />
      </I18nProvider>,
    );

    expect(scrollContainer.scrollTop).toBe(640);

    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 820,
    });

    rerender(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[
            buildMessage('user-1', 'user', 'hello'),
            buildMessage('assistant-1', 'assistant', 'hi'),
            buildMessage('assistant-2', 'assistant', 'task callback complete'),
          ]}
          isLoading={false}
          hasMore={false}
        />
      </I18nProvider>,
    );

    expect(scrollContainer.scrollTop).toBe(820);
  });

  it('does not automatically jump to latest when auto-scroll is disabled', () => {
    const { container, rerender } = render(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[]}
          isLoading
          hasMore={false}
          autoScrollToLatest={false}
        />
      </I18nProvider>,
    );

    const scrollContainer = container.querySelector('.overflow-y-auto') as HTMLDivElement;
    expect(scrollContainer).toBeTruthy();

    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 640,
    });
    Object.defineProperty(scrollContainer, 'clientHeight', {
      configurable: true,
      value: 420,
    });
    Object.defineProperty(scrollContainer, 'scrollTop', {
      configurable: true,
      writable: true,
      value: 0,
    });

    rerender(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[
            buildMessage('user-1', 'user', 'hello'),
            buildMessage('assistant-1', 'assistant', 'hi'),
          ]}
          isLoading={false}
          hasMore={false}
          autoScrollToLatest={false}
        />
      </I18nProvider>,
    );

    expect(scrollContainer.scrollTop).toBe(0);

    Object.defineProperty(scrollContainer, 'scrollHeight', {
      configurable: true,
      value: 820,
    });

    rerender(
      <I18nProvider>
        <MessageList
          sessionId="session-1"
          messages={[
            buildMessage('user-1', 'user', 'hello'),
            buildMessage('assistant-1', 'assistant', 'hi'),
            buildMessage('assistant-2', 'assistant', 'task callback complete'),
          ]}
          isLoading={false}
          hasMore={false}
          autoScrollToLatest={false}
        />
      </I18nProvider>,
    );

    expect(scrollContainer.scrollTop).toBe(0);
  });
});

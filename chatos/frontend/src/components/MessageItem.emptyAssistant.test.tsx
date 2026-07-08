// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../i18n/I18nProvider';
import type { Message } from '../types';
import { MessageItem } from './MessageItem';

vi.mock('./AttachmentRenderer', () => ({
  AttachmentRenderer: () => null,
}));

vi.mock('./messageItem/MessageActions', () => ({
  MessageActions: () => null,
}));

vi.mock('./messageItem/MessageAvatar', () => ({
  MessageAvatar: () => null,
}));

vi.mock('./messageItem/MessageEditForm', () => ({
  MessageEditForm: () => null,
}));

vi.mock('./messageItem/MessageHeader', () => ({
  MessageHeader: () => null,
}));

vi.mock('./messageItem/SessionSummaryCard', () => ({
  SessionSummaryCard: () => null,
}));

vi.mock('./messageTasks/MessageTaskDrawer', () => ({
  MessageTaskDrawer: () => null,
}));

const buildAssistantMessage = (overrides: Partial<Message> = {}): Message => ({
  id: 'assistant-1',
  sessionId: 'session-1',
  role: 'assistant',
  content: '',
  status: 'completed',
  createdAt: new Date('2026-06-12T10:00:00.000Z'),
  metadata: {
    historyFinalForUserMessageId: 'user-1',
    historyFinalForTurnId: 'turn-1',
  },
  ...overrides,
});

describe('MessageItem empty assistant rendering', () => {
  const originalResizeObserver = globalThis.ResizeObserver;

  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      disconnect() {}
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    globalThis.ResizeObserver = originalResizeObserver;
  });

  it('hides non-task-runner assistant messages that collapse to no visible body', () => {
    const message = buildAssistantMessage({
      metadata: {
        historyFinalForUserMessageId: 'user-1',
        historyFinalForTurnId: 'turn-1',
        contentSegments: [
          { type: 'thinking', content: '内部推理' },
          { type: 'tool_call', toolCallId: 'tool-1', content: '' },
        ],
      },
    });

    const { container } = render(
      <I18nProvider>
        <MessageItem message={message} />
      </I18nProvider>,
    );

    expect(container.firstChild).toBeNull();
  });

  it('keeps non-task-runner assistant messages when collapsed text is visible', () => {
    const message = buildAssistantMessage({
      metadata: {
        historyFinalForUserMessageId: 'user-1',
        historyFinalForTurnId: 'turn-1',
        contentSegments: [
          { type: 'thinking', content: '内部推理' },
          { type: 'text', content: '最终回答' },
        ],
      },
    });

    render(
      <I18nProvider>
        <MessageItem message={message} />
      </I18nProvider>,
    );

    expect(screen.getByText('最终回答')).toBeInTheDocument();
  });
});

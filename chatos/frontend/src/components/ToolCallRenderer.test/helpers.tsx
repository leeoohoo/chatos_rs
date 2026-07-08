// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import '@testing-library/jest-dom/vitest';
import type { ReactElement } from 'react';
import { cleanup, render } from '@testing-library/react';
import { afterEach, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { useAuthStore } from '../../lib/auth/authStore';
import type { Message, ToolCall } from '../../types';
import { ToolCallRenderer as ToolCallRendererComponent } from '../ToolCallRenderer';

vi.mock('../LazyMarkdownRenderer', () => ({
  LazyMarkdownRenderer: ({ content }: { content: string }) => (
    <div data-testid="lazy-markdown">{content}</div>
  ),
}));

export const buildToolCall = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  id: 'tool_1',
  messageId: 'msg_1',
  name: 'web_extract',
  arguments: { url: 'https://example.com' },
  result: {},
  createdAt: new Date('2026-04-15T10:00:00Z'),
  ...overrides,
});

export const buildToolResultMessage = (overrides: Partial<Message> = {}): Message => ({
  id: 'tool_msg_1',
  sessionId: 'session_1',
  role: 'tool',
  content: 'summary only',
  status: 'completed',
  createdAt: new Date('2026-04-15T10:00:01Z'),
  metadata: {},
  ...overrides,
});

export const renderWithEnglishI18n = (ui: ReactElement) => {
  window.localStorage.setItem('chat_ui_locale', 'en-US');
  useAuthStore.setState({
    initialized: false,
    user: null,
  });
  return render(<I18nProvider>{ui}</I18nProvider>);
};

export const cleanupToolCallRendererTest = () => {
  window.localStorage.removeItem('chat_ui_locale');
  cleanup();
};

afterEach(cleanupToolCallRendererTest);

export const ToolCallRenderer = ToolCallRendererComponent;
export type { Message, ToolCall };

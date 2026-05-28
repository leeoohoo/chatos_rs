// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../../i18n/I18nProvider';
import { SessionSection } from './SessionSection';
import type { Session } from '../../../types';

const buildSession = (): Session => ({
  id: 'session-1',
  title: '会话一',
  createdAt: new Date('2026-05-25T10:00:00.000Z'),
  updatedAt: new Date('2026-05-25T10:00:00.000Z'),
  messageCount: 2,
  tokenUsage: 0,
  pinned: false,
  archived: false,
});

const baseProps = {
  expanded: true,
  sessions: [buildSession()],
  currentSessionId: 'session-1',
  summarySessionId: null,
  runtimeContextSessionId: null,
  displaySessionRuntimeIdMap: {},
  sessionChatState: {
    'session-1': {
      isLoading: true,
      isStreaming: true,
      streamingPhase: 'reviewing' as const,
    },
  },
  taskReviewPanelsBySession: {},
  uiPromptPanelsBySession: {},
  hasMore: false,
  isRefreshing: false,
  isLoadingMore: false,
  onToggle: vi.fn(),
  onRefresh: vi.fn(),
  onCreateSession: vi.fn(),
  onSelectSession: vi.fn(),
  onOpenSummary: vi.fn(),
  onOpenRuntimeContext: vi.fn(),
  onDeleteSession: vi.fn(),
  onLoadMore: vi.fn(),
  onToggleActionMenu: vi.fn(),
  closeActionMenus: vi.fn(),
  formatTimeAgo: vi.fn(() => 'just now'),
  getSessionStatus: vi.fn(() => 'active' as const),
};

describe('SessionSection status badge', () => {
  afterEach(() => {
    window.localStorage.removeItem('chat_ui_locale');
    cleanup();
  });

  it('renders reviewing badge when chat state is reviewing', () => {
    window.localStorage.setItem('chat_ui_locale', 'en-US');

    render(
      <I18nProvider>
        <SessionSection {...baseProps} />
      </I18nProvider>,
    );

    expect(screen.getByText('Reviewing')).toBeInTheDocument();
  });

  it('renders thinking badge when pending runtime panels exist without streaming flags', () => {
    window.localStorage.setItem('chat_ui_locale', 'en-US');

    render(
      <I18nProvider>
        <SessionSection
          {...baseProps}
          sessionChatState={{
            'session-1': {
              isLoading: false,
              isStreaming: false,
              streamingPhase: null,
            },
          }}
          taskReviewPanelsBySession={{
            'session-1': [{
              reviewId: 'review-1',
              sessionId: 'session-1',
              conversationTurnId: 'turn-1',
              drafts: [],
            }],
          }}
        />
      </I18nProvider>,
    );

    expect(screen.getByText('Thinking')).toBeInTheDocument();
  });
});

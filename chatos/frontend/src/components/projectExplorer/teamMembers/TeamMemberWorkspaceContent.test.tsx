// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../../i18n/I18nProvider';
import { TeamMemberWorkspaceContent } from './TeamMemberWorkspaceContent';

vi.mock('../../MessageList', () => ({
  MessageList: ({
    hasMore,
    onLoadMore,
  }: {
    hasMore?: boolean;
    onLoadMore?: () => void;
  }) => (
    <button
      type="button"
      data-testid="message-list-load-more"
      disabled={!hasMore}
      onClick={onLoadMore}
    >
      Load more
    </button>
  ),
}));

const buildProps = (): ComponentProps<typeof TeamMemberWorkspaceContent> => ({
  selectedContact: {
    id: 'contact-1',
    agentId: 'agent-1',
    name: 'Agent One',
  },
  selectedProjectSession: {
    id: 'session-1',
    title: 'Session One',
    createdAt: new Date('2026-08-21T00:00:00.000Z'),
    updatedAt: new Date('2026-08-21T00:00:00.000Z'),
    messageCount: 1,
    tokenUsage: 0,
    pinned: false,
    archived: false,
  },
  isSelectedSessionActive: true,
  sessionSummaryPaneVisible: false,
  summaryItems: [],
  summaryLoading: false,
  summaryError: null,
  clearingSummaries: false,
  deletingSummaryId: null,
  messages: [],
  hasMoreMessages: true,
  anchorMessageId: null,
  anchorRequestKey: 0,
  onAnchorClear: vi.fn(),
  onLoadMore: vi.fn(),
  onClearSummaries: vi.fn(),
  onRefreshSummaries: vi.fn(),
  onCloseSummary: vi.fn(),
  onDeleteSummary: vi.fn(),
});

describe('TeamMemberWorkspaceContent message pagination', () => {
  it('exposes compact-history pagination in the normal message view', () => {
    const props = buildProps();

    render(
      <I18nProvider>
        <TeamMemberWorkspaceContent {...props} />
      </I18nProvider>,
    );

    const loadMoreButton = screen.getByTestId('message-list-load-more');
    expect(loadMoreButton).toBeEnabled();

    fireEvent.click(loadMoreButton);
    expect(props.onLoadMore).toHaveBeenCalledTimes(1);
  });
});

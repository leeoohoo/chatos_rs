// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../../i18n/I18nProvider';
import TeamMembersSidebar from './TeamMembersSidebar';
import type { ContactItem, ProjectContactRow } from './types';

const buildContact = (): ContactItem => ({
  id: 'contact-1',
  agentId: 'agent-1',
  name: 'Alice',
});

const buildProjectRows = (): ProjectContactRow[] => ([
  {
    contact: buildContact(),
    session: {
      id: 'session-1',
      title: '会话一',
      createdAt: new Date('2026-05-25T10:00:00.000Z'),
      updatedAt: new Date('2026-05-25T10:00:00.000Z'),
      messageCount: 2,
      tokenUsage: 0,
      pinned: false,
      archived: false,
    },
    updatedAt: Date.now(),
  },
]);

const baseProps = {
  projectName: 'Project A',
  projectMembersLoading: false,
  projectMembersError: null,
  memberPickerError: null,
  projectContacts: buildProjectRows(),
  selectedContactId: 'contact-1',
  switchingContactId: null,
  summaryPaneSessionId: null,
  openingSummaryContactId: null,
  runtimeContextSessionId: null,
  openingRuntimeContextContactId: null,
  removingContactId: null,
  taskReviewPanelsBySession: {},
  uiPromptPanelsBySession: {},
  onOpenAddMember: vi.fn(),
  onSelectContact: vi.fn(),
  onOpenSummary: vi.fn(),
  onOpenRuntimeContext: vi.fn(),
  onRemoveMember: vi.fn(),
};

describe('TeamMembersSidebar session status', () => {
  afterEach(() => {
    window.localStorage.removeItem('chat_ui_locale');
    cleanup();
  });

  it('shows reviewing when session streaming phase is reviewing', () => {
    window.localStorage.setItem('chat_ui_locale', 'en-US');

    render(
      <I18nProvider>
        <TeamMembersSidebar
          {...baseProps}
          sessionChatState={{
            'session-1': {
              isLoading: true,
              isStreaming: true,
              streamingPhase: 'reviewing',
            },
          }}
        />
      </I18nProvider>,
    );

    expect(screen.getByText('Reviewing')).toBeInTheDocument();
  });
});

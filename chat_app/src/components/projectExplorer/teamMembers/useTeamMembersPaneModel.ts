// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useMemo, type ComponentProps } from 'react';

import type { Project } from '../../../types';
import TurnRuntimeContextDrawer from '../../chatInterface/TurnRuntimeContextDrawer';
import TeamMemberWorkspace from './TeamMemberWorkspace';
import { useTeamMembersPaneSessionResources } from './useTeamMembersPaneSessionResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';
import { useTeamMemberRuntimeContextDrawerProps } from './useTeamMemberOverlayProps';
import { useTeamMemberWorkspaceProps } from './useTeamMemberWorkspaceProps';

interface UseTeamMembersPaneModelOptions {
  project: Project;
}

interface UseTeamMembersPaneModelResult {
  workspaceProps: ComponentProps<typeof TeamMemberWorkspace>;
  runtimeContextDrawerProps: ComponentProps<typeof TurnRuntimeContextDrawer>;
  userMessageSidebarActions: {
    summaryActive: boolean;
    runtimeContextActive: boolean;
    summaryLoading: boolean;
    runtimeContextLoading: boolean;
    summaryDisabled: boolean;
    runtimeContextDisabled: boolean;
    onOpenSummary: () => void;
    onOpenRuntimeContext: () => void;
  };
}

export const useTeamMembersPaneModel = ({
  project,
}: UseTeamMembersPaneModelOptions): UseTeamMembersPaneModelResult => {
  const store = useTeamMembersPaneStoreBridge();
  const resources = useTeamMembersPaneSessionResources({ project, store });
  const options = { project, store, resources };
  const workspaceProps = useTeamMemberWorkspaceProps(options);
  const runtimeContextDrawerProps = useTeamMemberRuntimeContextDrawerProps(options);
  const selectedContact = resources.conversation.selectedContact;
  const selectedSessionId = resources.conversation.selectedProjectSession?.id || null;

  const handleOpenSelectedSummary = useCallback(() => {
    if (!selectedContact) {
      return;
    }
    void resources.conversation.handleOpenSummary(selectedContact);
  }, [resources.conversation.handleOpenSummary, selectedContact]);

  const handleOpenSelectedRuntimeContext = useCallback(() => {
    if (!selectedContact) {
      return;
    }
    void resources.runtimeContext.handleOpenRuntimeContext(selectedContact);
  }, [resources.runtimeContext.handleOpenRuntimeContext, selectedContact]);

  const userMessageSidebarActions = useMemo(() => ({
    summaryActive: Boolean(
      selectedSessionId
      && resources.summary.summaryPaneSessionId === selectedSessionId,
    ),
    runtimeContextActive: Boolean(
      selectedSessionId
      && resources.runtimeContext.runtimeContextOpen
      && resources.runtimeContext.runtimeContextSessionId === selectedSessionId,
    ),
    summaryLoading: Boolean(
      selectedContact?.id
      && resources.conversation.openingSummaryContactId === selectedContact.id,
    ),
    runtimeContextLoading: Boolean(
      selectedContact?.id
      && resources.runtimeContext.openingRuntimeContextContactId === selectedContact.id,
    ),
    summaryDisabled: !selectedContact || !selectedSessionId,
    runtimeContextDisabled: !selectedContact || !selectedSessionId,
    onOpenSummary: handleOpenSelectedSummary,
    onOpenRuntimeContext: handleOpenSelectedRuntimeContext,
  }), [
    handleOpenSelectedRuntimeContext,
    handleOpenSelectedSummary,
    resources.conversation.openingSummaryContactId,
    resources.runtimeContext.openingRuntimeContextContactId,
    resources.runtimeContext.runtimeContextOpen,
    resources.runtimeContext.runtimeContextSessionId,
    resources.summary.summaryPaneSessionId,
    selectedContact,
    selectedSessionId,
  ]);

  return {
    workspaceProps,
    runtimeContextDrawerProps,
    userMessageSidebarActions,
  };
};

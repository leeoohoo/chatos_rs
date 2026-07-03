// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import type { ComponentProps } from 'react';

import TeamMemberWorkspace from './TeamMemberWorkspace';
import type { UseTeamMembersPaneViewPropsOptions } from './teamMembersPaneViewPropTypes';

export const useTeamMemberWorkspaceProps = ({
  project,
  store,
  resources,
}: UseTeamMembersPaneViewPropsOptions): ComponentProps<typeof TeamMemberWorkspace> => useMemo(() => {
  const selectedSessionId = resources.conversation.selectedProjectSession?.id || null;
  const hasMoreMessages = Boolean(
    selectedSessionId
    && store.sessionMessagePaginationState?.[selectedSessionId]?.nextBefore
  );
  const selectedRuntimeModelId = resources.composer.composerSelectedModelId;
  const supportsReasoning = Boolean(
    selectedRuntimeModelId
    && (store.aiModelConfigs || []).find((item) => item.id === selectedRuntimeModelId)?.supports_reasoning === true,
  );
  const handleSend: ComponentProps<typeof TeamMemberWorkspace>['onSend'] = async (
    content,
    attachments,
    runtimeOptions,
  ) => {
    await resources.composer.flushRuntimeSettings();
    await resources.conversation.handleSendMessage(content, attachments, runtimeOptions);
  };

  return ({
    project,
    selectedContact: resources.conversation.selectedContact,
    selectedProjectSession: resources.conversation.selectedProjectSession,
    isSelectedSessionActive: resources.conversation.isSelectedSessionActive,
    sessionSummaryPaneVisible: resources.conversation.sessionSummaryPaneVisible,
    summaryItems: resources.summary.summaryItems,
    summaryLoading: resources.summary.summaryLoading,
    summaryError: resources.summary.summaryError,
    clearingSummaries: resources.summary.clearingSummaries,
    deletingSummaryId: resources.summary.deletingSummaryId,
    messages: store.messages,
    hasMoreMessages,
    selectedModelId: selectedRuntimeModelId,
    selectedModelName: resources.composer.composerSelectedModelName,
    selectedThinkingLevel: resources.composer.composerSelectedThinkingLevel,
    aiModelConfigs: store.aiModelConfigs,
    supportsReasoning,
    reasoningEnabled: resources.composer.composerReasoningEnabled,
    planModeEnabled: resources.composer.composerPlanModeEnabled,
    reviewRepairRunning: resources.reviewRepair.reviewRepairRunning,
    availableRemoteConnections: resources.composer.remoteConnections || [],
    currentRemoteConnectionId: resources.composer.currentRemoteConnection?.id || null,
    onRemoteConnectionChange: resources.composer.handleComposerRemoteConnectionChange,
    onLoadMore: resources.conversation.handleLoadMore,
    onClearSummaries: () => {
      void resources.conversation.handleClearSummaries();
    },
    onRefreshSummaries: () => {
      const sessionId = resources.conversation.selectedProjectSession?.id;
      if (!sessionId) {
        return;
      }
      void resources.summary.loadSessionSummaries(sessionId, { force: true });
    },
    onCloseSummary: () => {
      resources.summary.setSummaryPaneSessionId(null);
    },
    onDeleteSummary: (summaryId) => {
      void resources.conversation.handleDeleteSummary(summaryId);
    },
    onSend: handleSend,
    onModelChange: resources.composer.handleComposerSelectedModelChange,
    onModelNameChange: resources.composer.handleComposerSelectedModelNameChange,
    onThinkingLevelChange: resources.composer.handleComposerSelectedThinkingLevelChange,
    onModelRuntimeChange: resources.composer.handleComposerModelRuntimeSelectionChange,
    onReasoningToggle: resources.composer.handleComposerReasoningToggle,
    onPlanModeToggle: resources.composer.handleComposerPlanModeToggle,
  });
}, [
  project,
  resources.composer.composerSelectedModelId,
  resources.composer.composerSelectedModelName,
  resources.composer.composerSelectedThinkingLevel,
  resources.composer.composerReasoningEnabled,
  resources.composer.composerPlanModeEnabled,
  resources.composer.handleComposerModelRuntimeSelectionChange,
  resources.composer.handleComposerSelectedModelChange,
  resources.composer.handleComposerSelectedModelNameChange,
  resources.composer.handleComposerSelectedThinkingLevelChange,
  resources.composer.currentRemoteConnection?.id,
  resources.composer.handleComposerRemoteConnectionChange,
  resources.composer.handleComposerReasoningToggle,
  resources.composer.handleComposerPlanModeToggle,
  resources.composer.flushRuntimeSettings,
  resources.composer.remoteConnections,
  resources.conversation.handleClearSummaries,
  resources.conversation.handleDeleteSummary,
  resources.conversation.handleLoadMore,
  resources.conversation.handleSendMessage,
  resources.conversation.isSelectedSessionActive,
  resources.conversation.selectedContact,
  resources.conversation.selectedProjectSession,
  resources.conversation.sessionSummaryPaneVisible,
  resources.reviewRepair.reviewRepairRunning,
  resources.summary.clearingSummaries,
  resources.summary.deletingSummaryId,
  resources.summary.loadSessionSummaries,
  resources.summary.setSummaryPaneSessionId,
  resources.summary.summaryError,
  resources.summary.summaryItems,
  resources.summary.summaryLoading,
  store.aiModelConfigs,
  store.messages,
  store.sessionMessagePaginationState,
]);

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
  reasoningEnabled: store.chatConfig?.reasoningEnabled === true,
  planModeEnabled: store.chatConfig?.planModeEnabled === true,
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
  onSend: resources.conversation.handleSendMessage,
  onModelChange: resources.composer.handleComposerSelectedModelChange,
  onModelNameChange: resources.composer.handleComposerSelectedModelNameChange,
  onThinkingLevelChange: resources.composer.handleComposerSelectedThinkingLevelChange,
  onModelRuntimeChange: resources.composer.handleComposerModelRuntimeSelectionChange,
  onReasoningToggle: (enabled) => store.updateChatConfig({ reasoningEnabled: enabled }),
  onPlanModeToggle: (enabled) => store.updateChatConfig({ planModeEnabled: enabled }),
  });
}, [
  project,
  resources.composer.composerSelectedModelId,
  resources.composer.composerSelectedModelName,
  resources.composer.composerSelectedThinkingLevel,
  resources.composer.handleComposerModelRuntimeSelectionChange,
  resources.composer.handleComposerSelectedModelChange,
  resources.composer.handleComposerSelectedModelNameChange,
  resources.composer.handleComposerSelectedThinkingLevelChange,
  resources.composer.currentRemoteConnection?.id,
  resources.composer.handleComposerRemoteConnectionChange,
  resources.composer.remoteConnections,
  resources.conversation.handleClearSummaries,
  resources.conversation.handleDeleteSummary,
  resources.conversation.handleLoadMore,
  resources.conversation.handleSendMessage,
  resources.conversation.isSelectedSessionActive,
  resources.conversation.selectedContact,
  resources.conversation.selectedProjectSession,
  resources.conversation.sessionSummaryPaneVisible,
  resources.summary.clearingSummaries,
  resources.summary.deletingSummaryId,
  resources.summary.loadSessionSummaries,
  resources.summary.setSummaryPaneSessionId,
  resources.summary.summaryError,
  resources.summary.summaryItems,
  resources.summary.summaryLoading,
  store.aiModelConfigs,
  store.chatConfig?.reasoningEnabled,
  store.chatConfig?.planModeEnabled,
  store.messages,
  store.sessionMessagePaginationState,
  store.updateChatConfig,
]);

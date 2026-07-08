// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useMemo } from 'react';

import { useSessionRuntimeSettings } from '../../../features/sessionRuntime/useSessionRuntimeSettings';
import { countPendingReviewRepairMessages } from '../../../lib/domain/reviewRepair';
import { useConversationSummariesRealtime } from '../../../lib/realtime/useConversationSummariesRealtime';
import { useReviewRepairRealtime } from '../../../lib/realtime/useReviewRepairRealtime';
import type { ContactItem } from './types';
import { useTeamMemberRuntimeContext } from './useTeamMemberRuntimeContext';
import { useTeamMembersContactResources } from './useTeamMembersContactResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';

interface UseTeamMembersRuntimeResourcesOptions {
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
  contacts: ReturnType<typeof useTeamMembersContactResources>;
}

export const useTeamMembersRuntimeResources = ({
  store,
  contacts,
}: UseTeamMembersRuntimeResourcesOptions) => {
  const {
    apiClient,
    sessions,
    remoteConnections,
    messages,
    sessionChatState,
    loadMessages,
  } = store;
  const {
    normalizedProjectId,
    ensureContactSession,
    conversation,
    summary,
    members,
  } = contacts;

  const runtimeContextRefreshNonce = useMemo(() => {
    if (!conversation.selectedProjectSession?.id) {
      return 0;
    }
    return sessionChatState?.[conversation.selectedProjectSession.id]?.runtimeContextRefreshNonce || 0;
  }, [conversation.selectedProjectSession?.id, sessionChatState]);
  const {
    selectedModelId: composerSelectedModelId,
    selectedModelName: composerSelectedModelName,
    selectedThinkingLevel: composerSelectedThinkingLevel,
    remoteConnectionId: composerRemoteConnectionId,
    reasoningEnabled: composerReasoningEnabled,
    planModeEnabled: composerPlanModeEnabled,
    setSelectedModelId: handleComposerSelectedModelChange,
    setSelectedModelName: handleComposerSelectedModelNameChange,
    setSelectedThinkingLevel: handleComposerSelectedThinkingLevelChange,
    setModelRuntimeSelection: handleComposerModelRuntimeSelectionChange,
    setRemoteConnectionId: handleComposerRemoteConnectionChange,
    setReasoningEnabled: handleComposerReasoningToggle,
    setPlanModeEnabled: handleComposerPlanModeToggle,
    flushRuntimeSettings,
  } = useSessionRuntimeSettings({
    session: conversation.selectedProjectSession,
  });

  const runtimeContext = useTeamMemberRuntimeContext({
    apiClient,
    sessions: sessions || [],
    normalizedProjectId,
    runtimeContextRefreshNonce,
    ensureContactSession,
    setSelectedContactId: conversation.setSelectedContactId,
  });

  useConversationSummariesRealtime({
    sessionId: conversation.selectedProjectSession?.id || null,
    enabled: Boolean(conversation.selectedProjectSession?.id),
    onEvent: async (payload) => {
      const selectedSessionId = conversation.selectedProjectSession?.id || null;
      if (!selectedSessionId) {
        return;
      }
      if (Array.isArray(payload?.items)) {
        summary.applyRealtimeSessionSummaries(selectedSessionId, payload);
        return;
      }
      summary.markSessionSummariesStale(selectedSessionId);
      if (!conversation.sessionSummaryPaneVisible) {
        return;
      }
      summary.hydrateSessionSummariesFromCache(selectedSessionId);
      await summary.loadSessionSummaries(selectedSessionId, { silent: true });
    },
  });

  const selectedReviewRepairSessionId = conversation.selectedProjectSession?.id || null;
  const {
    reviewRepairRunning,
    reviewRepairPendingCount,
    refreshReviewRepairStatus,
    markReviewRepairStarting,
  } = useReviewRepairRealtime({
    apiClient,
    sessionId: selectedReviewRepairSessionId,
    enabled: Boolean(selectedReviewRepairSessionId),
    messageCountHint: selectedReviewRepairSessionId ? messages.length : undefined,
    onCompleted: async () => {
      if (!selectedReviewRepairSessionId) {
        return;
      }
      await loadMessages(selectedReviewRepairSessionId);
      summary.markSessionSummariesStale(selectedReviewRepairSessionId);
      summary.hydrateSessionSummariesFromCache(selectedReviewRepairSessionId);
      if (conversation.sessionSummaryPaneVisible) {
        await summary.loadSessionSummaries(selectedReviewRepairSessionId, { silent: true, force: true });
      }
    },
    onFailed: (errorMessage) => {
      console.error('Team member review repair failed:', errorMessage);
    },
  });

  const loadedReviewRepairPendingCount = selectedReviewRepairSessionId
    ? countPendingReviewRepairMessages(messages, selectedReviewRepairSessionId)
    : 0;
  const reviewRepairDisabled = !reviewRepairRunning
    && reviewRepairPendingCount === 0
    && loadedReviewRepairPendingCount === 0;

  const handleRunReviewRepair = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    markReviewRepairStarting();
    try {
      const result = await apiClient.runConversationReviewRepair(sessionId);
      if (result?.success === false) {
        throw new Error(result.detail || result.error || '执行复盘失败');
      }
    } catch (error) {
      await refreshReviewRepairStatus(sessionId).catch((statusError) => {
        console.error('Failed to refresh team review repair status after run error:', statusError);
      });
      console.error('Failed to run team review repair:', error);
    }
  }, [
    apiClient,
    markReviewRepairStarting,
    refreshReviewRepairStatus,
  ]);

  const handleRemoveMember = useCallback(async (contact: ContactItem) => {
    const targetSessionId = members.projectContacts.find(
      (item) => item.contact.id === contact.id,
    )?.session?.id || null;
    const removed = await members.removeMemberFromManager(contact);
    if (!removed) {
      return;
    }
    if (conversation.selectedContactId === contact.id) {
      conversation.setSelectedContactId(null);
      summary.setSummaryPaneSessionId(null);
      summary.resetSummaryState();
    }
    if (targetSessionId && runtimeContext.runtimeContextSessionId === targetSessionId) {
      runtimeContext.setRuntimeContextOpen(false);
    }
  }, [
    conversation.selectedContactId,
    conversation.setSelectedContactId,
    members,
    runtimeContext,
    summary,
  ]);

  return {
    composer: {
      composerSelectedModelId,
      composerSelectedModelName,
      composerSelectedThinkingLevel,
      composerReasoningEnabled,
      composerPlanModeEnabled,
      handleComposerSelectedModelChange,
      handleComposerSelectedModelNameChange,
      handleComposerSelectedThinkingLevelChange,
      handleComposerModelRuntimeSelectionChange,
      handleComposerRemoteConnectionChange,
      handleComposerReasoningToggle,
      handleComposerPlanModeToggle,
      flushRuntimeSettings,
      remoteConnections,
      currentRemoteConnection: (remoteConnections || []).find(
        (connection) => connection.id === composerRemoteConnectionId,
      ) || null,
    },
    runtimeContext,
    reviewRepair: {
      handleRunReviewRepair,
      reviewRepairRunning,
      reviewRepairPendingCount,
      reviewRepairDisabled,
    },
    handleRemoveMember,
  };
};

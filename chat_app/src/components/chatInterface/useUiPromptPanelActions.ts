import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { UiPromptResponsePayload } from '../../lib/store/types';
import type { UiPromptPanelActionsArgs } from './panelActionTypes';
import { recoverPendingPanelConversation } from './pendingPanelRecovery';

export const useUiPromptPanelActions = ({
  activeUiPromptPanel,
  apiClient,
  chatStoreSet,
  getChatStoreState,
  preferRealtimeSync = false,
  uiPromptHistoryOpen = false,
  upsertUiPromptPanel,
  removeUiPromptPanel,
  loadUiPromptHistory,
  markUiPromptHistoryStale,
  removePendingUiPromptCachePanel,
}: UiPromptPanelActionsArgs) => {
  const { t } = useI18n();
  const handleUiPromptSubmit = useCallback(async (payload: UiPromptResponsePayload) => {
    if (!activeUiPromptPanel) {
      return;
    }

    const pendingPanel = {
      ...activeUiPromptPanel,
      submitting: true,
      error: null,
    };
    upsertUiPromptPanel(pendingPanel);

    try {
      await apiClient.submitUiPromptResponse(activeUiPromptPanel.promptId, payload);
      removePendingUiPromptCachePanel?.(
        activeUiPromptPanel.promptId,
        activeUiPromptPanel.sessionId,
      );
      removeUiPromptPanel(activeUiPromptPanel.promptId, activeUiPromptPanel.sessionId);
      if (chatStoreSet && getChatStoreState) {
        void recoverPendingPanelConversation({
          apiClient,
          getState: getChatStoreState,
          set: chatStoreSet,
          sessionId: activeUiPromptPanel.sessionId,
          conversationTurnId: activeUiPromptPanel.conversationTurnId,
        });
      }
      if (!preferRealtimeSync) {
        if (uiPromptHistoryOpen) {
          await loadUiPromptHistory(activeUiPromptPanel.sessionId, true);
        } else {
          markUiPromptHistoryStale?.(activeUiPromptPanel.sessionId);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : t('uiPrompt.submitFailed');
      upsertUiPromptPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeUiPromptPanel,
    apiClient,
    chatStoreSet,
    getChatStoreState,
    loadUiPromptHistory,
    markUiPromptHistoryStale,
    preferRealtimeSync,
    removePendingUiPromptCachePanel,
    removeUiPromptPanel,
    t,
    uiPromptHistoryOpen,
    upsertUiPromptPanel,
  ]);

  const handleUiPromptCancel = useCallback(async () => {
    if (!activeUiPromptPanel) {
      return;
    }

    const pendingPanel = {
      ...activeUiPromptPanel,
      submitting: true,
      error: null,
    };
    upsertUiPromptPanel(pendingPanel);

    try {
      await apiClient.submitUiPromptResponse(activeUiPromptPanel.promptId, {
        status: 'canceled',
        reason: 'user_cancelled',
      });
      removePendingUiPromptCachePanel?.(
        activeUiPromptPanel.promptId,
        activeUiPromptPanel.sessionId,
      );
      removeUiPromptPanel(activeUiPromptPanel.promptId, activeUiPromptPanel.sessionId);
      if (chatStoreSet && getChatStoreState) {
        void recoverPendingPanelConversation({
          apiClient,
          getState: getChatStoreState,
          set: chatStoreSet,
          sessionId: activeUiPromptPanel.sessionId,
          conversationTurnId: activeUiPromptPanel.conversationTurnId,
        });
      }
      if (!preferRealtimeSync) {
        if (uiPromptHistoryOpen) {
          await loadUiPromptHistory(activeUiPromptPanel.sessionId, true);
        } else {
          markUiPromptHistoryStale?.(activeUiPromptPanel.sessionId);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : t('uiPrompt.cancelFailed');
      upsertUiPromptPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeUiPromptPanel,
    apiClient,
    chatStoreSet,
    getChatStoreState,
    loadUiPromptHistory,
    markUiPromptHistoryStale,
    preferRealtimeSync,
    removePendingUiPromptCachePanel,
    removeUiPromptPanel,
    t,
    uiPromptHistoryOpen,
    upsertUiPromptPanel,
  ]);

  return {
    handleUiPromptSubmit,
    handleUiPromptCancel,
  };
};

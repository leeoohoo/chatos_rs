import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { TaskReviewDraft } from '../../lib/store/types';
import type { TaskReviewPanelActionsArgs } from './panelActionTypes';
import { recoverPendingPanelConversation } from './pendingPanelRecovery';

export const useTaskReviewPanelActions = ({
  activeTaskReviewPanel,
  apiClient,
  chatStoreSet,
  getChatStoreState,
  preferRealtimeSync = false,
  taskHistoryOpen = false,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  loadCurrentTurnWorkbarTasks,
  loadHistoryWorkbarTasks,
  markHistoryWorkbarTasksStale,
  removePendingTaskReviewCachePanel,
}: TaskReviewPanelActionsArgs) => {
  const { t } = useI18n();
  const handleTaskReviewConfirm = useCallback(async (drafts: TaskReviewDraft[]) => {
    if (!activeTaskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...activeTaskReviewPanel,
      drafts,
      submitting: true,
      error: null,
    };
    upsertTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(activeTaskReviewPanel.reviewId, {
        action: 'confirm',
        tasks: drafts.map((draft) => ({
          title: draft.title,
          details: draft.details,
          priority: draft.priority,
          status: draft.status,
          tags: draft.tags,
          due_at: draft.dueAt || undefined,
        })),
      });
      removePendingTaskReviewCachePanel?.(
        activeTaskReviewPanel.reviewId,
        activeTaskReviewPanel.sessionId,
      );
      removeTaskReviewPanel(activeTaskReviewPanel.reviewId, activeTaskReviewPanel.sessionId);
      if (chatStoreSet && getChatStoreState) {
        void recoverPendingPanelConversation({
          apiClient,
          getState: getChatStoreState,
          set: chatStoreSet,
          sessionId: activeTaskReviewPanel.sessionId,
          conversationTurnId: activeTaskReviewPanel.conversationTurnId,
        });
      }
      if (!preferRealtimeSync) {
        await loadCurrentTurnWorkbarTasks(
          activeTaskReviewPanel.sessionId,
          activeTaskReviewPanel.conversationTurnId,
          true,
        );
        if (taskHistoryOpen) {
          await loadHistoryWorkbarTasks(activeTaskReviewPanel.sessionId, true);
        } else {
          markHistoryWorkbarTasksStale?.(activeTaskReviewPanel.sessionId);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : t('taskDraft.confirmFailed');
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeTaskReviewPanel,
    apiClient,
    chatStoreSet,
    getChatStoreState,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    preferRealtimeSync,
    removePendingTaskReviewCachePanel,
    removeTaskReviewPanel,
    taskHistoryOpen,
    t,
    upsertTaskReviewPanel,
  ]);

  const handleTaskReviewCancel = useCallback(async () => {
    if (!activeTaskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...activeTaskReviewPanel,
      submitting: true,
      error: null,
    };
    upsertTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(activeTaskReviewPanel.reviewId, {
        action: 'cancel',
        reason: 'user_cancelled',
      });
      removePendingTaskReviewCachePanel?.(
        activeTaskReviewPanel.reviewId,
        activeTaskReviewPanel.sessionId,
      );
      removeTaskReviewPanel(activeTaskReviewPanel.reviewId, activeTaskReviewPanel.sessionId);
      if (chatStoreSet && getChatStoreState) {
        void recoverPendingPanelConversation({
          apiClient,
          getState: getChatStoreState,
          set: chatStoreSet,
          sessionId: activeTaskReviewPanel.sessionId,
          conversationTurnId: activeTaskReviewPanel.conversationTurnId,
        });
      }
      if (!preferRealtimeSync) {
        await loadCurrentTurnWorkbarTasks(
          activeTaskReviewPanel.sessionId,
          activeTaskReviewPanel.conversationTurnId,
          true,
        );
        if (taskHistoryOpen) {
          await loadHistoryWorkbarTasks(activeTaskReviewPanel.sessionId, true);
        } else {
          markHistoryWorkbarTasksStale?.(activeTaskReviewPanel.sessionId);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : t('taskDraft.cancelFailed');
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeTaskReviewPanel,
    apiClient,
    chatStoreSet,
    getChatStoreState,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    preferRealtimeSync,
    removePendingTaskReviewCachePanel,
    removeTaskReviewPanel,
    taskHistoryOpen,
    t,
    upsertTaskReviewPanel,
  ]);

  return {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
  };
};

import { useCallback } from 'react';

import type { TaskReviewDraft } from '../../lib/store/types';
import type { TaskReviewPanelActionsArgs } from './panelActionTypes';

export const useTaskReviewPanelActions = ({
  activeTaskReviewPanel,
  apiClient,
  preferRealtimeSync = false,
  taskHistoryOpen = false,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  loadCurrentTurnWorkbarTasks,
  loadHistoryWorkbarTasks,
  markHistoryWorkbarTasksStale,
  removePendingTaskReviewCachePanel,
}: TaskReviewPanelActionsArgs) => {
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
      const message = error instanceof Error ? error.message : '任务确认提交失败';
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeTaskReviewPanel,
    apiClient,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    preferRealtimeSync,
    removePendingTaskReviewCachePanel,
    removeTaskReviewPanel,
    taskHistoryOpen,
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
      const message = error instanceof Error ? error.message : '任务取消提交失败';
      upsertTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [
    activeTaskReviewPanel,
    apiClient,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    preferRealtimeSync,
    removePendingTaskReviewCachePanel,
    removeTaskReviewPanel,
    taskHistoryOpen,
    upsertTaskReviewPanel,
  ]);

  return {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
  };
};

import { useCallback, useEffect, useMemo } from 'react';

import type {
  SessionRuntimeGuidanceState,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';
import { useRealtimeConnectionState } from '../../lib/realtime/RealtimeProvider';
import type { RuntimeGuidanceWorkbarItem } from '../TaskWorkbar';
import {
  pickFirstSessionPanel,
  pickSessionScopedState,
} from './panelStateSync';
import { removePendingTaskReviewCachePanel } from './pendingTaskReviewCache';
import { removePendingUiPromptCachePanel } from './pendingUiPromptCache';
import { usePanelActions } from './usePanelActions';
import { useWorkbarMutations } from './useWorkbarMutations';
import { useWorkbarState } from './useWorkbarState';
import { usePendingWorkbarPanels } from './usePendingWorkbarPanels';
import { useSessionWorkbarTaskRealtime } from './useSessionWorkbarTaskRealtime';
import { useSessionWorkbarUiPromptRealtime } from './useSessionWorkbarUiPromptRealtime';
import {
  type OpenWorkbarHistoryOptions,
  type UseSessionWorkbarPanelsArgs,
} from './useSessionWorkbarPanels.types';
import { useTaskRealtimeMutationGuard } from './useTaskRealtimeMutationGuard';

export const useSessionWorkbarPanels = ({
  apiClient,
  session,
  enabled = true,
  messages,
  selectedSessionActiveTurnId = null,
  taskHistoryOpen = false,
  uiPromptHistoryOpen = false,
  sessionRuntimeGuidanceState,
  taskReviewPanelsBySession,
  uiPromptPanelsBySession,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
  loadWorkbarSummaries,
  loadUiPromptHistory,
  markUiPromptHistoryStale,
}: UseSessionWorkbarPanelsArgs) => {
  const sessionId = session?.id || null;
  const realtimeConnectionState = useRealtimeConnectionState();
  const preferRealtimeSync = enabled
    && Boolean(sessionId)
    && realtimeConnectionState === 'connected';
  const normalizedSelectedTurnId = typeof selectedSessionActiveTurnId === 'string'
    ? selectedSessionActiveTurnId.trim()
    : '';
  const {
    markTaskRealtimeMutationHandled,
    consumeRecentTaskRealtimeMutation,
  } = useTaskRealtimeMutationGuard();

  const activeTaskReviewPanel = useMemo(
    () => pickFirstSessionPanel<TaskReviewPanelState>(taskReviewPanelsBySession, sessionId),
    [sessionId, taskReviewPanelsBySession],
  );

  const activeUiPromptPanel = useMemo(
    () => pickFirstSessionPanel<UiPromptPanelState>(uiPromptPanelsBySession, sessionId),
    [sessionId, uiPromptPanelsBySession],
  );

  const runtimeGuidanceState = useMemo(
    () => pickSessionScopedState<SessionRuntimeGuidanceState>(sessionRuntimeGuidanceState, sessionId),
    [sessionId, sessionRuntimeGuidanceState],
  );

  const {
    activeConversationTurnId,
    mergedCurrentTurnTasks,
    workbarHistoryTasks,
    workbarLoading,
    workbarHistoryLoading,
    workbarError,
    workbarHistoryError,
    setWorkbarError,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    patchCurrentTurnWorkbarTask,
    removeCurrentTurnWorkbarTask,
    patchHistoryWorkbarTask,
    removeHistoryWorkbarTask,
    refreshWorkbarTasks,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
  } = useWorkbarState({
    apiClient,
    currentSession: enabled ? session : null,
    messages,
  });

  const preferredTurnId = normalizedSelectedTurnId || activeConversationTurnId || null;

  useEffect(() => {
    if (!enabled || !sessionId) {
      resetAllWorkbarState();
      return;
    }
    void loadCurrentTurnWorkbarTasks(sessionId, preferredTurnId);
  }, [
    enabled,
    loadCurrentTurnWorkbarTasks,
    preferredTurnId,
    resetAllWorkbarState,
    sessionId,
  ]);

  usePendingWorkbarPanels({
    apiClient,
    enabled,
    sessionId,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
  });

  const {
    workbarActionLoadingTaskId,
    taskModalError,
    taskModalMode,
    taskModalOpen,
    taskModalTask,
    closeTaskModal,
    submitTaskModal,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
  } = useWorkbarMutations({
    apiClient,
    currentSessionId: enabled ? (sessionId ?? null) : null,
    currentConversationTurnId: preferredTurnId,
    preferRealtimeSync,
    taskHistoryOpen,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    patchCurrentTurnWorkbarTask,
    removeCurrentTurnWorkbarTask,
    patchHistoryWorkbarTask,
    removeHistoryWorkbarTask,
    markTaskRealtimeMutationHandled,
    setWorkbarError,
  });

  const loadUiPromptHistorySafe = useCallback(async (targetSessionId: string, force = false) => {
    if (!loadUiPromptHistory) {
      return;
    }
    await loadUiPromptHistory(targetSessionId, force);
  }, [loadUiPromptHistory]);

  const {
    handleTaskReviewConfirm,
    handleTaskReviewCancel,
    handleUiPromptSubmit,
    handleUiPromptCancel,
  } = usePanelActions({
    activeTaskReviewPanel,
    activeUiPromptPanel,
    apiClient,
    preferRealtimeSync,
    taskHistoryOpen,
    uiPromptHistoryOpen,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    loadUiPromptHistory: loadUiPromptHistorySafe,
    markUiPromptHistoryStale,
    removePendingTaskReviewCachePanel: (reviewId: string, targetSessionId?: string) => {
      removePendingTaskReviewCachePanel(apiClient, reviewId, targetSessionId);
    },
    removePendingUiPromptCachePanel: (promptId: string, targetSessionId?: string) => {
      removePendingUiPromptCachePanel(apiClient, promptId, targetSessionId);
    },
  });

  const handleRefreshWorkbar = useCallback(() => {
    void refreshWorkbarTasks();
  }, [refreshWorkbarTasks]);

  const handleOpenWorkbarHistory = useCallback((
    targetSessionId: string,
    options: OpenWorkbarHistoryOptions = {},
  ) => {
    if (!targetSessionId) {
      return;
    }
    const forceHistory = options.forceHistory === true;
    const forceSummaries = options.forceSummaries !== false;
    void loadHistoryWorkbarTasks(targetSessionId, forceHistory);
    if (forceSummaries) {
      void loadWorkbarSummaries(targetSessionId, true);
    }
  }, [loadHistoryWorkbarTasks, loadWorkbarSummaries]);

  const runtimeGuidancePendingCount = Number(runtimeGuidanceState?.pendingCount || 0);
  const runtimeGuidanceAppliedCount = Number(runtimeGuidanceState?.appliedCount || 0);
  const runtimeGuidanceLastAppliedAt = runtimeGuidanceState?.lastAppliedAt || null;
  const runtimeGuidanceItems = useMemo<RuntimeGuidanceWorkbarItem[]>(() => (
    Array.isArray(runtimeGuidanceState?.items)
      ? (runtimeGuidanceState.items as RuntimeGuidanceWorkbarItem[])
      : []
  ), [runtimeGuidanceState?.items]);

  useSessionWorkbarTaskRealtime({
    apiClient,
    enabled,
    sessionId,
    preferredTurnId,
    preferRealtimeSync,
    taskHistoryOpen,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    patchCurrentTurnWorkbarTask,
    removeCurrentTurnWorkbarTask,
    patchHistoryWorkbarTask,
    removeHistoryWorkbarTask,
    consumeRecentTaskRealtimeMutation,
  });

  useSessionWorkbarUiPromptRealtime({
    apiClient,
    enabled,
    sessionId,
    preferRealtimeSync,
    uiPromptHistoryOpen,
    loadUiPromptHistory,
    markUiPromptHistoryStale,
    upsertUiPromptPanel,
    removeUiPromptPanel,
  });

  return {
    activeConversationTurnId,
    activeTaskReviewPanel,
    activeUiPromptPanel,
    handleOpenWorkbarHistory,
    handleRefreshWorkbar,
    handleTaskReviewCancel,
    handleTaskReviewConfirm,
    handleUiPromptCancel,
    handleUiPromptSubmit,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    mergedCurrentTurnTasks,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    runtimeGuidanceAppliedCount,
    runtimeGuidanceItems,
    runtimeGuidanceLastAppliedAt,
    runtimeGuidancePendingCount,
    runtimeGuidanceState,
    taskModalError,
    taskModalMode,
    taskModalOpen,
    taskModalTask,
    closeTaskModal,
    submitTaskModal,
    workbarActionLoadingTaskId,
    workbarError,
    workbarHistoryError,
    workbarHistoryLoading,
    workbarHistoryTasks,
    workbarLoading,
  };
};

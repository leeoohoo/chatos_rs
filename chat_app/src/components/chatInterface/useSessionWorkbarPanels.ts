import { useCallback, useEffect, useMemo } from 'react';

import type {
  SessionRuntimeGuidanceState,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';
import type { RuntimeGuidanceWorkbarItem } from '../TaskWorkbar';
import { pickFirstSessionPanel, pickSessionScopedState } from './helpers';
import { usePanelActions } from './usePanelActions';
import { useWorkbarMutations } from './useWorkbarMutations';
import { useWorkbarState } from './useWorkbarState';
import type { Message } from '../../types';

interface SessionWorkbarApiClient {
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
  completeTaskManagerTask: (sessionId: string, taskId: string) => Promise<unknown>;
  deleteTaskManagerTask: (sessionId: string, taskId: string) => Promise<unknown>;
  updateTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload: {
      title?: string;
      details?: string;
      priority?: 'high' | 'medium' | 'low';
      status?: 'pending_confirm' | 'pending_execute' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
      due_at?: string | null;
    },
  ) => Promise<unknown>;
  submitTaskReviewDecision: (
    reviewId: string,
    payload: {
      action: 'confirm' | 'cancel';
      tasks?: Array<{
        title: string;
        details: string;
        priority: 'high' | 'medium' | 'low';
        status: 'pending_confirm' | 'pending_execute' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
        tags: string[];
        due_at?: string | null;
      }>;
      reason?: string;
    },
  ) => Promise<unknown>;
  submitImActionRequest: (
    actionRequestId: string,
    payload: unknown,
  ) => Promise<unknown>;
  submitUiPromptResponse: (
    promptId: string,
    payload: {
      status: 'ok' | 'canceled';
      values?: Record<string, string>;
      selection?: string | string[];
      reason?: string;
    },
  ) => Promise<unknown>;
}

interface SessionLike {
  id: string;
}

interface UseSessionWorkbarPanelsArgs {
  apiClient: SessionWorkbarApiClient;
  session: SessionLike | null;
  enabled?: boolean;
  messages: Message[];
  selectedSessionActiveTurnId?: string | null;
  sessionRuntimeGuidanceState: Record<string, SessionRuntimeGuidanceState | undefined>;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[] | undefined>;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[] | undefined>;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
  loadWorkbarSummaries: (sessionId: string, force?: boolean) => Promise<void>;
  loadUiPromptHistory?: (sessionId: string, force?: boolean) => Promise<void>;
}

interface OpenWorkbarHistoryOptions {
  forceHistory?: boolean;
  forceSummaries?: boolean;
}

export const useSessionWorkbarPanels = ({
  apiClient,
  session,
  enabled = true,
  messages,
  selectedSessionActiveTurnId = null,
  sessionRuntimeGuidanceState,
  taskReviewPanelsBySession,
  uiPromptPanelsBySession,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
  loadWorkbarSummaries,
  loadUiPromptHistory,
}: UseSessionWorkbarPanelsArgs) => {
  const sessionId = session?.id || null;
  const normalizedSelectedTurnId = typeof selectedSessionActiveTurnId === 'string'
    ? selectedSessionActiveTurnId.trim()
    : '';

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
    void loadHistoryWorkbarTasks(sessionId);
  }, [
    enabled,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    preferredTurnId,
    resetAllWorkbarState,
    sessionId,
  ]);

  const {
    workbarActionLoadingTaskId,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
  } = useWorkbarMutations({
    apiClient,
    currentSessionId: enabled ? (sessionId ?? null) : null,
    refreshWorkbarTasks,
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
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    loadWorkbarSummaries,
    loadUiPromptHistory: loadUiPromptHistorySafe,
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
    workbarActionLoadingTaskId,
    workbarError,
    workbarHistoryError,
    workbarHistoryLoading,
    workbarHistoryTasks,
    workbarLoading,
  };
};

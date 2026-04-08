import { useCallback, useEffect, useMemo, useRef } from 'react';

import type {
  SessionRuntimeGuidanceState,
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';
import { readSessionImConversationId, resolveRuntimeSessionId } from '../../lib/store/helpers/sessionRuntime';
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
      status?: 'pending_confirm' | 'pending_execute' | 'running' | 'paused' | 'blocked' | 'completed' | 'failed' | 'cancelled' | 'skipped';
      due_at?: string | null;
    },
  ) => Promise<unknown>;
  confirmTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: { note?: string },
  ) => Promise<unknown>;
  pauseTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: { reason?: string },
  ) => Promise<unknown>;
  resumeTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: { note?: string },
  ) => Promise<unknown>;
  retryTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: { note?: string },
  ) => Promise<unknown>;
  submitTaskReviewDecision: (
    reviewId: string,
    payload: {
      action: 'confirm' | 'cancel';
      tasks?: Array<{
        title: string;
        details: string;
        priority: 'high' | 'medium' | 'low';
        status: TaskReviewDraft['status'];
        tags: string[];
        due_at?: string | null;
        task_ref?: string | null;
        task_kind?: string | null;
        depends_on_refs?: string[];
        verification_of_refs?: string[];
        acceptance_criteria?: string[];
        planned_builtin_mcp_ids?: string[];
        planned_context_assets?: Array<{
          asset_type: string;
          asset_id: string;
          display_name?: string | null;
          source_type?: string | null;
          source_path?: string | null;
        }>;
        execution_result_contract?: {
          result_required: boolean;
          preferred_format?: string | null;
        } | null;
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
  metadata?: unknown;
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
  const isImSession = useMemo(
    () => Boolean(readSessionImConversationId(session?.metadata)),
    [session?.metadata],
  );
  const runtimeSessionIdForWorkbar = useMemo(() => {
    return resolveRuntimeSessionId({
      sessionId,
      metadata: session?.metadata,
      messages,
    });
  }, [messages, session?.metadata, sessionId]);
  const normalizedSelectedTurnId = typeof selectedSessionActiveTurnId === 'string'
    ? selectedSessionActiveTurnId.trim()
    : '';
  const latestImMessageRefreshKeyRef = useRef<string | null>(null);

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
    currentSession: (enabled && runtimeSessionIdForWorkbar)
      ? {
        id: runtimeSessionIdForWorkbar,
      }
      : null,
    messages,
  });

  const preferredTurnId = normalizedSelectedTurnId || activeConversationTurnId || null;
  const latestImMessageRefreshKey = useMemo(() => {
    if (!enabled || !runtimeSessionIdForWorkbar || !isImSession) {
      return '';
    }

    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i];
      if (message?.sessionId && message.sessionId !== runtimeSessionIdForWorkbar) {
        continue;
      }
      const messageId = typeof message?.id === 'string' ? message.id.trim() : '';
      const updatedAt = message?.updatedAt instanceof Date
        ? message.updatedAt.toISOString()
        : '';
      const createdAt = message?.createdAt instanceof Date
        ? message.createdAt.toISOString()
        : '';
      const sender = typeof message?.role === 'string' ? message.role.trim() : '';
      const turnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      const effectiveTime = updatedAt || createdAt;
      if (messageId || effectiveTime || sender || turnId) {
        return [messageId, effectiveTime, sender, turnId].join('|');
      }
    }

    return '';
  }, [enabled, isImSession, messages, runtimeSessionIdForWorkbar]);

  useEffect(() => {
    if (!enabled || !runtimeSessionIdForWorkbar) {
      resetAllWorkbarState();
      return;
    }
    void loadCurrentTurnWorkbarTasks(runtimeSessionIdForWorkbar, preferredTurnId);
    void loadHistoryWorkbarTasks(runtimeSessionIdForWorkbar);
  }, [
    enabled,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    preferredTurnId,
    resetAllWorkbarState,
    runtimeSessionIdForWorkbar,
  ]);

  useEffect(() => {
    if (!enabled || !runtimeSessionIdForWorkbar || !isImSession) {
      latestImMessageRefreshKeyRef.current = null;
      return;
    }

    if (!latestImMessageRefreshKey) {
      return;
    }

    if (latestImMessageRefreshKeyRef.current === null) {
      latestImMessageRefreshKeyRef.current = latestImMessageRefreshKey;
      return;
    }

    if (latestImMessageRefreshKeyRef.current === latestImMessageRefreshKey) {
      return;
    }

    latestImMessageRefreshKeyRef.current = latestImMessageRefreshKey;
    void loadCurrentTurnWorkbarTasks(runtimeSessionIdForWorkbar, preferredTurnId);
    void loadHistoryWorkbarTasks(runtimeSessionIdForWorkbar, true);
  }, [
    enabled,
    isImSession,
    latestImMessageRefreshKey,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    preferredTurnId,
    runtimeSessionIdForWorkbar,
  ]);

  const {
    workbarActionLoadingTaskId,
    handleWorkbarConfirmTask,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    handleWorkbarPauseTask,
    handleWorkbarResumeTask,
    handleWorkbarRetryTask,
  } = useWorkbarMutations({
    apiClient,
    currentSessionId: enabled ? (runtimeSessionIdForWorkbar ?? null) : null,
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
    handleWorkbarConfirmTask,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    handleWorkbarPauseTask,
    handleWorkbarResumeTask,
    handleWorkbarRetryTask,
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

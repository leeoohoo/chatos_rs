import { useCallback, useEffect, useMemo, useRef } from 'react';

import type {
  SessionRuntimeGuidanceState,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';
import { useRealtimeConnectionState } from '../../lib/realtime/RealtimeProvider';
import { useConversationTaskBoardRealtime } from '../../lib/realtime/useConversationTaskBoardRealtime';
import { useConversationUiPromptRealtime } from '../../lib/realtime/useConversationUiPromptRealtime';
import type { RuntimeGuidanceWorkbarItem } from '../TaskWorkbar';
import {
  pickFirstSessionPanel,
  pickSessionScopedState,
  normalizeWorkbarTask,
  syncTaskReviewPanelsSnapshot,
  syncUiPromptPanelsSnapshot,
  toTaskReviewPanelFromRealtimePayload,
  toUiPromptPanelFromRealtimePayload,
} from './helpers';
import {
  loadPendingTaskReviewPanels,
  peekPendingTaskReviewCacheEntry,
  removePendingTaskReviewCachePanel,
  upsertPendingTaskReviewCachePanel,
} from './pendingTaskReviewCache';
import {
  loadPendingUiPromptPanels,
  peekPendingUiPromptCacheEntry,
  removePendingUiPromptCachePanel,
  upsertPendingUiPromptCachePanel,
} from './pendingUiPromptCache';
import { usePanelActions } from './usePanelActions';
import { useWorkbarMutations } from './useWorkbarMutations';
import { useWorkbarState } from './useWorkbarState';
import type { Message } from '../../types';
import type { TaskManagerTaskResponse } from '../../lib/api/client/types/runtime';

interface SessionWorkbarApiClient {
  getPendingTaskReviews: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
  getPendingUiPrompts: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
  completeTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: {
      outcome_summary?: string;
      resume_hint?: string;
    },
  ) => Promise<TaskManagerTaskResponse>;
  deleteTaskManagerTask: (sessionId: string, taskId: string) => Promise<{ success?: boolean }>;
  updateTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload: {
      title?: string;
      details?: string;
      priority?: 'high' | 'medium' | 'low';
      status?: 'todo' | 'doing' | 'blocked' | 'done';
      due_at?: string | null;
      outcome_summary?: string;
      resume_hint?: string;
      blocker_reason?: string;
      blocker_needs?: string[];
      blocker_kind?: string;
    },
  ) => Promise<TaskManagerTaskResponse>;
  submitTaskReviewDecision: (
    reviewId: string,
    payload: {
      action: 'confirm' | 'cancel';
      tasks?: Array<{
        title: string;
        details: string;
        priority: 'high' | 'medium' | 'low';
        status: 'todo' | 'doing' | 'blocked' | 'done';
        tags: string[];
        due_at?: string | null;
      }>;
      reason?: string;
    },
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
  taskHistoryOpen?: boolean;
  uiPromptHistoryOpen?: boolean;
  sessionRuntimeGuidanceState: Record<string, SessionRuntimeGuidanceState | undefined>;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[] | undefined>;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[] | undefined>;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
  loadWorkbarSummaries: (sessionId: string, force?: boolean) => Promise<void>;
  loadUiPromptHistory?: (sessionId: string, force?: boolean) => Promise<void>;
  markUiPromptHistoryStale?: (sessionId: string) => void;
}

interface OpenWorkbarHistoryOptions {
  forceHistory?: boolean;
  forceSummaries?: boolean;
}

interface TaskRealtimeMutationGuardPayload {
  action: string;
  taskId?: string | null;
  turnId?: string | null;
}

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
  const taskRealtimeInflightRef = useRef(false);
  const taskRealtimeQueuedRef = useRef<{
    targetSessionId: string;
    conversationTurnId?: string | null;
    forceHistory: boolean;
  } | null>(null);
  const taskRealtimeReloadKeyRef = useRef('');
  const taskRealtimeMutationGuardRef = useRef<Map<string, number>>(new Map());
  const uiPromptRealtimeInflightRef = useRef(false);
  const uiPromptRealtimeQueuedRef = useRef<{
    targetSessionId: string;
    force: boolean;
  } | null>(null);
  const uiPromptRealtimeReloadKeyRef = useRef('');
  const pendingTaskReviewLoadSeqRef = useRef(0);
  const pendingUiPromptLoadSeqRef = useRef(0);

  const markTaskRealtimeMutationHandled = useCallback((payload: TaskRealtimeMutationGuardPayload) => {
    const action = String(payload.action || '').trim();
    const taskId = String(payload.taskId || '').trim();
    const turnId = String(payload.turnId || '').trim();
    if (!action || !taskId) {
      return;
    }
    taskRealtimeMutationGuardRef.current.set(
      `${action}:${taskId}:${turnId}`,
      Date.now(),
    );
  }, []);

  const consumeRecentTaskRealtimeMutation = useCallback((payload: TaskRealtimeMutationGuardPayload): boolean => {
    const action = String(payload.action || '').trim();
    const taskId = String(payload.taskId || '').trim();
    const turnId = String(payload.turnId || '').trim();
    if (!action || !taskId) {
      return false;
    }
    const key = `${action}:${taskId}:${turnId}`;
    const seenAt = taskRealtimeMutationGuardRef.current.get(key);
    if (!seenAt) {
      return false;
    }
    if (Date.now() - seenAt > 4000) {
      taskRealtimeMutationGuardRef.current.delete(key);
      return false;
    }
    taskRealtimeMutationGuardRef.current.delete(key);
    return true;
  }, []);

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

  useEffect(() => {
    if (!enabled || !sessionId) {
      return;
    }

    const cached = peekPendingTaskReviewCacheEntry(apiClient, sessionId);
    if (cached && !cached.stale) {
      syncTaskReviewPanelsSnapshot({
        sessionId,
        panels: cached.panels,
        existingPanels: taskReviewPanelsBySession?.[sessionId],
        upsertTaskReviewPanel,
        removeTaskReviewPanel,
      });
      return;
    }

    const requestSeq = pendingTaskReviewLoadSeqRef.current + 1;
    pendingTaskReviewLoadSeqRef.current = requestSeq;
    let cancelled = false;
    const inflight = loadPendingTaskReviewPanels(apiClient, sessionId, { limit: 50 });

    void inflight
      .then((panels) => {
        if (cancelled || pendingTaskReviewLoadSeqRef.current !== requestSeq) {
          return;
        }
        syncTaskReviewPanelsSnapshot({
          sessionId,
          panels,
          existingPanels: taskReviewPanelsBySession?.[sessionId],
          upsertTaskReviewPanel,
          removeTaskReviewPanel,
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [
    apiClient,
    enabled,
    removeTaskReviewPanel,
    sessionId,
    taskReviewPanelsBySession,
    upsertTaskReviewPanel,
  ]);

  useEffect(() => {
    if (!enabled || !sessionId) {
      return;
    }

    const cached = peekPendingUiPromptCacheEntry(apiClient, sessionId);
    if (cached && !cached.stale) {
      syncUiPromptPanelsSnapshot({
        sessionId,
        panels: cached.panels,
        existingPanels: uiPromptPanelsBySession?.[sessionId],
        upsertUiPromptPanel,
        removeUiPromptPanel,
      });
      return;
    }

    const requestSeq = pendingUiPromptLoadSeqRef.current + 1;
    pendingUiPromptLoadSeqRef.current = requestSeq;
    let cancelled = false;
    const inflight = loadPendingUiPromptPanels(apiClient, sessionId, { limit: 50 });

    void inflight
      .then((panels) => {
        if (cancelled || pendingUiPromptLoadSeqRef.current !== requestSeq) {
          return;
        }
        syncUiPromptPanelsSnapshot({
          sessionId,
          panels,
          existingPanels: uiPromptPanelsBySession?.[sessionId],
          upsertUiPromptPanel,
          removeUiPromptPanel,
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [
    apiClient,
    enabled,
    removeUiPromptPanel,
    sessionId,
    uiPromptPanelsBySession,
    upsertUiPromptPanel,
  ]);

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

  const reloadTaskBoardFromRealtime = useCallback(async (
    targetSessionId: string,
    conversationTurnId?: string | null,
    forceHistory = true,
  ) => {
    if (!targetSessionId) {
      return;
    }
    if (taskRealtimeInflightRef.current) {
      taskRealtimeQueuedRef.current = {
        targetSessionId,
        conversationTurnId,
        forceHistory,
      };
      return;
    }
    taskRealtimeInflightRef.current = true;
    try {
      const reloads: Promise<void>[] = [
        loadCurrentTurnWorkbarTasks(targetSessionId, conversationTurnId, true),
      ];
      if (forceHistory) {
        reloads.push(loadHistoryWorkbarTasks(targetSessionId, true));
      } else {
        markHistoryWorkbarTasksStale(targetSessionId);
      }
      await Promise.all(reloads);
    } finally {
      window.setTimeout(() => {
        taskRealtimeInflightRef.current = false;
        const queued = taskRealtimeQueuedRef.current;
        if (queued) {
          taskRealtimeQueuedRef.current = null;
          void reloadTaskBoardFromRealtime(
            queued.targetSessionId,
            queued.conversationTurnId,
            queued.forceHistory,
          );
        }
      }, 250);
    }
  }, [loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks, markHistoryWorkbarTasksStale]);

  const reloadUiPromptHistoryFromRealtime = useCallback(async (
    targetSessionId: string,
    force = true,
  ) => {
    if (!loadUiPromptHistory || !targetSessionId) {
      return;
    }
    if (uiPromptRealtimeInflightRef.current) {
      uiPromptRealtimeQueuedRef.current = {
        targetSessionId,
        force,
      };
      return;
    }
    uiPromptRealtimeInflightRef.current = true;
    try {
      if (force) {
        await loadUiPromptHistory(targetSessionId, true);
      } else {
        markUiPromptHistoryStale?.(targetSessionId);
      }
    } finally {
      window.setTimeout(() => {
        uiPromptRealtimeInflightRef.current = false;
        const queued = uiPromptRealtimeQueuedRef.current;
        if (queued) {
          uiPromptRealtimeQueuedRef.current = null;
          void reloadUiPromptHistoryFromRealtime(queued.targetSessionId, queued.force);
        }
      }, 250);
    }
  }, [loadUiPromptHistory, markUiPromptHistoryStale]);

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

  useConversationTaskBoardRealtime({
    sessionId,
    enabled: enabled && Boolean(sessionId),
    onEvent: async (payload) => {
      if (payload.action === 'review_required') {
        const panel = toTaskReviewPanelFromRealtimePayload(payload);
        if (panel) {
          upsertPendingTaskReviewCachePanel(apiClient, panel);
          upsertTaskReviewPanel(panel);
        }
      }

      if (
        payload.action === 'review_confirmed'
        || payload.action === 'review_cancelled'
      ) {
        const reviewId = typeof payload.review_id === 'string' ? payload.review_id.trim() : '';
        if (reviewId) {
          removePendingTaskReviewCachePanel(apiClient, reviewId, sessionId || undefined);
          removeTaskReviewPanel(reviewId, sessionId || undefined);
        }
      }

      if (
        payload.action !== 'task_created'
        && payload.action !== 'task_updated'
        && payload.action !== 'task_deleted'
      ) {
        return;
      }

      const payloadTurnId = typeof payload.conversation_turn_id === 'string'
        ? payload.conversation_turn_id.trim()
        : '';
      const payloadTask = payload.task ? normalizeWorkbarTask(payload.task) : null;
      const payloadTaskId = typeof payload.task_id === 'string'
        ? payload.task_id.trim()
        : (payloadTask?.id || '');
      const reloadTurnId = payloadTurnId || preferredTurnId || null;
      const guardTurnId = payloadTurnId || (payloadTask?.conversationTurnId || '') || '';

      if (consumeRecentTaskRealtimeMutation({
        action: payload.action,
        taskId: payloadTaskId,
        turnId: guardTurnId,
      })) {
        return;
      }

      const reloadKey = [
        payload.action,
        payloadTaskId,
        payload.review_id || '',
        reloadTurnId || '',
      ].join(':');
      if (taskRealtimeReloadKeyRef.current === reloadKey) {
        return;
      }
      taskRealtimeReloadKeyRef.current = reloadKey;
      window.setTimeout(() => {
        if (taskRealtimeReloadKeyRef.current === reloadKey) {
          taskRealtimeReloadKeyRef.current = '';
        }
      }, 300);

      if (payload.action === 'task_deleted' && sessionId) {
        const deletedTaskId = payloadTaskId;
        if (deletedTaskId) {
          const currentPatched = removeCurrentTurnWorkbarTask(sessionId, deletedTaskId);
          if (taskHistoryOpen) {
            const historyPatched = removeHistoryWorkbarTask(sessionId, deletedTaskId);
            if (currentPatched || historyPatched) {
              return;
            }
          } else if (currentPatched) {
            markHistoryWorkbarTasksStale(sessionId);
            return;
          }
        }
      } else if (payloadTask && sessionId) {
        const currentPatched = patchCurrentTurnWorkbarTask(sessionId, payloadTask);
        if (taskHistoryOpen) {
          const historyPatched = patchHistoryWorkbarTask(sessionId, payloadTask);
          if (currentPatched || historyPatched) {
            return;
          }
        } else if (currentPatched) {
          markHistoryWorkbarTasksStale(sessionId);
          return;
        }
      }

      if (preferRealtimeSync && sessionId) {
        await reloadTaskBoardFromRealtime(sessionId, reloadTurnId, taskHistoryOpen);
        return;
      }

      if (!sessionId) {
        return;
      }
      await loadCurrentTurnWorkbarTasks(sessionId, reloadTurnId, true);
      if (taskHistoryOpen) {
        await loadHistoryWorkbarTasks(sessionId, true);
      } else {
        markHistoryWorkbarTasksStale(sessionId);
      }
    },
  });

  useConversationUiPromptRealtime({
    sessionId,
    enabled: enabled && Boolean(sessionId),
    onEvent: async (payload) => {
      if (payload.action === 'prompt_required') {
        const panel = toUiPromptPanelFromRealtimePayload(payload);
        if (panel) {
          upsertPendingUiPromptCachePanel(apiClient, panel);
          upsertUiPromptPanel(panel);
        }
      }

      if (payload.action === 'prompt_resolved') {
        const promptId = typeof payload.prompt_id === 'string' ? payload.prompt_id.trim() : '';
        if (promptId) {
          removePendingUiPromptCachePanel(apiClient, promptId, sessionId || undefined);
          removeUiPromptPanel(promptId, sessionId || undefined);
        }
      }

      if (loadUiPromptHistory && sessionId) {
        const reloadKey = [
          payload.action,
          payload.prompt_id || '',
          payload.status || '',
        ].join(':');
        if (uiPromptRealtimeReloadKeyRef.current === reloadKey) {
          return;
        }
        uiPromptRealtimeReloadKeyRef.current = reloadKey;
        window.setTimeout(() => {
          if (uiPromptRealtimeReloadKeyRef.current === reloadKey) {
            uiPromptRealtimeReloadKeyRef.current = '';
          }
        }, 300);

        if (preferRealtimeSync) {
          await reloadUiPromptHistoryFromRealtime(sessionId, uiPromptHistoryOpen);
          return;
        }
        if (uiPromptHistoryOpen) {
          await loadUiPromptHistory(sessionId, true);
        } else {
          markUiPromptHistoryStale?.(sessionId);
        }
      }
    },
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

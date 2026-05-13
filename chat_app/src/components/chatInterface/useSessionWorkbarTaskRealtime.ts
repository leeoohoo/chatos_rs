import { useCallback, useEffect, useRef } from 'react';

import type { RealtimeTaskBoardPayloadWrapper } from '../../lib/realtime/types';
import { useRealtimeInvalidationQueue } from '../../lib/realtime/invalidationQueue';
import { useConversationTaskBoardRealtime } from '../../lib/realtime/useConversationTaskBoardRealtime';
import { toTaskReviewPanelFromRealtimePayload } from './panelTransforms';
import { normalizeWorkbarTask } from './workbarTransforms';
import {
  removePendingTaskReviewCachePanel,
  upsertPendingTaskReviewCachePanel,
} from './pendingTaskReviewCache';
import type {
  SessionWorkbarApiClient,
  TaskRealtimeMutationGuardPayload,
} from './useSessionWorkbarPanels.types';
import type { TaskReviewPanelState } from '../../lib/store/types';
import type { TaskWorkbarItem } from '../TaskWorkbar';

interface UseSessionWorkbarTaskRealtimeArgs {
  apiClient: SessionWorkbarApiClient;
  enabled: boolean;
  sessionId: string | null;
  preferredTurnId: string | null;
  preferRealtimeSync: boolean;
  taskHistoryOpen: boolean;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  loadCurrentTurnWorkbarTasks: (
    sessionId: string,
    conversationTurnId?: string | null,
    force?: boolean,
  ) => Promise<void>;
  loadHistoryWorkbarTasks: (sessionId: string, force?: boolean) => Promise<void>;
  markHistoryWorkbarTasksStale: (sessionId: string) => void;
  patchCurrentTurnWorkbarTask: (sessionId: string, task: TaskWorkbarItem) => boolean;
  removeCurrentTurnWorkbarTask: (sessionId: string, taskId: string) => boolean;
  patchHistoryWorkbarTask: (sessionId: string, task: TaskWorkbarItem) => boolean;
  removeHistoryWorkbarTask: (sessionId: string, taskId: string) => boolean;
  consumeRecentTaskRealtimeMutation: (payload: TaskRealtimeMutationGuardPayload) => boolean;
}

export const useSessionWorkbarTaskRealtime = ({
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
}: UseSessionWorkbarTaskRealtimeArgs) => {
  const reloadKeyRef = useRef('');
  const sessionIdRef = useRef(sessionId);

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  const reloadTaskBoard = useCallback(async (
    payload: RealtimeTaskBoardPayloadWrapper,
    currentSessionId: string,
  ) => {
    const payloadTurnId = typeof payload.conversation_turn_id === 'string'
      ? payload.conversation_turn_id.trim()
      : '';
    const reloadTurnId = payloadTurnId || preferredTurnId || null;

    if (preferRealtimeSync) {
      await loadCurrentTurnWorkbarTasks(currentSessionId, reloadTurnId, true);
      if (taskHistoryOpen) {
        await loadHistoryWorkbarTasks(currentSessionId, true);
      } else {
        markHistoryWorkbarTasksStale(currentSessionId);
      }
      return;
    }

    await loadCurrentTurnWorkbarTasks(currentSessionId, reloadTurnId, true);
    if (taskHistoryOpen) {
      await loadHistoryWorkbarTasks(currentSessionId, true);
    } else {
      markHistoryWorkbarTasksStale(currentSessionId);
    }
  }, [
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    preferRealtimeSync,
    preferredTurnId,
    taskHistoryOpen,
  ]);

  const reloadTaskBoardRef = useRef(reloadTaskBoard);

  useEffect(() => {
    reloadTaskBoardRef.current = reloadTaskBoard;
  }, [reloadTaskBoard]);

  const reloadQueue = useRealtimeInvalidationQueue<RealtimeTaskBoardPayloadWrapper>({
    onExecute: async (payload) => {
      const currentSessionId = sessionIdRef.current;
      if (!currentSessionId) {
        return;
      }
      await reloadTaskBoardRef.current(payload, currentSessionId);
    },
  });

  const handlePatchedTask = useCallback((
    currentSessionId: string,
    payloadTask: TaskWorkbarItem,
  ): boolean => {
    const currentPatched = patchCurrentTurnWorkbarTask(currentSessionId, payloadTask);
    if (taskHistoryOpen) {
      const historyPatched = patchHistoryWorkbarTask(currentSessionId, payloadTask);
      return currentPatched || historyPatched;
    }
    if (currentPatched) {
      markHistoryWorkbarTasksStale(currentSessionId);
      return true;
    }
    return false;
  }, [
    markHistoryWorkbarTasksStale,
    patchCurrentTurnWorkbarTask,
    patchHistoryWorkbarTask,
    taskHistoryOpen,
  ]);

  const handleRemovedTask = useCallback((
    currentSessionId: string,
    taskId: string,
  ): boolean => {
    const currentPatched = removeCurrentTurnWorkbarTask(currentSessionId, taskId);
    if (taskHistoryOpen) {
      const historyPatched = removeHistoryWorkbarTask(currentSessionId, taskId);
      return currentPatched || historyPatched;
    }
    if (currentPatched) {
      markHistoryWorkbarTasksStale(currentSessionId);
      return true;
    }
    return false;
  }, [
    markHistoryWorkbarTasksStale,
    removeCurrentTurnWorkbarTask,
    removeHistoryWorkbarTask,
    taskHistoryOpen,
  ]);

  const handleEvent = useCallback(async (payload: RealtimeTaskBoardPayloadWrapper) => {
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
    if (reloadKeyRef.current === reloadKey) {
      return;
    }
    reloadKeyRef.current = reloadKey;
    window.setTimeout(() => {
      if (reloadKeyRef.current === reloadKey) {
        reloadKeyRef.current = '';
      }
    }, 300);

    if (!sessionId) {
      return;
    }

    if (payload.action === 'task_deleted' && payloadTaskId) {
      if (handleRemovedTask(sessionId, payloadTaskId)) {
        return;
      }
    } else if (payloadTask && handlePatchedTask(sessionId, payloadTask)) {
      return;
    }

    reloadQueue.run(payload);
  }, [
    apiClient,
    consumeRecentTaskRealtimeMutation,
    handlePatchedTask,
    handleRemovedTask,
    preferredTurnId,
    reloadQueue,
    removeTaskReviewPanel,
    sessionId,
    upsertTaskReviewPanel,
  ]);

  useConversationTaskBoardRealtime({
    sessionId,
    enabled: enabled && Boolean(sessionId),
    onEvent: handleEvent,
  });
};

import { useCallback, useEffect, useRef, useState } from 'react';

import type { TaskWorkbarItem } from '../TaskWorkbar';
import {
  normalizeWorkbarTask,
  selectLatestTurnTasks,
} from './workbarTransforms';
import {
  beginSessionLoadRequest,
  isSessionLoadRequestCurrent,
  runGuardedSessionLoad,
} from './sessionLoadGuard';
import {
  getWorkbarCurrentTurnInflight,
  getWorkbarHistoryInflight,
  markWorkbarHistoryStale,
  peekWorkbarCurrentTurnCacheEntry,
  peekWorkbarHistoryCacheEntry,
  removeWorkbarCurrentTurnCachedTask,
  removeWorkbarHistoryCachedTask,
  setWorkbarCurrentTurnCacheEntry,
  setWorkbarCurrentTurnInflight,
  setWorkbarHistoryCacheEntry,
  setWorkbarHistoryInflight,
  upsertWorkbarCurrentTurnCachedTask,
  upsertWorkbarHistoryCachedTask,
} from './workbarCache';

export interface WorkbarSessionLike {
  id: string;
}

export interface WorkbarStateApiClient {
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
}

interface UseWorkbarTaskResourceStateParams {
  apiClient: WorkbarStateApiClient;
  currentSession: WorkbarSessionLike | null;
  activeConversationTurnId: string | null;
}

export const useWorkbarTaskResourceState = ({
  apiClient,
  currentSession,
  activeConversationTurnId,
}: UseWorkbarTaskResourceStateParams) => {
  const [workbarCurrentTurnTasks, setWorkbarCurrentTurnTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarHistoryTasks, setWorkbarHistoryTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarHistoryLoadedSessionId, setWorkbarHistoryLoadedSessionId] = useState<string | null>(null);
  const [workbarLoading, setWorkbarLoading] = useState(false);
  const [workbarHistoryLoading, setWorkbarHistoryLoading] = useState(false);
  const [workbarError, setWorkbarError] = useState<string | null>(null);
  const [workbarHistoryError, setWorkbarHistoryError] = useState<string | null>(null);

  const currentSessionRef = useRef<string | null>(null);
  const currentTurnLoadSeqRef = useRef(0);
  const historyLoadSeqRef = useRef(0);
  const workbarHistoryStaleSessionsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    currentSessionRef.current = currentSession?.id || null;
  }, [currentSession?.id]);

  const loadCurrentTurnWorkbarTasks = useCallback(async (
    sessionId: string,
    conversationTurnId?: string | null,
    force = false,
  ) => {
    if (!sessionId) {
      setWorkbarCurrentTurnTasks([]);
      setWorkbarError(null);
      setWorkbarLoading(false);
      return;
    }

    const turnId = typeof conversationTurnId === 'string' ? conversationTurnId.trim() : '';
    const cached = peekWorkbarCurrentTurnCacheEntry(apiClient, sessionId, turnId || null);
    if (
      !force
      && cached
      && cached.turnId === (turnId || null)
      && !cached.stale
    ) {
      setWorkbarCurrentTurnTasks(cached.tasks);
      setWorkbarError(null);
      setWorkbarLoading(false);
      return;
    }

    const requestSeq = beginSessionLoadRequest(currentTurnLoadSeqRef);
    await runGuardedSessionLoad({
      applyResult: setWorkbarCurrentTurnTasks,
      errorMessage: '任务操作失败',
      load: () => {
        const existingInflight = force
          ? null
          : getWorkbarCurrentTurnInflight(apiClient, sessionId, turnId || null);
        if (existingInflight) {
          return existingInflight;
        }
        const inflight = (async () => {
          let normalizedTasks: TaskWorkbarItem[] = [];

          if (turnId) {
            const tasks = await apiClient.getTaskManagerTasks(sessionId, {
              conversationTurnId: turnId,
              includeDone: true,
              limit: 100,
            });
            normalizedTasks = tasks.map(normalizeWorkbarTask);
          }

          if (normalizedTasks.length === 0) {
            const fallbackTasks = await apiClient.getTaskManagerTasks(sessionId, {
              includeDone: true,
              limit: 200,
            });
            normalizedTasks = selectLatestTurnTasks(fallbackTasks.map(normalizeWorkbarTask));
          }

          setWorkbarCurrentTurnCacheEntry(apiClient, sessionId, {
            tasks: normalizedTasks,
            turnId: turnId || null,
          });
          return normalizedTasks;
        })()
          .finally(() => {
            setWorkbarCurrentTurnInflight(apiClient, sessionId, turnId || null, null);
          });
        setWorkbarCurrentTurnInflight(apiClient, sessionId, turnId || null, inflight);
        return inflight;
      },
      setError: setWorkbarError,
      setLoading: setWorkbarLoading,
      shouldApply: () => isSessionLoadRequestCurrent({
        currentSessionRef,
        requestSeq,
        requestSeqRef: currentTurnLoadSeqRef,
        sessionId,
      }),
    });
  }, [apiClient]);

  const loadHistoryWorkbarTasks = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarHistoryTasks([]);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      setWorkbarHistoryLoading(false);
      return;
    }

    const isStale = workbarHistoryStaleSessionsRef.current.has(sessionId);
    const cached = peekWorkbarHistoryCacheEntry(apiClient, sessionId);
    if (
      !force
      && !isStale
      && cached
      && !cached.stale
    ) {
      setWorkbarHistoryTasks(cached.tasks);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(sessionId);
      setWorkbarHistoryLoading(false);
      return;
    }

    const requestSeq = beginSessionLoadRequest(historyLoadSeqRef);
    await runGuardedSessionLoad({
      applyResult: (normalizedTasks) => {
        workbarHistoryStaleSessionsRef.current.delete(sessionId);
        setWorkbarHistoryTasks(normalizedTasks);
        setWorkbarHistoryLoadedSessionId(sessionId);
      },
      errorMessage: '任务加载失败',
      load: () => {
        const existingInflight = force
          ? null
          : getWorkbarHistoryInflight(apiClient, sessionId);
        if (existingInflight) {
          return existingInflight;
        }
        const inflight = apiClient.getTaskManagerTasks(sessionId, {
          includeDone: true,
          limit: 300,
        })
          .then((tasks) => tasks.map(normalizeWorkbarTask))
          .then((normalizedTasks) => {
            setWorkbarHistoryCacheEntry(apiClient, sessionId, normalizedTasks);
            return normalizedTasks;
          })
          .finally(() => {
            setWorkbarHistoryInflight(apiClient, sessionId, null);
          });
        setWorkbarHistoryInflight(apiClient, sessionId, inflight);
        return inflight;
      },
      setError: setWorkbarHistoryError,
      setLoading: setWorkbarHistoryLoading,
      shouldApply: () => isSessionLoadRequestCurrent({
        currentSessionRef,
        requestSeq,
        requestSeqRef: historyLoadSeqRef,
        sessionId,
      }),
    });
  }, [apiClient]);

  const refreshWorkbarTasks = useCallback(async () => {
    if (!currentSession) {
      return;
    }
    await Promise.all([
      loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId, true),
      loadHistoryWorkbarTasks(currentSession.id, true),
    ]);
  }, [activeConversationTurnId, currentSession, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks]);

  const markHistoryWorkbarTasksStale = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    workbarHistoryStaleSessionsRef.current.add(sessionId);
    markWorkbarHistoryStale(apiClient, sessionId);
  }, [apiClient]);

  const patchCurrentTurnWorkbarTask = useCallback((sessionId: string, task: TaskWorkbarItem) => {
    if (!sessionId || !task?.id) {
      return false;
    }
    const cachePatched = upsertWorkbarCurrentTurnCachedTask(apiClient, sessionId, task);
    if (currentSessionRef.current !== sessionId) {
      return cachePatched;
    }
    let statePatched = false;
    setWorkbarCurrentTurnTasks((prev) => {
      const currentTurnId = typeof activeConversationTurnId === 'string'
        ? activeConversationTurnId.trim()
        : '';
      const taskTurnId = typeof task.conversationTurnId === 'string'
        ? task.conversationTurnId.trim()
        : '';
      const index = prev.findIndex((item) => item.id === task.id);
      if (index >= 0) {
        const nextTasks = [...prev];
        nextTasks[index] = task;
        statePatched = true;
        return nextTasks;
      }
      if (currentTurnId && taskTurnId && currentTurnId !== taskTurnId) {
        return prev;
      }
      statePatched = true;
      return [task, ...prev];
    });
    return cachePatched || statePatched;
  }, [activeConversationTurnId, apiClient]);

  const removeCurrentTurnWorkbarTask = useCallback((sessionId: string, taskId: string) => {
    if (!sessionId || !taskId) {
      return false;
    }
    const cachePatched = removeWorkbarCurrentTurnCachedTask(apiClient, sessionId, taskId);
    if (currentSessionRef.current !== sessionId) {
      return cachePatched;
    }
    let statePatched = false;
    setWorkbarCurrentTurnTasks((prev) => {
      const nextTasks = prev.filter((task) => task.id !== taskId);
      if (nextTasks.length !== prev.length) {
        statePatched = true;
      }
      return nextTasks;
    });
    return cachePatched || statePatched;
  }, [apiClient]);

  const patchHistoryWorkbarTask = useCallback((sessionId: string, task: TaskWorkbarItem) => {
    if (!sessionId || !task?.id) {
      return false;
    }
    const cachePatched = upsertWorkbarHistoryCachedTask(apiClient, sessionId, task);
    if (currentSessionRef.current !== sessionId || workbarHistoryLoadedSessionId !== sessionId) {
      return cachePatched;
    }
    let statePatched = false;
    setWorkbarHistoryTasks((prev) => {
      const index = prev.findIndex((item) => item.id === task.id);
      if (index >= 0) {
        const nextTasks = [...prev];
        nextTasks[index] = task;
        statePatched = true;
        return nextTasks;
      }
      statePatched = true;
      return [task, ...prev];
    });
    return cachePatched || statePatched;
  }, [apiClient, workbarHistoryLoadedSessionId]);

  const removeHistoryWorkbarTask = useCallback((sessionId: string, taskId: string) => {
    if (!sessionId || !taskId) {
      return false;
    }
    const cachePatched = removeWorkbarHistoryCachedTask(apiClient, sessionId, taskId);
    if (currentSessionRef.current !== sessionId || workbarHistoryLoadedSessionId !== sessionId) {
      return cachePatched;
    }
    let statePatched = false;
    setWorkbarHistoryTasks((prev) => {
      const nextTasks = prev.filter((task) => task.id !== taskId);
      if (nextTasks.length !== prev.length) {
        statePatched = true;
      }
      return nextTasks;
    });
    return cachePatched || statePatched;
  }, [apiClient, workbarHistoryLoadedSessionId]);

  const resetAllWorkbarState = useCallback(() => {
    currentTurnLoadSeqRef.current += 1;
    historyLoadSeqRef.current += 1;
    workbarHistoryStaleSessionsRef.current.clear();
    setWorkbarCurrentTurnTasks([]);
    setWorkbarHistoryTasks([]);
    setWorkbarError(null);
    setWorkbarHistoryError(null);
    setWorkbarLoading(false);
    setWorkbarHistoryLoading(false);
    setWorkbarHistoryLoadedSessionId(null);
  }, []);

  const resetHistoryWorkbarState = useCallback(() => {
    historyLoadSeqRef.current += 1;
    workbarHistoryStaleSessionsRef.current.clear();
    setWorkbarHistoryTasks([]);
    setWorkbarHistoryError(null);
    setWorkbarHistoryLoadedSessionId(null);
    setWorkbarHistoryLoading(false);
  }, []);

  return {
    workbarCurrentTurnTasks,
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
  };
};

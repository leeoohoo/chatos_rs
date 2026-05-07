import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { Message } from '../../types';
import type { TaskWorkbarItem } from '../TaskWorkbar';
import {
  collectMessageToolCalls,
  hasToolCallError,
  normalizeWorkbarTask,
  selectLatestTurnTasks,
  shouldRefreshForTaskMutationToolCall,
  extractTaskIdsFromToolCall,
} from './helpers';
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

const CURRENT_TURN_MUTATION_FALLBACK_LIMIT = 8;

interface SessionLike {
  id: string;
}

interface WorkbarStateApiClient {
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
}

interface UseWorkbarStateParams {
  apiClient: WorkbarStateApiClient;
  currentSession: SessionLike | null;
  messages: Message[];
}

export const useWorkbarState = ({
  apiClient,
  currentSession,
  messages,
}: UseWorkbarStateParams) => {
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

  const activeConversationTurnId = useMemo(() => {
    if (!currentSession) {
      return null;
    }

    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i];
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }
      const turnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      if (turnId) {
        return turnId;
      }
    }

    return null;
  }, [currentSession, messages]);

  const currentTurnMutationTaskIds = useMemo(() => {
    if (!currentSession || !activeConversationTurnId) {
      return [];
    }

    const ids = new Set<string>();
    let lastKnownTurnId = '';

    for (const message of messages) {
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }

      const messageTurnId = typeof message?.metadata?.conversation_turn_id === 'string'
        ? message.metadata.conversation_turn_id.trim()
        : '';
      if (messageTurnId) {
        lastKnownTurnId = messageTurnId;
      }

      const effectiveTurnId = messageTurnId || lastKnownTurnId;
      if (effectiveTurnId !== activeConversationTurnId) {
        continue;
      }

      const toolCalls = collectMessageToolCalls(message);
      for (const toolCall of toolCalls) {
        if (!shouldRefreshForTaskMutationToolCall(toolCall)) {
          continue;
        }
        if (toolCall?.completed !== true || hasToolCallError(toolCall)) {
          continue;
        }

        extractTaskIdsFromToolCall(toolCall).forEach((taskId) => ids.add(taskId));
      }
    }

    return Array.from(ids);
  }, [
    activeConversationTurnId,
    currentSession,
    messages,
  ]);

  const mergedCurrentTurnTasks = useMemo(() => {
    const baseTasks = workbarCurrentTurnTasks.length > 0
      ? workbarCurrentTurnTasks
      : selectLatestTurnTasks(workbarHistoryTasks);

    if (currentTurnMutationTaskIds.length === 0) {
      return baseTasks;
    }

    const existing = new Set(baseTasks.map((task) => task.id));
    const fallbackCandidates = workbarHistoryTasks
      .filter((task) => currentTurnMutationTaskIds.includes(task.id) && !existing.has(task.id))
      .slice(0, CURRENT_TURN_MUTATION_FALLBACK_LIMIT);

    if (fallbackCandidates.length === 0) {
      return baseTasks;
    }

    return [...baseTasks, ...fallbackCandidates];
  }, [currentTurnMutationTaskIds, workbarCurrentTurnTasks, workbarHistoryTasks]);

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

    const requestSeq = currentTurnLoadSeqRef.current + 1;
    currentTurnLoadSeqRef.current = requestSeq;
    const turnId = typeof conversationTurnId === 'string' ? conversationTurnId.trim() : '';
    const cached = peekWorkbarCurrentTurnCacheEntry(apiClient, sessionId, turnId || null);
    if (
      !force
      &&
      cached
      && cached.turnId === (turnId || null)
      && !cached.stale
    ) {
      setWorkbarCurrentTurnTasks(cached.tasks);
      setWorkbarError(null);
      setWorkbarLoading(false);
      return;
    }

    const existingInflight = !force
      ? getWorkbarCurrentTurnInflight(apiClient, sessionId, turnId || null)
      : null;
    if (existingInflight) {
      setWorkbarLoading(true);
      setWorkbarError(null);
      try {
        const normalizedTasks = await existingInflight;
        if (
          currentTurnLoadSeqRef.current !== requestSeq
          || currentSessionRef.current !== sessionId
        ) {
          return;
        }
        setWorkbarCurrentTurnTasks(normalizedTasks);
      } catch (error) {
        if (
          currentTurnLoadSeqRef.current !== requestSeq
          || currentSessionRef.current !== sessionId
        ) {
          return;
        }
        setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
      } finally {
        if (
          currentTurnLoadSeqRef.current === requestSeq
          && currentSessionRef.current === sessionId
        ) {
          setWorkbarLoading(false);
        }
      }
      return;
    }

    setWorkbarLoading(true);
    setWorkbarError(null);
    try {
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
      const normalizedTasks = await inflight;

      if (
        currentTurnLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarCurrentTurnTasks(normalizedTasks);
    } catch (error) {
      if (
        currentTurnLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
    } finally {
      if (
        currentTurnLoadSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setWorkbarLoading(false);
      }
    }
  }, [apiClient]);

  const loadHistoryWorkbarTasks = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      setWorkbarHistoryTasks([]);
      setWorkbarHistoryError(null);
      setWorkbarHistoryLoadedSessionId(null);
      setWorkbarHistoryLoading(false);
      return;
    }

    const requestSeq = historyLoadSeqRef.current + 1;
    historyLoadSeqRef.current = requestSeq;
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

    const existingInflight = !force
      ? getWorkbarHistoryInflight(apiClient, sessionId)
      : null;
    if (existingInflight) {
      setWorkbarHistoryLoading(true);
      setWorkbarHistoryError(null);
      try {
        const normalizedTasks = await existingInflight;
        if (
          historyLoadSeqRef.current !== requestSeq
          || currentSessionRef.current !== sessionId
        ) {
          return;
        }
        workbarHistoryStaleSessionsRef.current.delete(sessionId);
        setWorkbarHistoryTasks(normalizedTasks);
        setWorkbarHistoryLoadedSessionId(sessionId);
      } catch (error) {
        if (
          historyLoadSeqRef.current !== requestSeq
          || currentSessionRef.current !== sessionId
        ) {
          return;
        }
        setWorkbarHistoryError(error instanceof Error ? error.message : '任务加载失败');
      } finally {
        if (
          historyLoadSeqRef.current === requestSeq
          && currentSessionRef.current === sessionId
        ) {
          setWorkbarHistoryLoading(false);
        }
      }
      return;
    }

    setWorkbarHistoryLoading(true);
    setWorkbarHistoryError(null);
    try {
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
      const normalizedTasks = await inflight;
      if (
        historyLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      workbarHistoryStaleSessionsRef.current.delete(sessionId);
      setWorkbarHistoryTasks(normalizedTasks);
      setWorkbarHistoryLoadedSessionId(sessionId);
    } catch (error) {
      if (
        historyLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarHistoryError(error instanceof Error ? error.message : '任务加载失败');
    } finally {
      if (
        historyLoadSeqRef.current === requestSeq
        && currentSessionRef.current === sessionId
      ) {
        setWorkbarHistoryLoading(false);
      }
    }
  }, [apiClient, workbarHistoryLoadedSessionId, workbarHistoryTasks.length]);

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
  };
};

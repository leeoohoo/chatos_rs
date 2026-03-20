import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { TaskWorkbarItem } from '../TaskWorkbar';
import {
  collectMessageToolCalls,
  hasToolCallError,
  normalizeWorkbarTask,
  selectLatestTurnTasks,
  shouldRefreshForTaskMutationToolCall,
  extractTaskIdsFromToolCall,
} from './helpers';

const CURRENT_TURN_MUTATION_FALLBACK_LIMIT = 8;

interface UseWorkbarStateParams {
  apiClient: any;
  activePanel: string;
  currentSession: any | null;
  messages: any[];
  sessionSummaryPaneVisible: boolean;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<any> | void;
}

export const useWorkbarState = ({
  apiClient,
  activePanel,
  currentSession,
  messages,
  sessionSummaryPaneVisible,
  loadContactMemoryContext,
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
  const handledTaskMutationKeysRef = useRef<Set<string>>(new Set());

  const activeConversationTurnId = useMemo(() => {
    if (!currentSession) {
      return null;
    }

    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i] as any;
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

    for (const message of messages as any[]) {
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

  const loadCurrentTurnWorkbarTasks = useCallback(async (sessionId: string, conversationTurnId?: string | null) => {
    if (!sessionId) {
      setWorkbarCurrentTurnTasks([]);
      setWorkbarError(null);
      setWorkbarLoading(false);
      return;
    }

    const requestSeq = currentTurnLoadSeqRef.current + 1;
    currentTurnLoadSeqRef.current = requestSeq;
    const turnId = typeof conversationTurnId === 'string' ? conversationTurnId.trim() : '';

    setWorkbarLoading(true);
    setWorkbarError(null);
    try {
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

    if (!force && workbarHistoryLoadedSessionId === sessionId && workbarHistoryTasks.length > 0) {
      return;
    }

    const requestSeq = historyLoadSeqRef.current + 1;
    historyLoadSeqRef.current = requestSeq;
    setWorkbarHistoryLoading(true);
    setWorkbarHistoryError(null);
    try {
      const tasks = await apiClient.getTaskManagerTasks(sessionId, {
        includeDone: true,
        limit: 300,
      });
      if (
        historyLoadSeqRef.current !== requestSeq
        || currentSessionRef.current !== sessionId
      ) {
        return;
      }
      setWorkbarHistoryTasks(tasks.map(normalizeWorkbarTask));
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
      loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId),
      loadHistoryWorkbarTasks(currentSession.id, true),
      loadContactMemoryContext(currentSession.id, true),
    ]);
  }, [activeConversationTurnId, currentSession, loadContactMemoryContext, loadCurrentTurnWorkbarTasks, loadHistoryWorkbarTasks]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      return;
    }

    const handled = handledTaskMutationKeysRef.current;
    const pendingKeys: string[] = [];

    for (const message of messages as any[]) {
      if (message?.sessionId && message.sessionId !== currentSession.id) {
        continue;
      }

      const toolCalls = collectMessageToolCalls(message);
      for (const toolCall of toolCalls) {
        if (!shouldRefreshForTaskMutationToolCall(toolCall)) {
          continue;
        }
        if (toolCall?.completed !== true) {
          continue;
        }
        if (hasToolCallError(toolCall)) {
          continue;
        }

        const toolCallId = String(toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId || '').trim();
        const key = currentSession.id + ':' + String(message?.id || '') + ':' + (toolCallId || String(toolCall?.name || ''));
        if (handled.has(key)) {
          continue;
        }
        pendingKeys.push(key);
      }
    }

    if (pendingKeys.length === 0) {
      return;
    }

    pendingKeys.forEach((key) => handled.add(key));
    if (handled.size > 2048) {
      const tail = Array.from(handled).slice(-1024);
      handled.clear();
      tail.forEach((key) => handled.add(key));
    }
    void loadCurrentTurnWorkbarTasks(currentSession.id, activeConversationTurnId);
    if (sessionSummaryPaneVisible) {
      void loadContactMemoryContext(currentSession.id, true);
    }
  }, [
    activeConversationTurnId,
    activePanel,
    currentSession,
    loadContactMemoryContext,
    loadCurrentTurnWorkbarTasks,
    messages,
    sessionSummaryPaneVisible,
  ]);

  const resetAllWorkbarState = useCallback(() => {
    currentTurnLoadSeqRef.current += 1;
    historyLoadSeqRef.current += 1;
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
    refreshWorkbarTasks,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
  };
};

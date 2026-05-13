import { useMemo } from 'react';

import type { Message } from '../../types';
import {
  collectMessageToolCalls,
  extractTaskIdsFromToolCall,
  hasToolCallError,
  shouldRefreshForTaskMutationToolCall,
} from './toolCallHelpers';
import { selectLatestTurnTasks } from './workbarTransforms';
import type {
  WorkbarSessionLike as SessionLike,
  WorkbarStateApiClient,
} from './useWorkbarTaskResourceState';
import { useWorkbarTaskResourceState } from './useWorkbarTaskResourceState';

const CURRENT_TURN_MUTATION_FALLBACK_LIMIT = 8;

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

  const {
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
  } = useWorkbarTaskResourceState({
    apiClient,
    currentSession,
    activeConversationTurnId,
  });

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

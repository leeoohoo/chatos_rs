// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type {
  ConversationTaskRunnerActiveMessageTasksResponse,
  UserMessageTurnResponse,
} from '../../lib/api/client/types';
import { normalizeRawMessages } from '../../lib/domain/messages';
import type { Message } from '../../types';
import type { UserMessageTaskState, UserMessageTurn } from './types';

const PAGE_SIZE = 10;
const LIVE_TASK_POLL_INTERVAL_MS = 12000;
const EXTERNAL_REFRESH_DELAY_MS = 350;
const EXTERNAL_REFRESH_RETRY_DELAYS_MS = [600, 1400, 3000, 5000];

interface UseConversationUserMessagesOptions {
  refreshKey?: string | number | null;
  refreshDelayMs?: number;
  liveMessages?: Message[];
}

const EMPTY_LIVE_MESSAGES: Message[] = [];

const readRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const readRefreshKey = (value: string | number | null | undefined): string => {
  if (typeof value === 'string') {
    return value.trim();
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }
  return '';
};

const resolveRefreshDelay = (value: number | null | undefined): number => (
  typeof value === 'number' && Number.isFinite(value) && value >= 0
    ? value
    : EXTERNAL_REFRESH_DELAY_MS
);

const readStringArray = (value: unknown): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map(readString)
    .filter(Boolean);
};

const activeStatus = (value: unknown): boolean => {
  const status = readString(value).toLowerCase();
  return ['pending', 'queued', 'running', 'processing', 'in_progress'].includes(status);
};

const terminalStatus = (value: unknown): boolean => {
  const status = readString(value).toLowerCase();
  return ['completed', 'succeeded', 'failed', 'blocked', 'cancelled', 'canceled'].includes(status);
};

const EMPTY_TASK_STATE: UserMessageTaskState = {
  hasTask: false,
  running: false,
  label: null,
  runningCount: 0,
};

const taskStateFromMessage = (message: Message | null): UserMessageTaskState => {
  const taskRunnerAsync = readRecord(message?.metadata?.task_runner_async);
  if (!taskRunnerAsync) {
    return EMPTY_TASK_STATE;
  }

  const runningIds = readStringArray(taskRunnerAsync['running_task_ids']);
  const queuedIds = readStringArray(taskRunnerAsync['queued_task_ids']);
  const pendingIds = readStringArray(taskRunnerAsync['pending_task_ids']);
  const createdIds = readStringArray(taskRunnerAsync['created_task_ids']);
  const terminalIds = [
    ...readStringArray(taskRunnerAsync['terminal_task_ids']),
    ...readStringArray(taskRunnerAsync['succeeded_task_ids']),
    ...readStringArray(taskRunnerAsync['failed_task_ids']),
    ...readStringArray(taskRunnerAsync['blocked_task_ids']),
    ...readStringArray(taskRunnerAsync['cancelled_task_ids']),
  ];
  const runningCount = runningIds.length + queuedIds.length + pendingIds.length;
  const statusRunning = activeStatus(taskRunnerAsync['overall_status']) || activeStatus(taskRunnerAsync['status']);
  const statusTerminal = terminalStatus(taskRunnerAsync['overall_status']) || terminalStatus(taskRunnerAsync['status']);
  const hasTask = runningCount > 0
    || createdIds.length > 0
    || terminalIds.length > 0
    || statusRunning
    || statusTerminal
    || readString(taskRunnerAsync['task_id']).length > 0
    || readString(taskRunnerAsync['last_task_id']).length > 0;
  const running = statusRunning || (!statusTerminal && runningCount > 0);

  return {
    hasTask,
    running,
    label: runningCount > 0
      ? `${runningCount}`
      : readString(taskRunnerAsync['overall_status']) || readString(taskRunnerAsync['status']) || null,
    runningCount: running ? runningCount : 0,
  };
};

const taskStatesFromActiveMessageTasks = (
  response: ConversationTaskRunnerActiveMessageTasksResponse,
  visibleItems: UserMessageTurn[],
): Map<string, UserMessageTaskState> => {
  const taskStates = new Map<string, UserMessageTaskState>();
  const messageIdByTurnId = new Map(
    visibleItems
      .map((item) => [readString(item.turnId), item.userMessage.id] as const)
      .filter(([turnId]) => Boolean(turnId)),
  );
  const writeTaskState = (
    sourceUserMessageId: string,
    runningCount: number,
  ) => {
    const normalizedMessageId = readString(sourceUserMessageId);
    if (!normalizedMessageId) {
      return;
    }
    taskStates.set(normalizedMessageId, {
      hasTask: true,
      running: true,
      label: `${runningCount}`,
      runningCount,
    });
  };
  const items = Array.isArray(response.items) ? response.items : [];
  items.forEach((item) => {
    const sourceUserMessageId = readString(item.source_user_message_id)
      || messageIdByTurnId.get(readString(item.source_turn_id))
      || '';
    if (!sourceUserMessageId) {
      return;
    }
    const runningCount = Math.max(Number(item.running_count || 0), 0);
    if (runningCount <= 0) {
      return;
    }
    writeTaskState(sourceUserMessageId, runningCount);
  });
  const ids = Array.isArray(response.running_source_user_message_ids)
    ? response.running_source_user_message_ids
    : [];
  ids.forEach((id) => {
    const sourceUserMessageId = readString(id);
    if (!sourceUserMessageId || taskStates.has(sourceUserMessageId)) {
      return;
    }
    writeTaskState(sourceUserMessageId, 1);
  });
  return taskStates;
};

const mergeTaskState = (
  user: UserMessageTaskState,
  assistant: UserMessageTaskState,
): UserMessageTaskState => ({
  hasTask: user.hasTask || assistant.hasTask,
  running: user.running || assistant.running,
  label: user.running ? user.label : assistant.label || user.label,
  runningCount: user.runningCount + assistant.runningCount,
});

const mergeLiveTaskState = (
  existing: UserMessageTaskState,
  live: UserMessageTaskState,
): UserMessageTaskState => {
  if (!live.hasTask) {
    return {
      ...existing,
      running: false,
      label: existing.running ? null : existing.label,
      runningCount: 0,
    };
  }
  return {
    hasTask: true,
    running: live.running,
    label: live.label || existing.label,
    runningCount: live.runningCount,
  };
};

const sameTaskState = (
  left: UserMessageTaskState,
  right: UserMessageTaskState,
): boolean => (
  left.hasTask === right.hasTask
  && left.running === right.running
  && left.label === right.label
  && left.runningCount === right.runningCount
);

const containsUserMessageId = (
  items: UserMessageTurn[],
  messageId: string,
): boolean => (
  Boolean(messageId)
  && items.some((item) => item.userMessage.id === messageId)
);

const normalizeTurn = (
  sessionId: string,
  item: UserMessageTurnResponse,
): UserMessageTurn | null => {
  if (!item?.user_message?.id) {
    return null;
  }
  const [userMessage] = normalizeRawMessages([item.user_message], sessionId);
  if (!userMessage) {
    return null;
  }
  const [finalAssistantMessage] = item.final_assistant_message
    ? normalizeRawMessages([item.final_assistant_message], sessionId)
    : [];

  return {
    turnId: item.turn_id,
    userMessage,
    finalAssistantMessage: finalAssistantMessage || null,
    hasProcess: item.has_process === true,
    toolCallCount: Number(item.tool_call_count || 0),
    thinkingCount: Number(item.thinking_count || 0),
    processMessageCount: Number(item.process_message_count || 0),
    taskState: mergeTaskState(
      taskStateFromMessage(userMessage),
      taskStateFromMessage(finalAssistantMessage || null),
    ),
  };
};

const messageTurnId = (message: Message | null | undefined): string => {
  const taskRunnerAsync = readRecord(message?.metadata?.task_runner_async);
  return readString(message?.metadata?.conversation_turn_id)
    || readString(taskRunnerAsync?.['source_turn_id'])
    || readString(message?.id);
};

const messageTime = (message: Message): number => (
  message.createdAt instanceof Date && Number.isFinite(message.createdAt.getTime())
    ? message.createdAt.getTime()
    : 0
);

export const buildLiveUserMessageTurns = (
  sessionId: string | null | undefined,
  messages: Message[],
): UserMessageTurn[] => {
  const normalizedSessionId = readString(sessionId);
  if (!normalizedSessionId) {
    return [];
  }
  const sessionMessages = messages.filter((message) => (
    message?.sessionId === normalizedSessionId
    && message.metadata?.historyProcessPlaceholder !== true
  ));
  const assistantByTurnId = new Map<string, Message>();
  sessionMessages.forEach((message) => {
    if (message.role !== 'assistant') {
      return;
    }
    const turnId = messageTurnId(message);
    if (turnId) {
      assistantByTurnId.set(turnId, message);
    }
  });
  return sessionMessages
    .filter((message) => message.role === 'user')
    .map((userMessage) => {
      const turnId = messageTurnId(userMessage);
      const finalAssistantMessage = assistantByTurnId.get(turnId) || null;
      return {
        turnId,
        userMessage,
        finalAssistantMessage,
        hasProcess: false,
        toolCallCount: 0,
        thinkingCount: 0,
        processMessageCount: 0,
        taskState: mergeTaskState(
          taskStateFromMessage(userMessage),
          taskStateFromMessage(finalAssistantMessage),
        ),
      };
    })
    .sort((left, right) => messageTime(right.userMessage) - messageTime(left.userMessage));
};

const mergeTurnTaskState = (
  persisted: UserMessageTaskState,
  live: UserMessageTaskState,
): UserMessageTaskState => ({
  hasTask: persisted.hasTask || live.hasTask,
  running: persisted.running || live.running,
  label: live.running ? live.label : persisted.label || live.label,
  runningCount: Math.max(persisted.runningCount, live.runningCount),
});

export const mergeLiveUserMessageTurns = (
  persistedItems: UserMessageTurn[],
  liveItems: UserMessageTurn[],
): UserMessageTurn[] => {
  if (liveItems.length === 0) {
    return persistedItems;
  }
  const liveByMessageId = new Map(
    liveItems.map((item) => [item.userMessage.id, item]),
  );
  const liveByTurnId = new Map(
    liveItems
      .filter((item) => Boolean(readString(item.turnId)))
      .map((item) => [readString(item.turnId), item]),
  );
  const consumedLiveIds = new Set<string>();
  const mergedPersisted = persistedItems.map((persistedItem) => {
    const liveItem = liveByMessageId.get(persistedItem.userMessage.id)
      || liveByTurnId.get(readString(persistedItem.turnId));
    if (!liveItem) {
      return persistedItem;
    }
    consumedLiveIds.add(liveItem.userMessage.id);
    return {
      ...persistedItem,
      finalAssistantMessage: persistedItem.finalAssistantMessage || liveItem.finalAssistantMessage,
      hasProcess: persistedItem.hasProcess || liveItem.hasProcess,
      toolCallCount: Math.max(persistedItem.toolCallCount, liveItem.toolCallCount),
      thinkingCount: Math.max(persistedItem.thinkingCount, liveItem.thinkingCount),
      processMessageCount: Math.max(
        persistedItem.processMessageCount,
        liveItem.processMessageCount,
      ),
      taskState: mergeTurnTaskState(persistedItem.taskState, liveItem.taskState),
    };
  });
  const missingLiveItems = liveItems.filter(
    (item) => !consumedLiveIds.has(item.userMessage.id),
  );
  return [...missingLiveItems, ...mergedPersisted];
};

export const useConversationUserMessages = (
  sessionId: string | null | undefined,
  options: UseConversationUserMessagesOptions = {},
) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
  const externalRefreshKey = readRefreshKey(options.refreshKey);
  const externalRefreshDelayMs = resolveRefreshDelay(options.refreshDelayMs);
  const liveMessages = options.liveMessages || EMPTY_LIVE_MESSAGES;
  const [items, setItems] = useState<UserMessageTurn[]>([]);
  const [nextBefore, setNextBefore] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestSeqRef = useRef(0);
  const activeSessionIdRef = useRef<string | null | undefined>(sessionId);
  const itemsRef = useRef<UserMessageTurn[]>([]);
  const hydratingLiveTaskStatesRef = useRef(false);
  const externalRefreshKeyRef = useRef(externalRefreshKey);

  useEffect(() => {
    activeSessionIdRef.current = sessionId;
  }, [sessionId]);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const hydrateLiveTaskStates = useCallback(async (baseItems: UserMessageTurn[]) => {
    if (!sessionId || baseItems.length === 0) {
      return;
    }
    if (hydratingLiveTaskStatesRef.current) {
      return;
    }
    const candidates = baseItems.filter((item) => item.userMessage.id);
    if (candidates.length === 0) {
      return;
    }
    hydratingLiveTaskStatesRef.current = true;
    try {
      const response = await apiClient.getConversationTaskRunnerActiveMessageTasks(sessionId, {
        sourceUserMessageIds: candidates.map((item) => item.userMessage.id),
        sourceTurnIds: candidates.map((item) => item.turnId).filter(Boolean),
      });
      if (activeSessionIdRef.current !== sessionId) {
        return;
      }
      const activeTaskStateByMessageId = taskStatesFromActiveMessageTasks(response, candidates);
      const candidateMessageIds = new Set(candidates.map((item) => item.userMessage.id));
      setItems((current) => {
        let changed = false;
        const nextItems = current.map((item) => {
          if (!candidateMessageIds.has(item.userMessage.id)) {
            return item;
          }
          const activeTaskState = activeTaskStateByMessageId.get(item.userMessage.id);
          const taskState = activeTaskState
            ? mergeLiveTaskState(item.taskState, activeTaskState)
            : mergeLiveTaskState(item.taskState, EMPTY_TASK_STATE);
          if (sameTaskState(item.taskState, taskState)) {
            return item;
          }
          changed = true;
          return { ...item, taskState };
        });
        return changed ? nextItems : current;
      });
    } catch (err) {
      console.warn('Failed to hydrate user message active task state:', err);
    } finally {
      hydratingLiveTaskStatesRef.current = false;
    }
  }, [apiClient, sessionId]);

  const hydrateInitialLiveTaskStates = useCallback(async (baseItems: UserMessageTurn[]) => {
    await hydrateLiveTaskStates(baseItems);
  }, [hydrateLiveTaskStates]);

  const loadPage = useCallback(async (
    before: string | null,
    mode: 'replace' | 'append',
  ): Promise<UserMessageTurn[]> => {
    const requestSeq = requestSeqRef.current + 1;
    requestSeqRef.current = requestSeq;
    if (!sessionId) {
      setItems([]);
      setNextBefore(null);
      setHasMore(false);
      setError(null);
      setLoading(false);
      setLoadingMore(false);
      return [];
    }
    if (mode === 'replace') {
      setLoading(true);
    } else {
      setLoadingMore(true);
    }
    setError(null);
    try {
      const response = await apiClient.getConversationUserMessageTurns(sessionId, {
        limit: PAGE_SIZE,
        before,
      });
      if (requestSeqRef.current !== requestSeq) {
        return [];
      }
      const nextItems = (Array.isArray(response.items) ? response.items : [])
        .map((item) => normalizeTurn(sessionId, item))
        .filter((item): item is UserMessageTurn => Boolean(item));
      setItems((current) => (mode === 'append' ? [...nextItems, ...current] : nextItems));
      void hydrateInitialLiveTaskStates(nextItems);
      setNextBefore(response.next_before || null);
      setHasMore(response.has_more === true);
      return nextItems;
    } catch (err) {
      if (requestSeqRef.current !== requestSeq) {
        return [];
      }
      setError(err instanceof Error ? err.message : t('projectUserMessages.error.loadFailed'));
      if (mode === 'replace') {
        setItems([]);
      }
      return [];
    } finally {
      if (requestSeqRef.current === requestSeq) {
        setLoading(false);
        setLoadingMore(false);
      }
    }
  }, [apiClient, hydrateInitialLiveTaskStates, sessionId, t]);

  useEffect(() => {
    requestSeqRef.current += 1;
    setItems([]);
    setNextBefore(null);
    setHasMore(false);
    setError(null);
    setLoading(Boolean(sessionId));
    setLoadingMore(false);
    void loadPage(null, 'replace');
  }, [loadPage, sessionId]);

  useEffect(() => {
    if (!sessionId) {
      externalRefreshKeyRef.current = externalRefreshKey;
      return undefined;
    }
    if (!externalRefreshKey || externalRefreshKeyRef.current === externalRefreshKey) {
      return undefined;
    }
    externalRefreshKeyRef.current = externalRefreshKey;
    if (itemsRef.current.some((item) => item.userMessage.id === externalRefreshKey)) {
      return undefined;
    }

    let cancelled = false;
    let timeoutId: number | null = null;

    const scheduleAttempt = (attempt: number, delayMs: number) => {
      timeoutId = window.setTimeout(() => {
        void (async () => {
          const loadedItems = await loadPage(null, 'replace');
          if (cancelled) {
            return;
          }
          if (
            containsUserMessageId(loadedItems, externalRefreshKey)
            || containsUserMessageId(itemsRef.current, externalRefreshKey)
          ) {
            return;
          }
          const retryDelayMs = EXTERNAL_REFRESH_RETRY_DELAYS_MS[attempt];
          if (typeof retryDelayMs === 'number') {
            scheduleAttempt(attempt + 1, retryDelayMs);
          }
        })();
      }, delayMs);
    };

    scheduleAttempt(0, externalRefreshDelayMs);
    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [externalRefreshDelayMs, externalRefreshKey, loadPage, sessionId]);

  useEffect(() => {
    if (!sessionId) {
      return undefined;
    }
    const intervalId = window.setInterval(() => {
      if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
        return;
      }
      const currentItems = itemsRef.current;
      if (!currentItems.some((item) => item.taskState.running)) {
        return;
      }
      void hydrateLiveTaskStates(currentItems);
    }, LIVE_TASK_POLL_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, [hydrateLiveTaskStates, sessionId]);

  const reload = useCallback(() => {
    void loadPage(null, 'replace');
  }, [loadPage]);

  const loadMore = useCallback(async (): Promise<UserMessageTurn[]> => {
    if (!nextBefore || loadingMore) {
      return [];
    }
    return loadPage(nextBefore, 'append');
  }, [loadPage, loadingMore, nextBefore]);

  const visibleItems = useMemo(() => mergeLiveUserMessageTurns(
    items,
    buildLiveUserMessageTurns(sessionId, liveMessages),
  ), [items, liveMessages, sessionId]);

  return {
    items: visibleItems,
    loading,
    loadingMore,
    error,
    hasMore,
    reload,
    loadMore,
  };
};

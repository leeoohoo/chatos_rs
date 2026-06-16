import { useCallback, useEffect, useRef, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { getMessageTaskRunnerGraph } from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerTask,
  UserMessageTurnResponse,
} from '../../lib/api/client/types';
import { normalizeRawMessages } from '../../lib/domain/messages';
import type { Message } from '../../types';
import type { UserMessageTaskState, UserMessageTurn } from './types';

const PAGE_SIZE = 10;
const LIVE_TASK_POLL_INTERVAL_MS = 12000;
const LIVE_TASK_HYDRATION_CONCURRENCY = 3;

const readRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
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

const taskRunningFromTask = (task: MessageTaskRunnerTask): boolean => (
  activeStatus(task.status)
  || activeStatus(task.last_run?.status)
);

const taskStateFromGraph = (graph: MessageTaskRunnerGraphResponse): UserMessageTaskState => {
  const tasks = (Array.isArray(graph.nodes) ? graph.nodes : [])
    .map((node) => node.task)
    .filter((task): task is MessageTaskRunnerTask => Boolean(task?.id));
  if (tasks.length === 0) {
    return EMPTY_TASK_STATE;
  }
  const runningCount = tasks.filter(taskRunningFromTask).length;

  return {
    hasTask: true,
    running: runningCount > 0,
    label: runningCount > 0 ? `${runningCount}` : tasks[0]?.status || null,
    runningCount,
  };
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

const runInBatches = async <T, R>(
  items: T[],
  batchSize: number,
  task: (item: T) => Promise<R>,
): Promise<R[]> => {
  const results: R[] = [];
  for (let index = 0; index < items.length; index += batchSize) {
    const batch = items.slice(index, index + batchSize);
    results.push(...await Promise.all(batch.map(task)));
  }
  return results;
};

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

export const useConversationUserMessages = (sessionId: string | null | undefined) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
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

  useEffect(() => {
    activeSessionIdRef.current = sessionId;
  }, [sessionId]);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const hydrateLiveTaskStates = useCallback(async (
    baseItems: UserMessageTurn[],
    options?: { runningOnly?: boolean },
  ) => {
    if (!sessionId || baseItems.length === 0) {
      return;
    }
    if (hydratingLiveTaskStatesRef.current) {
      return;
    }
    const candidates = baseItems.filter((item) => (
      item.taskState.hasTask
      && (!options?.runningOnly || item.taskState.running)
    ));
    if (candidates.length === 0) {
      return;
    }
    hydratingLiveTaskStatesRef.current = true;
    const request = apiClient.getRequestFn();
    try {
      const hydrated = await runInBatches(candidates, LIVE_TASK_HYDRATION_CONCURRENCY, async (item) => {
        try {
          const graph = await getMessageTaskRunnerGraph(request, item.userMessage.id, {
            sessionId,
            turnId: item.turnId,
            sourceUserMessageId: item.userMessage.id,
          });
          const liveTaskState = taskStateFromGraph(graph);
          return {
            userMessageId: item.userMessage.id,
            taskState: mergeLiveTaskState(item.taskState, liveTaskState),
          };
        } catch (err) {
          console.warn('Failed to hydrate user message task state:', err);
          return null;
        }
      });
      if (activeSessionIdRef.current !== sessionId) {
        return;
      }
      const taskStateByMessageId = new Map(
        hydrated
          .filter((item): item is { userMessageId: string; taskState: UserMessageTaskState } => Boolean(item))
          .map((item) => [item.userMessageId, item.taskState]),
      );
      if (taskStateByMessageId.size === 0) {
        return;
      }
      setItems((current) => {
        let changed = false;
        const nextItems = current.map((item) => {
          const taskState = taskStateByMessageId.get(item.userMessage.id);
          if (!taskState || sameTaskState(item.taskState, taskState)) {
            return item;
          }
          changed = true;
          return { ...item, taskState };
        });
        return changed ? nextItems : current;
      });
    } finally {
      hydratingLiveTaskStatesRef.current = false;
    }
  }, [apiClient, sessionId]);

  const hydrateInitialLiveTaskStates = useCallback(async (baseItems: UserMessageTurn[]) => {
    if (!sessionId || baseItems.length === 0) {
      return;
    }
    const candidates = baseItems.filter((item) => item.taskState.hasTask);
    if (candidates.length === 0) {
      return;
    }
    const request = apiClient.getRequestFn();
    const hydrated = await runInBatches(candidates, LIVE_TASK_HYDRATION_CONCURRENCY, async (item) => {
      try {
        const graph = await getMessageTaskRunnerGraph(request, item.userMessage.id, {
          sessionId,
          turnId: item.turnId,
          sourceUserMessageId: item.userMessage.id,
        });
        const liveTaskState = taskStateFromGraph(graph);
        if (!liveTaskState.hasTask && !item.taskState.running) {
          return null;
        }
        return {
          userMessageId: item.userMessage.id,
          taskState: mergeLiveTaskState(item.taskState, liveTaskState),
        };
      } catch (err) {
        console.warn('Failed to hydrate user message task state:', err);
        return null;
      }
    });
    if (activeSessionIdRef.current !== sessionId) {
      return;
    }
    const taskStateByMessageId = new Map(
      hydrated
        .filter((item): item is { userMessageId: string; taskState: UserMessageTaskState } => Boolean(item))
        .map((item) => [item.userMessageId, item.taskState]),
    );
    if (taskStateByMessageId.size === 0) {
      return;
    }
    setItems((current) => {
      let changed = false;
      const nextItems = current.map((item) => {
        const taskState = taskStateByMessageId.get(item.userMessage.id);
        if (!taskState || sameTaskState(item.taskState, taskState)) {
          return item;
        }
        changed = true;
        return { ...item, taskState };
      });
      return changed ? nextItems : current;
    });
  }, [apiClient, sessionId]);

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
    void loadPage(null, 'replace');
  }, [loadPage]);

  useEffect(() => {
    if (!sessionId) {
      return undefined;
    }
    const intervalId = window.setInterval(() => {
      const currentItems = itemsRef.current;
      if (!currentItems.some((item) => item.taskState.running)) {
        return;
      }
      void hydrateLiveTaskStates(currentItems, { runningOnly: true });
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

  return {
    items,
    loading,
    loadingMore,
    error,
    hasMore,
    reload,
    loadMore,
  };
};

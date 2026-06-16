import { useCallback, useEffect, useRef, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type { UserMessageTurnResponse } from '../../lib/api/client/types';
import { normalizeRawMessages } from '../../lib/domain/messages';
import type { Message } from '../../types';
import type { UserMessageTaskState, UserMessageTurn } from './types';

const PAGE_SIZE = 10;

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

const taskStateFromMessage = (message: Message | null): UserMessageTaskState => {
  const taskRunnerAsync = readRecord(message?.metadata?.task_runner_async);
  if (!taskRunnerAsync) {
    return {
      hasTask: false,
      running: false,
      label: null,
      runningCount: 0,
    };
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

const mergeTaskState = (
  user: UserMessageTaskState,
  assistant: UserMessageTaskState,
): UserMessageTaskState => ({
  hasTask: user.hasTask || assistant.hasTask,
  running: user.running || assistant.running,
  label: user.running ? user.label : assistant.label || user.label,
  runningCount: user.runningCount + assistant.runningCount,
});

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
  }, [apiClient, sessionId, t]);

  useEffect(() => {
    void loadPage(null, 'replace');
  }, [loadPage]);

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

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useState } from 'react';
import { useApiClient } from '../../lib/api/ApiClientContext';
import {
  getMessageTaskRunnerRun,
  getMessageTaskRunnerTask,
  getMessageTaskRunnerTasks,
} from '../../lib/api/client/messages';
import type { MessageTaskRunnerLookupOptions } from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { readString } from './utils';

interface UseMessageTasksArgs {
  open: boolean;
  messageId: string;
  lookup?: MessageTaskRunnerLookupOptions;
}

const RUN_EVENT_PAGE_SIZE = 40;

const mergeRunEventPage = (
  current: MessageTaskRunnerRunDetailResponse,
  next: MessageTaskRunnerRunDetailResponse,
): MessageTaskRunnerRunDetailResponse => {
  const seen = new Set<string>();
  const events = [...current.events, ...next.events].filter((event) => {
    const key = readString(event.id) || `${event.run_id}:${event.created_at}:${event.event_type}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
  return {
    ...next,
    events,
    events_offset: current.events_offset ?? 0,
  };
};

export function useMessageTasks({ open, messageId, lookup }: UseMessageTasksArgs) {
  const apiClient = useApiClient();
  const [tasks, setTasks] = useState<MessageTaskRunnerTask[]>([]);
  const [sourceUserMessageId, setSourceUserMessageId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailTask, setDetailTask] = useState<MessageTaskRunnerTask | null>(null);
  const [runDetail, setRunDetail] = useState<MessageTaskRunnerRunDetailResponse | null>(null);
  const [loadingDetailId, setLoadingDetailId] = useState<string | null>(null);
  const [loadingRunId, setLoadingRunId] = useState<string | null>(null);

  const reloadTasks = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await getMessageTaskRunnerTasks(apiClient.getRequestFn(), messageId, lookup);
      const nextTasks = Array.isArray(response.items) ? response.items : [];
      setTasks(nextTasks);
      setSourceUserMessageId(
        readString(response.source_user_message_id)
        || readString(nextTasks[0]?.source_user_message_id),
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取任务失败');
      setTasks([]);
      setSourceUserMessageId(null);
    } finally {
      setLoading(false);
    }
  }, [apiClient, messageId, lookup]);

  const openDetail = useCallback(async (task: MessageTaskRunnerTask) => {
    setLoadingDetailId(task.id);
    setError(null);
    try {
      const detailLookup = sourceUserMessageId
        ? { ...lookup, sourceUserMessageId }
        : lookup;
      const detail = await getMessageTaskRunnerTask(apiClient.getRequestFn(), messageId, task.id, detailLookup);
      setDetailTask(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取任务详情失败');
    } finally {
      setLoadingDetailId(null);
    }
  }, [apiClient, messageId, lookup, sourceUserMessageId]);

  const openRun = useCallback(async (task: MessageTaskRunnerTask) => {
    const runId = readString(task.last_run_id);
    if (!runId) {
      return;
    }
    setLoadingRunId(runId);
    setError(null);
    try {
      const detailLookup = sourceUserMessageId
        ? { ...lookup, sourceUserMessageId }
        : lookup;
      const detail = await getMessageTaskRunnerRun(apiClient.getRequestFn(), messageId, runId, {
        ...detailLookup,
        eventLimit: RUN_EVENT_PAGE_SIZE,
        eventOffset: 0,
      });
      setRunDetail(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行详情失败');
    } finally {
      setLoadingRunId(null);
    }
  }, [apiClient, messageId, lookup, sourceUserMessageId]);

  const loadMoreRunEvents = useCallback(async () => {
    if (!runDetail?.events_has_more) {
      return;
    }
    const runId = readString(runDetail.run?.id);
    if (!runId || loadingRunId === runId) {
      return;
    }
    setLoadingRunId(runId);
    setError(null);
    try {
      const detailLookup = sourceUserMessageId
        ? { ...lookup, sourceUserMessageId }
        : lookup;
      const offset = (runDetail.events_offset ?? 0) + runDetail.events.length;
      const detail = await getMessageTaskRunnerRun(apiClient.getRequestFn(), messageId, runId, {
        ...detailLookup,
        eventLimit: RUN_EVENT_PAGE_SIZE,
        eventOffset: offset,
      });
      setRunDetail((current) => (current ? mergeRunEventPage(current, detail) : detail));
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取更多运行事件失败');
    } finally {
      setLoadingRunId(null);
    }
  }, [apiClient, loadingRunId, messageId, lookup, runDetail, sourceUserMessageId]);

  useEffect(() => {
    if (!open) {
      return;
    }
    void reloadTasks();
  }, [open, reloadTasks]);

  useEffect(() => {
    if (!open) {
      setDetailTask(null);
      setRunDetail(null);
      setError(null);
    }
  }, [open]);

  return {
    tasks,
    sourceUserMessageId,
    loading,
    error,
    detailTask,
    runDetail,
    loadingDetailId,
    loadingRunId,
    reloadTasks,
    openDetail,
    openRun,
    loadMoreRunEvents,
    closeDetail: () => setDetailTask(null),
    closeRun: () => setRunDetail(null),
  };
}

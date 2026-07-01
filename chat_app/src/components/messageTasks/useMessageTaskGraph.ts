// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useApiClient } from '../../lib/api/ApiClientContext';
import {
  getMessageTaskRunnerGraph,
  getMessageTaskRunnerGraphRun,
  getMessageTaskRunnerTask,
} from '../../lib/api/client/messages';
import type { MessageTaskRunnerLookupOptions } from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { readString } from './utils';

interface UseMessageTaskGraphArgs {
  open: boolean;
  messageId: string;
  lookup?: MessageTaskRunnerLookupOptions;
}

interface TaskSourceLookup {
  messageId: string;
  lookup?: MessageTaskRunnerLookupOptions;
}

const EMPTY_GRAPH: MessageTaskRunnerGraphResponse = {
  root_task_ids: [],
  nodes: [],
  edges: [],
  source_session_id: null,
  source_turn_id: null,
  source_user_message_id: null,
};

const RUN_EVENT_PAGE_SIZE = 40;

const isTemporaryMessageId = (value: string): boolean => value.startsWith('temp_');

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

export const buildTaskSourceLookup = ({
  task,
  graph,
  fallbackMessageId,
  fallbackLookup,
}: {
  task: MessageTaskRunnerTask;
  graph: MessageTaskRunnerGraphResponse;
  fallbackMessageId: string;
  fallbackLookup?: MessageTaskRunnerLookupOptions;
}): TaskSourceLookup => {
  const taskId = readString(task.id);
  const taskSourceSessionId = readString(task.source_session_id)
    || readString(graph.source_session_id)
    || readString(fallbackLookup?.sessionId);
  const taskSourceUserMessageId = readString(task.source_user_message_id);
  const taskSourceTurnId = readString(task.source_turn_id);
  const lookupSourceUserMessageId = taskSourceUserMessageId
    || (!taskSourceTurnId ? readString(fallbackLookup?.sourceUserMessageId) : '');
  const lookupTurnId = taskSourceTurnId
    || (!taskSourceUserMessageId ? readString(fallbackLookup?.turnId) : '');
  const lookup: MessageTaskRunnerLookupOptions = {
    ...fallbackLookup,
    sessionId: taskSourceSessionId || fallbackLookup?.sessionId || null,
    turnId: lookupTurnId || null,
    sourceUserMessageId: lookupSourceUserMessageId || null,
  };
  const lookupMessageId = taskSourceUserMessageId && !isTemporaryMessageId(taskSourceUserMessageId)
    ? taskSourceUserMessageId
    : taskSourceSessionId && (taskSourceUserMessageId || taskSourceTurnId)
      ? `task-source-${taskId || 'unknown'}`
      : fallbackMessageId;

  return {
    messageId: lookupMessageId,
    lookup,
  };
};

export function useMessageTaskGraph({ open, messageId, lookup }: UseMessageTaskGraphArgs) {
  const apiClient = useApiClient();
  const [graph, setGraph] = useState<MessageTaskRunnerGraphResponse>(EMPTY_GRAPH);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailTask, setDetailTask] = useState<MessageTaskRunnerTask | null>(null);
  const [processTask, setProcessTask] = useState<MessageTaskRunnerTask | null>(null);
  const [runDetail, setRunDetail] = useState<MessageTaskRunnerRunDetailResponse | null>(null);
  const [loadingProcessTaskId, setLoadingProcessTaskId] = useState<string | null>(null);
  const [loadingRunId, setLoadingRunId] = useState<string | null>(null);

  const reloadGraph = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await getMessageTaskRunnerGraph(apiClient.getRequestFn(), messageId, lookup);
      setGraph({
        root_task_ids: Array.isArray(response.root_task_ids) ? response.root_task_ids : [],
        nodes: Array.isArray(response.nodes) ? response.nodes : [],
        edges: Array.isArray(response.edges) ? response.edges : [],
        source_session_id: response.source_session_id ?? null,
        source_turn_id: response.source_turn_id ?? null,
        source_user_message_id: response.source_user_message_id ?? null,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取任务流程图失败');
      setGraph(EMPTY_GRAPH);
    } finally {
      setLoading(false);
    }
  }, [apiClient, lookup, messageId]);

  const taskById = useMemo(() => {
    const map = new Map<string, MessageTaskRunnerTask>();
    graph.nodes.forEach((node) => {
      if (readString(node.task?.id)) {
        map.set(node.task.id, node.task);
      }
    });
    return map;
  }, [graph.nodes]);

  const rootTasks = useMemo(
    () => graph.root_task_ids
      .map((taskId) => taskById.get(taskId))
      .filter((task): task is MessageTaskRunnerTask => Boolean(task)),
    [graph.root_task_ids, taskById],
  );

  const allTasks = useMemo(
    () => graph.nodes.map((node) => node.task).filter((task): task is MessageTaskRunnerTask => Boolean(task)),
    [graph.nodes],
  );

  const sourceUserMessageId = useMemo(
    () => readString(graph.source_user_message_id) || readString(rootTasks[0]?.source_user_message_id),
    [graph.source_user_message_id, rootTasks],
  );

  const openDetail = useCallback((task: MessageTaskRunnerTask) => {
    setDetailTask(task);
  }, []);

  const openProcessLog = useCallback(async (task: MessageTaskRunnerTask) => {
    const taskId = readString(task.id);
    if (!taskId) {
      return;
    }
    setLoadingProcessTaskId(taskId);
    setError(null);
    try {
      const detailSource = buildTaskSourceLookup({
        task,
        graph,
        fallbackMessageId: messageId,
        fallbackLookup: lookup,
      });
      const detail = await getMessageTaskRunnerTask(
        apiClient.getRequestFn(),
        detailSource.messageId,
        taskId,
        detailSource.lookup,
      );
      setProcessTask(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取执行过程失败');
    } finally {
      setLoadingProcessTaskId(null);
    }
  }, [apiClient, graph, lookup, messageId]);

  const openRun = useCallback(async (task: MessageTaskRunnerTask) => {
    const runId = readString(task.last_run_id);
    if (!runId) {
      return;
    }
    setLoadingRunId(runId);
    setError(null);
    try {
      const detailSource = buildTaskSourceLookup({
        task,
        graph,
        fallbackMessageId: messageId,
        fallbackLookup: lookup,
      });
      const detail = await getMessageTaskRunnerGraphRun(
        apiClient.getRequestFn(),
        detailSource.messageId,
        runId,
        {
          ...detailSource.lookup,
          eventLimit: RUN_EVENT_PAGE_SIZE,
          eventOffset: 0,
        },
      );
      setRunDetail(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行详情失败');
    } finally {
      setLoadingRunId(null);
    }
  }, [apiClient, graph, lookup, messageId]);

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
      const detailSource = buildTaskSourceLookup({
        task: runDetail.task,
        graph,
        fallbackMessageId: messageId,
        fallbackLookup: lookup,
      });
      const offset = (runDetail.events_offset ?? 0) + runDetail.events.length;
      const detail = await getMessageTaskRunnerGraphRun(
        apiClient.getRequestFn(),
        detailSource.messageId,
        runId,
        {
          ...detailSource.lookup,
          eventLimit: RUN_EVENT_PAGE_SIZE,
          eventOffset: offset,
        },
      );
      setRunDetail((current) => (current ? mergeRunEventPage(current, detail) : detail));
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取更多运行事件失败');
    } finally {
      setLoadingRunId(null);
    }
  }, [apiClient, graph, loadingRunId, lookup, messageId, runDetail]);

  useEffect(() => {
    if (!open) {
      return;
    }
    void reloadGraph();
  }, [open, reloadGraph]);

  useEffect(() => {
    if (!open) {
      setDetailTask(null);
      setProcessTask(null);
      setRunDetail(null);
      setError(null);
    }
  }, [open]);

  return {
    graph,
    rootTasks,
    allTasks,
    sourceUserMessageId,
    loading,
    error,
    detailTask,
    processTask,
    loadingProcessTaskId,
    runDetail,
    loadingRunId,
    reloadGraph,
    openDetail,
    openProcessLog,
    openRun,
    loadMoreRunEvents,
    closeDetail: () => setDetailTask(null),
    closeProcessLog: () => setProcessTask(null),
    closeRun: () => setRunDetail(null),
  };
}

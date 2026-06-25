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

const EMPTY_GRAPH: MessageTaskRunnerGraphResponse = {
  root_task_ids: [],
  nodes: [],
  edges: [],
  source_session_id: null,
  source_turn_id: null,
  source_user_message_id: null,
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
      const detailLookup = sourceUserMessageId
        ? { ...lookup, sourceUserMessageId }
        : lookup;
      const detail = await getMessageTaskRunnerTask(
        apiClient.getRequestFn(),
        messageId,
        taskId,
        detailLookup,
      );
      setProcessTask(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取执行过程失败');
    } finally {
      setLoadingProcessTaskId(null);
    }
  }, [apiClient, lookup, messageId, sourceUserMessageId]);

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
      const detail = await getMessageTaskRunnerGraphRun(
        apiClient.getRequestFn(),
        messageId,
        runId,
        detailLookup,
      );
      setRunDetail(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行详情失败');
    } finally {
      setLoadingRunId(null);
    }
  }, [apiClient, lookup, messageId, sourceUserMessageId]);

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
    closeDetail: () => setDetailTask(null),
    closeProcessLog: () => setProcessTask(null),
    closeRun: () => setRunDetail(null),
  };
}

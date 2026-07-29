// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useApiClient } from '../../lib/api/ApiClientContext';
import {
  getMessageTaskRunnerGraph,
  getMessageTaskRunnerGraphRun,
  getMessageTaskRunnerRunOutputChanges,
  getMessageTaskRunnerRunOutputDiff,
  getMessageTaskRunnerTask,
  retryMessageTaskRunnerRun,
} from '../../lib/api/client/messages';
import type { MessageTaskRunnerLookupOptions } from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerFileChange,
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerRunOutputChangesResponse,
  MessageTaskRunnerRunOutputDiffResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { readString } from './utils';

interface UseMessageTaskGraphArgs {
  open: boolean;
  messageId: string;
  lookup?: MessageTaskRunnerLookupOptions;
  isTransientError?: (error: unknown) => boolean;
}

interface TaskSourceLookup {
  messageId: string;
  lookup?: MessageTaskRunnerLookupOptions;
}

interface ReloadMessageTaskGraphOptions {
  silent?: boolean;
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

export function useMessageTaskGraph({
  open,
  messageId,
  lookup,
  isTransientError,
}: UseMessageTaskGraphArgs) {
  const apiClient = useApiClient();
  const [graph, setGraph] = useState<MessageTaskRunnerGraphResponse>(EMPTY_GRAPH);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailTask, setDetailTask] = useState<MessageTaskRunnerTask | null>(null);
  const [processTask, setProcessTask] = useState<MessageTaskRunnerTask | null>(null);
  const [processRunDetail, setProcessRunDetail] = useState<MessageTaskRunnerRunDetailResponse | null>(null);
  const [runDetail, setRunDetail] = useState<MessageTaskRunnerRunDetailResponse | null>(null);
  const [changesTask, setChangesTask] = useState<MessageTaskRunnerTask | null>(null);
  const [changesSource, setChangesSource] = useState<TaskSourceLookup | null>(null);
  const [outputChanges, setOutputChanges] = useState<MessageTaskRunnerRunOutputChangesResponse | null>(null);
  const [outputDiff, setOutputDiff] = useState<MessageTaskRunnerRunOutputDiffResponse | null>(null);
  const [selectedChangePath, setSelectedChangePath] = useState<string | null>(null);
  const [loadingProcessTaskId, setLoadingProcessTaskId] = useState<string | null>(null);
  const [loadingRunId, setLoadingRunId] = useState<string | null>(null);
  const [loadingChangesRunId, setLoadingChangesRunId] = useState<string | null>(null);
  const [loadingDiffPath, setLoadingDiffPath] = useState<string | null>(null);
  const [retryingTaskId, setRetryingTaskId] = useState<string | null>(null);
  const [retryError, setRetryError] = useState<string | null>(null);
  const graphRequestSequenceRef = useRef(0);
  const overlayRequestSequenceRef = useRef(0);
  const graphRequestIdentity = useMemo(() => JSON.stringify([
    messageId,
    lookup?.sessionId ?? null,
    lookup?.turnId ?? null,
    lookup?.sourceUserMessageId ?? null,
  ]), [
    lookup?.sessionId,
    lookup?.sourceUserMessageId,
    lookup?.turnId,
    messageId,
  ]);
  const activeGraphRequestIdentityRef = useRef(graphRequestIdentity);
  activeGraphRequestIdentityRef.current = graphRequestIdentity;

  const nextOverlayRequestSequence = useCallback(() => {
    overlayRequestSequenceRef.current += 1;
    return overlayRequestSequenceRef.current;
  }, []);

  const isCurrentOverlayRequest = useCallback((requestSequence: number) => (
    overlayRequestSequenceRef.current === requestSequence
  ), []);

  const clearChangesOverlay = useCallback(() => {
    setChangesTask(null);
    setChangesSource(null);
    setOutputChanges(null);
    setOutputDiff(null);
    setSelectedChangePath(null);
  }, []);

  const clearSiblingOverlays = useCallback((active: 'detail' | 'process' | 'run' | 'changes') => {
    if (active !== 'detail') {
      setDetailTask(null);
      setRetryError(null);
    }
    if (active !== 'process') {
      setProcessTask(null);
      setProcessRunDetail(null);
      setLoadingProcessTaskId(null);
    }
    if (active !== 'run') {
      setRunDetail(null);
      setLoadingRunId(null);
    }
    if (active !== 'changes') {
      clearChangesOverlay();
      setLoadingChangesRunId(null);
      setLoadingDiffPath(null);
    }
  }, [clearChangesOverlay]);

  const reloadGraph = useCallback(async (options: ReloadMessageTaskGraphOptions = {}) => {
    const requestIdentity = graphRequestIdentity;
    const requestSequence = graphRequestSequenceRef.current + 1;
    graphRequestSequenceRef.current = requestSequence;
    if (!options.silent) {
      setLoading(true);
    }
    setError(null);
    try {
      const response = await getMessageTaskRunnerGraph(apiClient.getRequestFn(), messageId, lookup);
      if (
        activeGraphRequestIdentityRef.current !== requestIdentity
        || graphRequestSequenceRef.current !== requestSequence
      ) {
        return;
      }
      setGraph({
        root_task_ids: Array.isArray(response.root_task_ids) ? response.root_task_ids : [],
        nodes: Array.isArray(response.nodes) ? response.nodes : [],
        edges: Array.isArray(response.edges) ? response.edges : [],
        source_session_id: response.source_session_id ?? null,
        source_turn_id: response.source_turn_id ?? null,
        source_user_message_id: response.source_user_message_id ?? null,
      });
    } catch (err) {
      if (
        activeGraphRequestIdentityRef.current !== requestIdentity
        || graphRequestSequenceRef.current !== requestSequence
      ) {
        return;
      }
      if (isTransientError?.(err)) {
        setError(null);
        if (!options.silent) {
          setGraph(EMPTY_GRAPH);
        }
        return;
      }
      setError(err instanceof Error ? err.message : '读取任务流程图失败');
      if (!options.silent) {
        setGraph(EMPTY_GRAPH);
      }
    } finally {
      if (
        activeGraphRequestIdentityRef.current === requestIdentity
        && graphRequestSequenceRef.current === requestSequence
      ) {
        setLoading(false);
      }
    }
  }, [apiClient, graphRequestIdentity, isTransientError, lookup, messageId]);

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
    nextOverlayRequestSequence();
    clearSiblingOverlays('detail');
    setRetryError(null);
    setDetailTask(task);
  }, [clearSiblingOverlays, nextOverlayRequestSequence]);

  const openProcessLog = useCallback(async (task: MessageTaskRunnerTask) => {
    const taskId = readString(task.id);
    if (!taskId) {
      return;
    }
    const requestSequence = nextOverlayRequestSequence();
    clearSiblingOverlays('process');
    setLoadingProcessTaskId(taskId);
    setError(null);
    setProcessTask(task);
    setProcessRunDetail(null);
    try {
      const detailSource = buildTaskSourceLookup({
        task,
        graph,
        fallbackMessageId: messageId,
        fallbackLookup: lookup,
      });
      const runId = readString(task.last_run_id);
      if (runId) {
        const detail = await getMessageTaskRunnerGraphRun(
          apiClient.getRequestFn(),
          detailSource.messageId,
          runId,
          {
            ...detailSource.lookup,
            eventLimit: 200,
            eventOffset: 0,
          },
        );
        if (!isCurrentOverlayRequest(requestSequence)) {
          return;
        }
        setProcessTask(detail.task || task);
        setProcessRunDetail(detail);
      } else {
        const detail = await getMessageTaskRunnerTask(
          apiClient.getRequestFn(),
          detailSource.messageId,
          taskId,
          detailSource.lookup,
        );
        if (!isCurrentOverlayRequest(requestSequence)) {
          return;
        }
        setProcessTask(detail);
      }
    } catch (err) {
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setError(err instanceof Error ? err.message : '读取执行过程失败');
    } finally {
      if (isCurrentOverlayRequest(requestSequence)) {
        setLoadingProcessTaskId(null);
      }
    }
  }, [
    apiClient,
    clearSiblingOverlays,
    graph,
    isCurrentOverlayRequest,
    lookup,
    messageId,
    nextOverlayRequestSequence,
  ]);

  const openRun = useCallback(async (task: MessageTaskRunnerTask) => {
    const runId = readString(task.last_run_id);
    if (!runId) {
      return;
    }
    const requestSequence = nextOverlayRequestSequence();
    clearSiblingOverlays('run');
    setLoadingRunId(runId);
    setError(null);
    setRunDetail(null);
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
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setRunDetail(detail);
    } catch (err) {
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setError(err instanceof Error ? err.message : '读取运行详情失败');
    } finally {
      if (isCurrentOverlayRequest(requestSequence)) {
        setLoadingRunId(null);
      }
    }
  }, [
    apiClient,
    clearSiblingOverlays,
    graph,
    isCurrentOverlayRequest,
    lookup,
    messageId,
    nextOverlayRequestSequence,
  ]);

  const retryTask = useCallback(async (
    task: MessageTaskRunnerTask,
    retryInstruction?: string,
  ) => {
    const taskId = readString(task.id);
    const runId = readString(task.last_run_id);
    const status = readString(task.status)?.toLowerCase();
    if (!taskId || !runId || !status || !['failed', 'blocked'].includes(status)) {
      setRetryError('当前任务缺少可重试的运行记录，请刷新任务流程后重试。');
      return false;
    }
    if (retryingTaskId) {
      setRetryError('另一个任务节点正在重新处理，请等待其提交完成后再试。');
      return false;
    }
    const source = buildTaskSourceLookup({
      task,
      graph,
      fallbackMessageId: messageId,
      fallbackLookup: lookup,
    });
    setRetryingTaskId(taskId);
    setRetryError(null);
    setError(null);
    try {
      const response = await retryMessageTaskRunnerRun(
        apiClient.getRequestFn(),
        source.messageId,
        runId,
        source.lookup,
        retryInstruction,
      );
      setDetailTask((current) => (
        current?.id === taskId
          ? {
            ...current,
            status: readString(response.run.status) || 'queued',
            last_run_id: response.run.id,
            last_run: response.run,
            result_summary: null,
          }
          : current
      ));
      await reloadGraph();
      setRetryError(null);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : '重试任务节点失败';
      setRetryError(message);
      setError(message);
      return false;
    } finally {
      setRetryingTaskId(null);
    }
  }, [apiClient, graph, lookup, messageId, reloadGraph, retryingTaskId]);

  const loadChangeDiff = useCallback(async (
    task: MessageTaskRunnerTask,
    source: TaskSourceLookup,
    file: MessageTaskRunnerFileChange,
    requestSequence = overlayRequestSequenceRef.current,
  ) => {
    const runId = readString(task.last_run_id);
    const path = readString(file.path);
    if (!runId || !path) {
      return;
    }
    if (!isCurrentOverlayRequest(requestSequence)) {
      return;
    }
    setSelectedChangePath(path);
    setLoadingDiffPath(path);
    setError(null);
    try {
      const diff = await getMessageTaskRunnerRunOutputDiff(
        apiClient.getRequestFn(),
        source.messageId,
        runId,
        path,
        source.lookup,
      );
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setOutputDiff(diff);
    } catch (err) {
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setOutputDiff(null);
      setError(err instanceof Error ? err.message : '读取文件 diff 失败');
    } finally {
      if (isCurrentOverlayRequest(requestSequence)) {
        setLoadingDiffPath(null);
      }
    }
  }, [apiClient, isCurrentOverlayRequest]);

  const openChanges = useCallback(async (task: MessageTaskRunnerTask) => {
    const runId = readString(task.last_run_id);
    if (!runId) {
      return;
    }
    const requestSequence = nextOverlayRequestSequence();
    clearSiblingOverlays('changes');
    clearChangesOverlay();
    const source = buildTaskSourceLookup({
      task,
      graph,
      fallbackMessageId: messageId,
      fallbackLookup: lookup,
    });
    setChangesTask(task);
    setChangesSource(source);
    setLoadingChangesRunId(runId);
    setLoadingDiffPath(null);
    setError(null);
    try {
      const changes = await getMessageTaskRunnerRunOutputChanges(
        apiClient.getRequestFn(),
        source.messageId,
        runId,
        {
          ...source.lookup,
          limit: 200,
          offset: 0,
        },
      );
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setOutputChanges(changes);
      const firstFile = Array.isArray(changes.files) ? changes.files[0] : null;
      if (firstFile) {
        await loadChangeDiff(task, source, firstFile, requestSequence);
      }
    } catch (err) {
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setError(err instanceof Error ? err.message : '读取文件变更失败');
    } finally {
      if (isCurrentOverlayRequest(requestSequence)) {
        setLoadingChangesRunId(null);
      }
    }
  }, [
    apiClient,
    clearChangesOverlay,
    clearSiblingOverlays,
    graph,
    isCurrentOverlayRequest,
    loadChangeDiff,
    lookup,
    messageId,
    nextOverlayRequestSequence,
  ]);

  const selectChangeFile = useCallback(async (file: MessageTaskRunnerFileChange) => {
    if (!changesTask || !changesSource) {
      return;
    }
    const requestSequence = nextOverlayRequestSequence();
    await loadChangeDiff(changesTask, changesSource, file, requestSequence);
  }, [changesSource, changesTask, loadChangeDiff, nextOverlayRequestSequence]);

  const loadMoreRunEvents = useCallback(async () => {
    if (!runDetail?.events_has_more) {
      return;
    }
    const requestSequence = overlayRequestSequenceRef.current;
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
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setRunDetail((current) => (current ? mergeRunEventPage(current, detail) : detail));
    } catch (err) {
      if (!isCurrentOverlayRequest(requestSequence)) {
        return;
      }
      setError(err instanceof Error ? err.message : '读取更多运行事件失败');
    } finally {
      if (isCurrentOverlayRequest(requestSequence)) {
        setLoadingRunId(null);
      }
    }
  }, [
    apiClient,
    graph,
    isCurrentOverlayRequest,
    loadingRunId,
    lookup,
    messageId,
    runDetail,
  ]);

  useEffect(() => {
    if (!open) {
      return;
    }
    void reloadGraph();
  }, [open, reloadGraph]);

  useEffect(() => {
    if (!open) {
      nextOverlayRequestSequence();
      setDetailTask(null);
      setProcessTask(null);
      setProcessRunDetail(null);
      setRunDetail(null);
      setChangesTask(null);
      setChangesSource(null);
      setOutputChanges(null);
      setOutputDiff(null);
      setSelectedChangePath(null);
      setRetryingTaskId(null);
      setRetryError(null);
      setError(null);
    }
  }, [nextOverlayRequestSequence, open]);

  useEffect(() => {
    nextOverlayRequestSequence();
    setDetailTask(null);
    setProcessTask(null);
    setProcessRunDetail(null);
    setRunDetail(null);
    clearChangesOverlay();
    setLoadingProcessTaskId(null);
    setLoadingRunId(null);
    setLoadingChangesRunId(null);
    setLoadingDiffPath(null);
    setRetryingTaskId(null);
    setRetryError(null);
    setError(null);
  }, [clearChangesOverlay, graphRequestIdentity, nextOverlayRequestSequence]);

  return {
    graph,
    rootTasks,
    allTasks,
    sourceUserMessageId,
    loading,
    error,
    detailTask,
    processTask,
    processRunDetail,
    loadingProcessTaskId,
    runDetail,
    changesTask,
    outputChanges,
    outputDiff,
    selectedChangePath,
    loadingRunId,
    loadingChangesRunId,
    loadingDiffPath,
    retryingTaskId,
    retryError,
    reloadGraph,
    openDetail,
    openProcessLog,
    openRun,
    openChanges,
    retryTask,
    selectChangeFile,
    loadMoreRunEvents,
    closeDetail: () => {
      nextOverlayRequestSequence();
      setDetailTask(null);
      setRetryError(null);
    },
    closeProcessLog: () => {
      nextOverlayRequestSequence();
      setProcessTask(null);
      setProcessRunDetail(null);
      setLoadingProcessTaskId(null);
    },
    closeRun: () => {
      nextOverlayRequestSequence();
      setRunDetail(null);
      setLoadingRunId(null);
    },
    closeChanges: () => {
      nextOverlayRequestSequence();
      clearChangesOverlay();
      setLoadingChangesRunId(null);
      setLoadingDiffPath(null);
    },
  };
}

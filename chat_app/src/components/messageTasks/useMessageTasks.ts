import { useCallback, useEffect, useState } from 'react';
import { useApiClient } from '../../lib/api/ApiClientContext';
import {
  getMessageTaskRunnerRun,
  getMessageTaskRunnerTask,
  getMessageTaskRunnerTasks,
} from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { readString } from './utils';

interface UseMessageTasksArgs {
  open: boolean;
  messageId: string;
}

export function useMessageTasks({ open, messageId }: UseMessageTasksArgs) {
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
      const response = await getMessageTaskRunnerTasks(apiClient.getRequestFn(), messageId);
      setTasks(Array.isArray(response.items) ? response.items : []);
      setSourceUserMessageId(readString(response.source_user_message_id));
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取任务失败');
      setTasks([]);
      setSourceUserMessageId(null);
    } finally {
      setLoading(false);
    }
  }, [apiClient, messageId]);

  const openDetail = useCallback(async (task: MessageTaskRunnerTask) => {
    setLoadingDetailId(task.id);
    setError(null);
    try {
      const detail = await getMessageTaskRunnerTask(apiClient.getRequestFn(), messageId, task.id);
      setDetailTask(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取任务详情失败');
    } finally {
      setLoadingDetailId(null);
    }
  }, [apiClient, messageId]);

  const openRun = useCallback(async (task: MessageTaskRunnerTask) => {
    const runId = readString(task.last_run_id);
    if (!runId) {
      return;
    }
    setLoadingRunId(runId);
    setError(null);
    try {
      const detail = await getMessageTaskRunnerRun(apiClient.getRequestFn(), messageId, runId);
      setRunDetail(detail);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行详情失败');
    } finally {
      setLoadingRunId(null);
    }
  }, [apiClient, messageId]);

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
    closeDetail: () => setDetailTask(null),
    closeRun: () => setRunDetail(null),
  };
}

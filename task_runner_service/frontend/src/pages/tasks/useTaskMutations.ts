import { useCallback } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { MessageInstance } from 'antd/es/message/interface';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  BatchTaskOperationResponse,
  CreateTaskPayload,
  StartTaskRunPayload,
} from '../../types';

type UseTaskMutationsParams = {
  t: TranslateFn;
  messageApi: MessageInstance;
  onTaskSaved: () => void;
  onRunStarted: () => void;
  onBatchRunStarted: () => void;
  onClearSelectedTasks: () => void;
};

export function useTaskMutations({
  t,
  messageApi,
  onTaskSaved,
  onRunStarted,
  onBatchRunStarted,
  onClearSelectedTasks,
}: UseTaskMutationsParams) {
  const queryClient = useQueryClient();

  const invalidateTaskQueries = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['tasks'] }),
      queryClient.invalidateQueries({ queryKey: ['task-index'] }),
      queryClient.invalidateQueries({ queryKey: ['task-stats'] }),
      queryClient.invalidateQueries({ queryKey: ['task'] }),
      queryClient.invalidateQueries({ queryKey: ['task-recent-runs'] }),
      queryClient.invalidateQueries({ queryKey: ['task-prompts'] }),
      queryClient.invalidateQueries({ queryKey: ['runs'] }),
      queryClient.invalidateQueries({ queryKey: ['run-index'] }),
      queryClient.invalidateQueries({ queryKey: ['task-list-last-run-events'] }),
      queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
      queryClient.invalidateQueries({ queryKey: ['prompts'] }),
      queryClient.invalidateQueries({ queryKey: ['prompt-task-counts'] }),
    ]);
  }, [queryClient]);

  const showBatchOperationResult = useCallback(
    (action: string, result: BatchTaskOperationResponse) => {
      const failedItems = result.results.filter((item) => !item.ok);
      const summary = t('tasks.batchSummary', {
        action,
        succeeded: result.succeeded,
        total: result.total,
      });
      if (!failedItems.length) {
        messageApi.success(summary);
        return;
      }

      const detail = failedItems
        .slice(0, 3)
        .map(
          (item) =>
            `${item.task_id.slice(0, 8)}: ${item.message || t('tasks.batchFailedFallback')}`,
        )
        .join('；');
      const messageText = t('tasks.batchMessage', {
        summary,
        failed: result.failed,
        detail: detail ? t('tasks.batchDetailPrefix', { detail }) : '',
      });
      if (result.succeeded > 0) {
        messageApi.warning(messageText);
      } else {
        messageApi.error(messageText);
      }
    },
    [messageApi, t],
  );

  const createTaskMutation = useMutation({
    mutationFn: api.createTask,
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.created'));
      onTaskSaved();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const updateTaskMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: Partial<CreateTaskPayload> }) =>
      api.updateTask(id, payload),
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.updated'));
      onTaskSaved();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const deleteTaskMutation = useMutation({
    mutationFn: api.deleteTask,
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.deleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const runTaskMutation = useMutation({
    mutationFn: ({ taskId, payload }: { taskId: string; payload: StartTaskRunPayload }) =>
      api.startTaskRun(taskId, payload),
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.started'));
      onRunStarted();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const batchUpdateTaskStatusMutation = useMutation({
    mutationFn: api.batchUpdateTaskStatus,
    onSuccess: async (result, payload) => {
      await invalidateTaskQueries();
      onClearSelectedTasks();
      showBatchOperationResult(t('tasks.batchUpdateAction', { status: payload.status }), result);
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const batchDeleteTasksMutation = useMutation({
    mutationFn: api.batchDeleteTasks,
    onSuccess: async (result) => {
      await invalidateTaskQueries();
      onClearSelectedTasks();
      showBatchOperationResult(t('tasks.batchDeleteAction'), result);
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const batchStartTaskRunsMutation = useMutation({
    mutationFn: api.batchStartTaskRuns,
    onSuccess: async (result) => {
      await invalidateTaskQueries();
      onClearSelectedTasks();
      onBatchRunStarted();
      showBatchOperationResult(t('tasks.batchRunAction'), result);
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const summarizeTaskMemoryMutation = useMutation({
    mutationFn: api.summarizeTaskMemory,
    onSuccess: async (_, taskId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-memory-context', taskId] }),
        queryClient.invalidateQueries({ queryKey: ['task-memory-records', taskId] }),
      ]);
      messageApi.success(t('tasks.memorySummarizeStarted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const draftMcpPreviewMutation = useMutation({
    mutationFn: api.previewMcpPrompt,
    onError: (error: Error) => messageApi.error(error.message),
  });

  return {
    createTaskMutation,
    updateTaskMutation,
    deleteTaskMutation,
    runTaskMutation,
    batchUpdateTaskStatusMutation,
    batchDeleteTasksMutation,
    batchStartTaskRunsMutation,
    summarizeTaskMemoryMutation,
    draftMcpPreviewMutation,
  };
}

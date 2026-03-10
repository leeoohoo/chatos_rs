import { useCallback, useState } from 'react';
import type { SessionSummaryWorkbarItem, TaskWorkbarItem } from '../TaskWorkbar';

interface UseWorkbarMutationsArgs {
  apiClient: any;
  currentSessionId: string | null;
  workbarSummariesLength: number;
  refreshWorkbarTasks: () => Promise<void>;
  loadWorkbarSummaries: (sessionId: string, force?: boolean) => Promise<void>;
  setWorkbarError: (value: string | null) => void;
  setWorkbarSummariesError: (value: string | null) => void;
}

export function useWorkbarMutations({
  apiClient,
  currentSessionId,
  workbarSummariesLength,
  refreshWorkbarTasks,
  loadWorkbarSummaries,
  setWorkbarError,
  setWorkbarSummariesError,
}: UseWorkbarMutationsArgs) {
  const [workbarActionLoadingTaskId, setWorkbarActionLoadingTaskId] = useState<string | null>(null);
  const [workbarSummaryActionLoadingKey, setWorkbarSummaryActionLoadingKey] = useState<string | null>(null);

  const withWorkbarTaskMutation = useCallback(async (taskId: string, action: () => Promise<void>) => {
    setWorkbarActionLoadingTaskId(taskId);
    setWorkbarError(null);
    try {
      await action();
      await refreshWorkbarTasks();
    } catch (error) {
      setWorkbarError(error instanceof Error ? error.message : '任务操作失败');
    } finally {
      setWorkbarActionLoadingTaskId(null);
    }
  }, [refreshWorkbarTasks, setWorkbarError]);

  const withWorkbarSummaryMutation = useCallback(async (
    sessionId: string,
    actionKey: string,
    action: () => Promise<void>
  ) => {
    if (!sessionId) {
      return;
    }

    setWorkbarSummaryActionLoadingKey(actionKey);
    setWorkbarSummariesError(null);
    try {
      await action();
      await loadWorkbarSummaries(sessionId, true);
    } catch (error) {
      setWorkbarSummariesError(error instanceof Error ? error.message : '会话总结操作失败');
    } finally {
      setWorkbarSummaryActionLoadingKey(null);
    }
  }, [loadWorkbarSummaries, setWorkbarSummariesError]);

  const handleWorkbarCompleteTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSessionId) {
      return;
    }
    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.completeTaskManagerTask(currentSessionId, task.id);
    });
  }, [apiClient, currentSessionId, withWorkbarTaskMutation]);

  const handleWorkbarDeleteTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSessionId) {
      return;
    }
    if (typeof window !== 'undefined') {
      const confirmed = window.confirm('Delete task "' + task.title + '"?');
      if (!confirmed) {
        return;
      }
    }

    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.deleteTaskManagerTask(currentSessionId, task.id);
    });
  }, [apiClient, currentSessionId, withWorkbarTaskMutation]);

  const handleWorkbarEditTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSessionId || typeof window === 'undefined') {
      return;
    }

    const nextTitleRaw = window.prompt('Task title', task.title);
    if (nextTitleRaw === null) {
      return;
    }
    const nextDetailsRaw = window.prompt('Task details (optional)', task.details || '');
    if (nextDetailsRaw === null) {
      return;
    }
    const nextPriorityRaw = window.prompt('Priority (high/medium/low)', task.priority);
    if (nextPriorityRaw === null) {
      return;
    }
    const nextStatusRaw = window.prompt('Status (todo/doing/blocked/done)', task.status);
    if (nextStatusRaw === null) {
      return;
    }
    const nextDueAtRaw = window.prompt('Due time (empty string to clear)', task.dueAt || '');
    if (nextDueAtRaw === null) {
      return;
    }

    const allowedPriority: Array<TaskWorkbarItem['priority']> = ['high', 'medium', 'low'];
    const allowedStatus: Array<TaskWorkbarItem['status']> = ['todo', 'doing', 'blocked', 'done'];
    const nextPriority = nextPriorityRaw.trim().toLowerCase() as TaskWorkbarItem['priority'];
    const nextStatus = nextStatusRaw.trim().toLowerCase() as TaskWorkbarItem['status'];

    if (!allowedPriority.includes(nextPriority)) {
      setWorkbarError('Priority must be high / medium / low');
      return;
    }
    if (!allowedStatus.includes(nextStatus)) {
      setWorkbarError('Status must be todo / doing / blocked / done');
      return;
    }

    const nextTitle = nextTitleRaw.trim();
    const nextDetails = nextDetailsRaw.trim();
    const nextDueAt = nextDueAtRaw.trim();

    const payload: {
      title?: string;
      details?: string;
      priority?: TaskWorkbarItem['priority'];
      status?: TaskWorkbarItem['status'];
      due_at?: string | null;
    } = {};

    if (nextTitle && nextTitle !== task.title) {
      payload.title = nextTitle;
    }
    if (nextDetails !== task.details) {
      payload.details = nextDetails;
    }
    if (nextPriority !== task.priority) {
      payload.priority = nextPriority;
    }
    if (nextStatus !== task.status) {
      payload.status = nextStatus;
    }

    const currentDueAt = (task.dueAt || '').trim();
    if (nextDueAt !== currentDueAt) {
      payload.due_at = nextDueAt || null;
    }

    if (Object.keys(payload).length === 0) {
      return;
    }

    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.updateTaskManagerTask(currentSessionId, task.id, payload);
    });
  }, [apiClient, currentSessionId, setWorkbarError, withWorkbarTaskMutation]);

  const handleDeleteWorkbarSummary = useCallback(async (summary: SessionSummaryWorkbarItem) => {
    if (!currentSessionId) {
      return;
    }
    if (typeof window !== 'undefined') {
      const confirmed = window.confirm('确认删除这条会话总结吗？相关消息会重新进入待总结队列。');
      if (!confirmed) {
        return;
      }
    }

    await withWorkbarSummaryMutation(currentSessionId, `delete:${summary.id}`, async () => {
      await apiClient.deleteSessionSummary(currentSessionId, summary.id);
    });
  }, [apiClient, currentSessionId, withWorkbarSummaryMutation]);

  const handleClearWorkbarSummaries = useCallback(async () => {
    if (!currentSessionId || workbarSummariesLength === 0) {
      return;
    }
    if (typeof window !== 'undefined') {
      const confirmed = window.confirm('确认清空当前会话的所有总结吗？相关消息会重新进入待总结队列。');
      if (!confirmed) {
        return;
      }
    }

    await withWorkbarSummaryMutation(currentSessionId, 'clear-all', async () => {
      await apiClient.clearSessionSummaries(currentSessionId);
    });
  }, [apiClient, currentSessionId, withWorkbarSummaryMutation, workbarSummariesLength]);

  return {
    workbarActionLoadingTaskId,
    workbarSummaryActionLoadingKey,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    handleDeleteWorkbarSummary,
    handleClearWorkbarSummaries,
  };
}

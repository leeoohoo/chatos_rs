import { useCallback, useState } from 'react';
import type { TaskWorkbarItem } from '../TaskWorkbar';
import type { TaskOutcomeDraft } from '../taskWorkbar/TaskOutcomeModal';
import { useDialogService } from '../ui/DialogProvider';
import type { TaskManagerTaskResponse } from '../../lib/api/client/types/runtime';
import {
  applyLocalTaskMutationResult,
  buildRealtimeMutationHandledPayload,
  buildTaskUpdatePayload,
  type TaskModalMode,
  validateTaskModalDraft,
  type WorkbarMutationResult,
} from './workbarMutationHelpers';

interface WorkbarMutationApiClient {
  completeTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: {
      outcome_summary?: string;
      outcome_items?: Array<{
        kind?: string;
        text?: string;
        importance?: 'high' | 'medium' | 'low';
        refs?: string[];
      }>;
      resume_hint?: string;
    },
  ) => Promise<TaskManagerTaskResponse>;
  deleteTaskManagerTask: (sessionId: string, taskId: string) => Promise<{ success?: boolean }>;
  updateTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload: {
      title?: string;
      details?: string;
      priority?: TaskWorkbarItem['priority'];
      status?: TaskWorkbarItem['status'];
      due_at?: string | null;
      outcome_summary?: string;
      resume_hint?: string;
      blocker_reason?: string;
      blocker_needs?: string[];
      blocker_kind?: string;
    },
  ) => Promise<TaskManagerTaskResponse>;
}

interface UseWorkbarMutationsArgs {
  apiClient: WorkbarMutationApiClient;
  currentSessionId: string | null;
  currentConversationTurnId?: string | null;
  preferRealtimeSync?: boolean;
  taskHistoryOpen?: boolean;
  loadCurrentTurnWorkbarTasks: (
    sessionId: string,
    conversationTurnId?: string | null,
    force?: boolean,
  ) => Promise<void>;
  loadHistoryWorkbarTasks: (sessionId: string, force?: boolean) => Promise<void>;
  markHistoryWorkbarTasksStale: (sessionId: string) => void;
  patchCurrentTurnWorkbarTask: (sessionId: string, task: TaskWorkbarItem) => boolean;
  removeCurrentTurnWorkbarTask: (sessionId: string, taskId: string) => boolean;
  patchHistoryWorkbarTask: (sessionId: string, task: TaskWorkbarItem) => boolean;
  removeHistoryWorkbarTask: (sessionId: string, taskId: string) => boolean;
  markTaskRealtimeMutationHandled?: (payload: {
    action: string;
    taskId?: string | null;
    turnId?: string | null;
  }) => void;
  setWorkbarError: (value: string | null) => void;
}

export function useWorkbarMutations({
  apiClient,
  currentSessionId,
  currentConversationTurnId = null,
  preferRealtimeSync = false,
  taskHistoryOpen = false,
  loadCurrentTurnWorkbarTasks,
  loadHistoryWorkbarTasks,
  markHistoryWorkbarTasksStale,
  patchCurrentTurnWorkbarTask,
  removeCurrentTurnWorkbarTask,
  patchHistoryWorkbarTask,
  removeHistoryWorkbarTask,
  markTaskRealtimeMutationHandled,
  setWorkbarError,
}: UseWorkbarMutationsArgs) {
  const { confirm } = useDialogService();
  const [workbarActionLoadingTaskId, setWorkbarActionLoadingTaskId] = useState<string | null>(null);
  const [taskModalOpen, setTaskModalOpen] = useState(false);
  const [taskModalMode, setTaskModalMode] = useState<TaskModalMode>('edit');
  const [taskModalTask, setTaskModalTask] = useState<TaskWorkbarItem | null>(null);
  const [taskModalError, setTaskModalError] = useState<string | null>(null);

  const withWorkbarTaskMutation = useCallback(async (
    taskId: string,
    action: () => Promise<WorkbarMutationResult | void>,
  ) => {
    setWorkbarActionLoadingTaskId(taskId);
    setWorkbarError(null);
    setTaskModalError(null);
    try {
      const result = await action();
      if (currentSessionId) {
        await applyLocalTaskMutationResult({
          currentConversationTurnId,
          loadCurrentTurnWorkbarTasks,
          loadHistoryWorkbarTasks,
          markHistoryWorkbarTasksStale,
          patchCurrentTurnWorkbarTask,
          patchHistoryWorkbarTask,
          preferRealtimeSync,
          removeCurrentTurnWorkbarTask,
          removeHistoryWorkbarTask,
          result,
          sessionId: currentSessionId,
          taskHistoryOpen,
        });
      }
      if (preferRealtimeSync && currentSessionId) {
        const handledPayload = buildRealtimeMutationHandledPayload({
          currentConversationTurnId,
          result,
        });
        if (handledPayload) {
          markTaskRealtimeMutationHandled?.(handledPayload);
        }
      }
      setTaskModalOpen(false);
      setTaskModalTask(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务操作失败';
      setTaskModalError(message);
      setWorkbarError(message);
    } finally {
      setWorkbarActionLoadingTaskId(null);
    }
  }, [
    currentSessionId,
    applyLocalTaskMutationResult,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    patchCurrentTurnWorkbarTask,
    patchHistoryWorkbarTask,
    preferRealtimeSync,
    markTaskRealtimeMutationHandled,
    removeCurrentTurnWorkbarTask,
    removeHistoryWorkbarTask,
    setWorkbarError,
    taskHistoryOpen,
  ]);

  const closeTaskModal = useCallback(() => {
    if (workbarActionLoadingTaskId) {
      return;
    }
    setTaskModalOpen(false);
    setTaskModalTask(null);
    setTaskModalError(null);
  }, [workbarActionLoadingTaskId]);

  const handleWorkbarCompleteTask = useCallback(async (task: TaskWorkbarItem) => {
    setTaskModalMode('complete');
    setTaskModalTask(task);
    setTaskModalError(null);
    setTaskModalOpen(true);
  }, []);

  const handleWorkbarDeleteTask = useCallback(async (task: TaskWorkbarItem) => {
    if (!currentSessionId) {
      return;
    }
    const confirmed = await confirm({
      title: 'Delete Task',
      message: `Delete task "${task.title}"?`,
      confirmText: 'Delete',
      cancelText: 'Cancel',
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }

    await withWorkbarTaskMutation(task.id, async () => {
      await apiClient.deleteTaskManagerTask(currentSessionId, task.id);
      return {
        removeTaskId: task.id,
      };
    });
  }, [apiClient, confirm, currentSessionId, withWorkbarTaskMutation]);

  const handleWorkbarEditTask = useCallback(async (task: TaskWorkbarItem) => {
    setTaskModalMode('edit');
    setTaskModalTask(task);
    setTaskModalError(null);
    setTaskModalOpen(true);
  }, []);

  const submitTaskModal = useCallback(async (draft: TaskOutcomeDraft) => {
    if (!currentSessionId || !taskModalTask) {
      return;
    }

    const validationMessage = validateTaskModalDraft({
      draft,
      mode: taskModalMode,
    });
    if (validationMessage) {
      setTaskModalError(validationMessage);
      setWorkbarError(validationMessage);
      return;
    }

    if (taskModalMode === 'complete') {
      const payload = buildTaskUpdatePayload({
        draft,
        task: taskModalTask,
      });
      await withWorkbarTaskMutation(taskModalTask.id, async () => {
        const completedTask = await apiClient.completeTaskManagerTask(currentSessionId, taskModalTask.id, {
          outcome_summary: payload.outcome_summary,
          resume_hint: payload.resume_hint,
        });
        return {
          patchTask: completedTask,
        };
      });
      return;
    }

    const payload = buildTaskUpdatePayload({
      draft,
      task: taskModalTask,
    });

    if (Object.keys(payload).length === 0) {
      closeTaskModal();
      return;
    }
    await withWorkbarTaskMutation(taskModalTask.id, async () => {
      const updatedTask = await apiClient.updateTaskManagerTask(currentSessionId, taskModalTask.id, payload);
      return {
        patchTask: updatedTask,
      };
    });
  }, [apiClient, closeTaskModal, currentSessionId, setWorkbarError, taskModalMode, taskModalTask, withWorkbarTaskMutation]);

  return {
    workbarActionLoadingTaskId,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    taskModalError,
    taskModalMode,
    taskModalOpen,
    taskModalTask,
    closeTaskModal,
    submitTaskModal,
  };
}

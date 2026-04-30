import { useCallback, useState } from 'react';
import type { TaskWorkbarItem } from '../TaskWorkbar';
import type { TaskOutcomeDraft } from '../taskWorkbar/TaskOutcomeModal';
import { useDialogService } from '../ui/DialogProvider';
import { normalizeWorkbarTask } from './helpers';
import type { TaskManagerTaskResponse } from '../../lib/api/client/types/runtime';

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

type TaskModalMode = 'complete' | 'edit';
type WorkbarMutationResult = {
  patchTask?: TaskManagerTaskResponse | null;
  removeTaskId?: string | null;
};

const normalizeNeeds = (raw: string): string[] => raw
  .split(/\r?\n|[;；]/)
  .map((item) => item.trim())
  .filter((item) => item.length > 0);

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

  const applyLocalTaskMutationResult = useCallback(async (
    sessionId: string,
    result: WorkbarMutationResult | void,
  ): Promise<void> => {
    const patchedTask = result?.patchTask
      ? normalizeWorkbarTask(result.patchTask)
      : null;
    const removedTaskId = typeof result?.removeTaskId === 'string'
      ? result.removeTaskId.trim()
      : '';

    if (patchedTask) {
      const currentTurnPatched = patchCurrentTurnWorkbarTask(sessionId, patchedTask);
      if (!currentTurnPatched && !preferRealtimeSync) {
        await loadCurrentTurnWorkbarTasks(sessionId, currentConversationTurnId, true);
      }
      if (taskHistoryOpen) {
        const historyPatched = patchHistoryWorkbarTask(sessionId, patchedTask);
        if (!historyPatched && !preferRealtimeSync) {
          await loadHistoryWorkbarTasks(sessionId, true);
        }
      } else {
        markHistoryWorkbarTasksStale(sessionId);
      }
      return;
    }

    if (removedTaskId) {
      const currentTurnPatched = removeCurrentTurnWorkbarTask(sessionId, removedTaskId);
      if (!currentTurnPatched && !preferRealtimeSync) {
        await loadCurrentTurnWorkbarTasks(sessionId, currentConversationTurnId, true);
      }
      if (taskHistoryOpen) {
        const historyPatched = removeHistoryWorkbarTask(sessionId, removedTaskId);
        if (!historyPatched && !preferRealtimeSync) {
          await loadHistoryWorkbarTasks(sessionId, true);
        }
      } else {
        markHistoryWorkbarTasksStale(sessionId);
      }
      return;
    }

    if (!preferRealtimeSync) {
      await loadCurrentTurnWorkbarTasks(sessionId, currentConversationTurnId, true);
      if (taskHistoryOpen) {
        await loadHistoryWorkbarTasks(sessionId, true);
      } else {
        markHistoryWorkbarTasksStale(sessionId);
      }
    }
  }, [
    currentConversationTurnId,
    loadCurrentTurnWorkbarTasks,
    loadHistoryWorkbarTasks,
    markHistoryWorkbarTasksStale,
    patchCurrentTurnWorkbarTask,
    patchHistoryWorkbarTask,
    preferRealtimeSync,
    removeCurrentTurnWorkbarTask,
    removeHistoryWorkbarTask,
    taskHistoryOpen,
  ]);

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
        await applyLocalTaskMutationResult(currentSessionId, result);
      }
      if (preferRealtimeSync && currentSessionId) {
        const taskIdForGuard = typeof result?.removeTaskId === 'string'
          ? result.removeTaskId.trim()
          : String(result?.patchTask?.id || '').trim();
        const turnIdForGuard = String(
          result?.patchTask?.conversation_turn_id || currentConversationTurnId || '',
        ).trim();
        const actionForGuard = result?.removeTaskId
          ? 'task_deleted'
          : (result?.patchTask ? 'task_updated' : '');
        if (actionForGuard && taskIdForGuard) {
          markTaskRealtimeMutationHandled?.({
            action: actionForGuard,
            taskId: taskIdForGuard,
            turnId: turnIdForGuard,
          });
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

    const nextTitle = draft.title.trim();
    const nextDetails = draft.details.trim();
    const nextDueAt = draft.dueAt.trim();
    const nextOutcomeSummary = draft.outcomeSummary.trim();
    const nextResumeHint = draft.resumeHint.trim();
    const nextBlockerReason = draft.blockerReason.trim();
    const nextBlockerNeeds = normalizeNeeds(draft.blockerNeedsText);
    const nextBlockerKind = (draft.blockerKind || 'unknown').trim() || 'unknown';

    if (taskModalMode === 'complete' && !nextOutcomeSummary) {
      const message = '完成任务时必须填写成果摘要';
      setTaskModalError(message);
      setWorkbarError(message);
      return;
    }
    if (draft.status === 'blocked') {
      if (!nextOutcomeSummary) {
        const message = '阻塞任务必须填写已完成尝试或成果摘要';
        setTaskModalError(message);
        setWorkbarError(message);
        return;
      }
      if (!nextBlockerReason) {
        const message = '阻塞任务必须填写阻塞原因';
        setTaskModalError(message);
        setWorkbarError(message);
        return;
      }
    }

    if (taskModalMode === 'complete') {
      await withWorkbarTaskMutation(taskModalTask.id, async () => {
        const completedTask = await apiClient.completeTaskManagerTask(currentSessionId, taskModalTask.id, {
          outcome_summary: nextOutcomeSummary,
          resume_hint: nextResumeHint,
        });
        return {
          patchTask: completedTask,
        };
      });
      return;
    }

    const payload: {
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
    } = {};

    if (nextTitle && nextTitle !== taskModalTask.title) {
      payload.title = nextTitle;
    }
    if (nextDetails !== taskModalTask.details) {
      payload.details = nextDetails;
    }
    if (draft.priority !== taskModalTask.priority) {
      payload.priority = draft.priority;
    }
    if (draft.status !== taskModalTask.status) {
      payload.status = draft.status;
    }
    if (nextDueAt !== (taskModalTask.dueAt || '').trim()) {
      payload.due_at = nextDueAt || null;
    }
    if (nextOutcomeSummary !== taskModalTask.outcomeSummary.trim()) {
      payload.outcome_summary = nextOutcomeSummary;
    }
    if (nextResumeHint !== taskModalTask.resumeHint.trim()) {
      payload.resume_hint = nextResumeHint;
    }

    if (draft.status === 'blocked') {
      if (nextBlockerReason !== taskModalTask.blockerReason.trim()) {
        payload.blocker_reason = nextBlockerReason;
      }
      if (JSON.stringify(nextBlockerNeeds) !== JSON.stringify(taskModalTask.blockerNeeds)) {
        payload.blocker_needs = nextBlockerNeeds;
      }
      if (nextBlockerKind !== (taskModalTask.blockerKind || '')) {
        payload.blocker_kind = nextBlockerKind;
      }
    } else if (taskModalTask.blockerReason || taskModalTask.blockerNeeds.length > 0 || taskModalTask.blockerKind) {
      payload.blocker_reason = '';
      payload.blocker_needs = [];
      payload.blocker_kind = '';
    }

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

import { useCallback, useState } from 'react';
import type { TaskWorkbarItem } from '../TaskWorkbar';
import type { TaskOutcomeDraft } from '../taskWorkbar/TaskOutcomeModal';

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
  ) => Promise<unknown>;
  deleteTaskManagerTask: (sessionId: string, taskId: string) => Promise<unknown>;
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
  ) => Promise<unknown>;
}

interface UseWorkbarMutationsArgs {
  apiClient: WorkbarMutationApiClient;
  currentSessionId: string | null;
  refreshWorkbarTasks: () => Promise<void>;
  setWorkbarError: (value: string | null) => void;
}

type TaskModalMode = 'complete' | 'edit';

const normalizeNeeds = (raw: string): string[] => raw
  .split(/\r?\n|[;；]/)
  .map((item) => item.trim())
  .filter((item) => item.length > 0);

export function useWorkbarMutations({
  apiClient,
  currentSessionId,
  refreshWorkbarTasks,
  setWorkbarError,
}: UseWorkbarMutationsArgs) {
  const [workbarActionLoadingTaskId, setWorkbarActionLoadingTaskId] = useState<string | null>(null);
  const [taskModalOpen, setTaskModalOpen] = useState(false);
  const [taskModalMode, setTaskModalMode] = useState<TaskModalMode>('edit');
  const [taskModalTask, setTaskModalTask] = useState<TaskWorkbarItem | null>(null);
  const [taskModalError, setTaskModalError] = useState<string | null>(null);

  const withWorkbarTaskMutation = useCallback(async (taskId: string, action: () => Promise<void>) => {
    setWorkbarActionLoadingTaskId(taskId);
    setWorkbarError(null);
    setTaskModalError(null);
    try {
      await action();
      await refreshWorkbarTasks();
      setTaskModalOpen(false);
      setTaskModalTask(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务操作失败';
      setTaskModalError(message);
      setWorkbarError(message);
    } finally {
      setWorkbarActionLoadingTaskId(null);
    }
  }, [refreshWorkbarTasks, setWorkbarError]);

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
        await apiClient.completeTaskManagerTask(currentSessionId, taskModalTask.id, {
          outcome_summary: nextOutcomeSummary,
          resume_hint: nextResumeHint,
        });
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
      await apiClient.updateTaskManagerTask(currentSessionId, taskModalTask.id, payload);
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

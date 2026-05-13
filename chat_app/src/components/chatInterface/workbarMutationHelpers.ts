import type { TaskManagerTaskResponse } from '../../lib/api/client/types/runtime';
import type { TaskOutcomeDraft } from '../taskWorkbar/TaskOutcomeModal';
import type { TaskWorkbarItem } from '../TaskWorkbar';
import { normalizeWorkbarTask } from './workbarTransforms';

export type TaskModalMode = 'complete' | 'edit';

export type WorkbarMutationResult = {
  patchTask?: TaskManagerTaskResponse | null;
  removeTaskId?: string | null;
};

interface ApplyLocalTaskMutationResultArgs {
  currentConversationTurnId?: string | null;
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
  preferRealtimeSync?: boolean;
  result: WorkbarMutationResult | void;
  sessionId: string;
  taskHistoryOpen?: boolean;
}

interface ValidateTaskModalDraftArgs {
  draft: TaskOutcomeDraft;
  mode: TaskModalMode;
}

interface BuildTaskUpdatePayloadArgs {
  draft: TaskOutcomeDraft;
  task: TaskWorkbarItem;
}

type WorkbarTaskUpdatePayload = {
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
};

export interface NormalizedTaskModalDraft {
  title: string;
  details: string;
  dueAt: string;
  outcomeSummary: string;
  resumeHint: string;
  blockerReason: string;
  blockerNeeds: string[];
  blockerKind: string;
}

const normalizeNeeds = (raw: string): string[] => raw
  .split(/\r?\n|[;；]/)
  .map((item) => item.trim())
  .filter((item) => item.length > 0);

export const normalizeTaskModalDraft = (
  draft: TaskOutcomeDraft,
): NormalizedTaskModalDraft => ({
  title: draft.title.trim(),
  details: draft.details.trim(),
  dueAt: draft.dueAt.trim(),
  outcomeSummary: draft.outcomeSummary.trim(),
  resumeHint: draft.resumeHint.trim(),
  blockerReason: draft.blockerReason.trim(),
  blockerNeeds: normalizeNeeds(draft.blockerNeedsText),
  blockerKind: (draft.blockerKind || 'unknown').trim() || 'unknown',
});

export const validateTaskModalDraft = ({
  draft,
  mode,
}: ValidateTaskModalDraftArgs): string | null => {
  const normalized = normalizeTaskModalDraft(draft);
  if (mode === 'complete' && !normalized.outcomeSummary) {
    return '完成任务时必须填写成果摘要';
  }
  if (draft.status === 'blocked') {
    if (!normalized.outcomeSummary) {
      return '阻塞任务必须填写已完成尝试或成果摘要';
    }
    if (!normalized.blockerReason) {
      return '阻塞任务必须填写阻塞原因';
    }
  }
  return null;
};

export const buildTaskUpdatePayload = ({
  draft,
  task,
}: BuildTaskUpdatePayloadArgs): WorkbarTaskUpdatePayload => {
  const normalized = normalizeTaskModalDraft(draft);
  const payload: WorkbarTaskUpdatePayload = {};

  if (normalized.title && normalized.title !== task.title) {
    payload.title = normalized.title;
  }
  if (normalized.details !== task.details) {
    payload.details = normalized.details;
  }
  if (draft.priority !== task.priority) {
    payload.priority = draft.priority;
  }
  if (draft.status !== task.status) {
    payload.status = draft.status;
  }
  if (normalized.dueAt !== (task.dueAt || '').trim()) {
    payload.due_at = normalized.dueAt || null;
  }
  if (normalized.outcomeSummary !== task.outcomeSummary.trim()) {
    payload.outcome_summary = normalized.outcomeSummary;
  }
  if (normalized.resumeHint !== task.resumeHint.trim()) {
    payload.resume_hint = normalized.resumeHint;
  }

  if (draft.status === 'blocked') {
    if (normalized.blockerReason !== task.blockerReason.trim()) {
      payload.blocker_reason = normalized.blockerReason;
    }
    if (JSON.stringify(normalized.blockerNeeds) !== JSON.stringify(task.blockerNeeds)) {
      payload.blocker_needs = normalized.blockerNeeds;
    }
    if (normalized.blockerKind !== (task.blockerKind || '')) {
      payload.blocker_kind = normalized.blockerKind;
    }
  } else if (task.blockerReason || task.blockerNeeds.length > 0 || task.blockerKind) {
    payload.blocker_reason = '';
    payload.blocker_needs = [];
    payload.blocker_kind = '';
  }

  return payload;
};

export const buildRealtimeMutationHandledPayload = ({
  currentConversationTurnId,
  result,
}: {
  currentConversationTurnId?: string | null;
  result: WorkbarMutationResult | void;
}): { action: string; taskId: string; turnId: string } | null => {
  const taskId = typeof result?.removeTaskId === 'string'
    ? result.removeTaskId.trim()
    : String(result?.patchTask?.id || '').trim();
  const turnId = String(
    result?.patchTask?.conversation_turn_id || currentConversationTurnId || '',
  ).trim();
  const action = result?.removeTaskId
    ? 'task_deleted'
    : (result?.patchTask ? 'task_updated' : '');

  if (!action || !taskId) {
    return null;
  }

  return {
    action,
    taskId,
    turnId,
  };
};

export const applyLocalTaskMutationResult = async ({
  currentConversationTurnId = null,
  loadCurrentTurnWorkbarTasks,
  loadHistoryWorkbarTasks,
  markHistoryWorkbarTasksStale,
  patchCurrentTurnWorkbarTask,
  removeCurrentTurnWorkbarTask,
  patchHistoryWorkbarTask,
  removeHistoryWorkbarTask,
  preferRealtimeSync = false,
  result,
  sessionId,
  taskHistoryOpen = false,
}: ApplyLocalTaskMutationResultArgs): Promise<void> => {
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
};

import type {
  SessionRuntimeGuidanceState,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';
import type { Message } from '../../types';
import type { TaskManagerTaskResponse } from '../../lib/api/client/types/runtime';

export interface SessionWorkbarApiClient {
  getPendingTaskReviews: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
  getPendingUiPrompts: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
  completeTaskManagerTask: (
    sessionId: string,
    taskId: string,
    payload?: {
      outcome_summary?: string;
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
      priority?: 'high' | 'medium' | 'low';
      status?: 'todo' | 'doing' | 'blocked' | 'done';
      due_at?: string | null;
      outcome_summary?: string;
      resume_hint?: string;
      blocker_reason?: string;
      blocker_needs?: string[];
      blocker_kind?: string;
    },
  ) => Promise<TaskManagerTaskResponse>;
  submitTaskReviewDecision: (
    reviewId: string,
    payload: {
      action: 'confirm' | 'cancel';
      tasks?: Array<{
        title: string;
        details: string;
        priority: 'high' | 'medium' | 'low';
        status: 'todo' | 'doing' | 'blocked' | 'done';
        tags: string[];
        due_at?: string | null;
      }>;
      reason?: string;
    },
  ) => Promise<unknown>;
  submitUiPromptResponse: (
    promptId: string,
    payload: {
      status: 'ok' | 'canceled';
      values?: Record<string, string>;
      selection?: string | string[];
      reason?: string;
    },
  ) => Promise<unknown>;
}

export interface SessionLike {
  id: string;
}

export interface UseSessionWorkbarPanelsArgs {
  apiClient: SessionWorkbarApiClient;
  session: SessionLike | null;
  enabled?: boolean;
  messages: Message[];
  selectedSessionActiveTurnId?: string | null;
  taskHistoryOpen?: boolean;
  uiPromptHistoryOpen?: boolean;
  sessionRuntimeGuidanceState: Record<string, SessionRuntimeGuidanceState | undefined>;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[] | undefined>;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[] | undefined>;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
  loadWorkbarSummaries: (sessionId: string, force?: boolean) => Promise<void>;
  loadUiPromptHistory?: (sessionId: string, force?: boolean) => Promise<void>;
  markUiPromptHistoryStale?: (sessionId: string) => void;
}

export interface OpenWorkbarHistoryOptions {
  forceHistory?: boolean;
  forceSummaries?: boolean;
}

export interface TaskRealtimeMutationGuardPayload {
  action: string;
  taskId?: string | null;
  turnId?: string | null;
}

import type { SessionRuntimeGuidanceState, TaskReviewPanelState, UiPromptPanelState } from '../../lib/store/types';
import type { SessionSummariesListResponse } from '../../lib/api/client/types';
import type { TaskManagerTaskResponse } from '../../lib/api/client/types/runtime';
import type { Message, Project, Session } from '../../types';
import { useContactMemoryContext } from './useContactMemoryContext';
import { useContactProjectScope } from './useContactProjectScope';
import { useSessionWorkbarPanels } from './useSessionWorkbarPanels';
import { useUiPromptHistory } from './useUiPromptHistory';

interface SessionResourcesApiClient {
  getPendingTaskReviews: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
  getContactProjects: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getConversationSummaries: (
    sessionId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<SessionSummariesListResponse>;
  getContactAgentRecalls: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getUiPromptHistory: (
    sessionId: string,
    params?: { limit?: number },
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

interface UseChatInterfaceSessionResourcesParams {
  apiClient: SessionResourcesApiClient;
  currentSession: Session | null;
  currentContactId: string;
  isTaskRunnerAsyncContactMode: boolean;
  currentChatStateActiveTurnId: string | null;
  currentProject: Project | null;
  projects: Project[];
  messages: Message[];
  activePanel: string;
  taskHistoryOpen?: boolean;
  uiPromptHistoryOpen?: boolean;
  sessionRuntimeGuidanceState: Record<string, SessionRuntimeGuidanceState | undefined>;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[] | undefined>;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[] | undefined>;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}

export const useChatInterfaceSessionResources = ({
  apiClient,
  currentSession,
  currentContactId,
  isTaskRunnerAsyncContactMode,
  currentChatStateActiveTurnId,
  currentProject,
  projects,
  messages,
  activePanel,
  taskHistoryOpen = false,
  uiPromptHistoryOpen = false,
  sessionRuntimeGuidanceState,
  taskReviewPanelsBySession,
  uiPromptPanelsBySession,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
}: UseChatInterfaceSessionResourcesParams) => {
  const {
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
  } = useContactProjectScope({
    apiClient,
    currentSession,
    currentContactId,
    projects,
  });

  const {
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    loadSessionMemorySummaries,
    applyRealtimeSessionMemorySummaries,
    markContactMemoryContextStale,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
  } = useContactMemoryContext({
    apiClient,
    currentSessionId: currentSession?.id || null,
    currentContactId,
    currentProjectIdForMemory,
  });

  const {
    uiPromptHistoryItems,
    uiPromptHistoryLoading,
    uiPromptHistoryError,
    loadUiPromptHistory,
    markUiPromptHistoryStale,
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
  } = useUiPromptHistory({
    apiClient,
    currentSessionId: currentSession?.id || null,
  });

  const workbar = useSessionWorkbarPanels({
    apiClient,
    session: currentSession,
    enabled: activePanel === 'chat' && !isTaskRunnerAsyncContactMode,
    messages,
    selectedSessionActiveTurnId: currentChatStateActiveTurnId,
    taskHistoryOpen,
    uiPromptHistoryOpen,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadWorkbarSummaries: loadSessionMemorySummaries,
    loadUiPromptHistory,
    markUiPromptHistoryStale,
  });

  return {
    currentProject,
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    loadSessionMemorySummaries,
    applyRealtimeSessionMemorySummaries,
    markContactMemoryContextStale,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
    uiPromptHistoryItems,
    uiPromptHistoryLoading,
    uiPromptHistoryError,
    loadUiPromptHistory,
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
    ...workbar,
  };
};

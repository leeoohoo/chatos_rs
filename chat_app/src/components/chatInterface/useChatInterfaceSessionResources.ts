import type { SessionRuntimeGuidanceState, TaskReviewPanelState, UiPromptPanelState } from '../../lib/store/types';
import type { Message, Project, Session } from '../../types';
import { useContactMemoryContext } from './useContactMemoryContext';
import { useContactProjectScope } from './useContactProjectScope';
import { useSessionWorkbarPanels } from './useSessionWorkbarPanels';
import { useUiPromptHistory } from './useUiPromptHistory';

interface SessionResourcesApiClient {
  getContactProjects: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getConversationSummaries: (
    sessionId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<{ items?: unknown[] }>;
  getContactAgentRecalls: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getUiPromptHistory: (
    sessionId: string,
    params?: { limit?: number },
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
  ) => Promise<unknown>;
  deleteTaskManagerTask: (sessionId: string, taskId: string) => Promise<unknown>;
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
  ) => Promise<unknown>;
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
  currentChatStateActiveTurnId: string | null;
  currentProject: Project | null;
  projects: Project[];
  messages: Message[];
  activePanel: string;
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
  currentChatStateActiveTurnId,
  currentProject,
  projects,
  messages,
  activePanel,
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
    resetMemoryState,
    cancelPendingMemoryLoad,
  } = useContactMemoryContext({
    apiClient,
    currentSessionId: currentSession?.id || null,
    currentContactId,
    currentProjectIdForMemory,
  });

  const currentSessionIdForUiPrompts = currentSession?.id || null;
  const {
    uiPromptHistoryItems,
    uiPromptHistoryLoading,
    uiPromptHistoryError,
    loadUiPromptHistory,
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
  } = useUiPromptHistory({
    apiClient,
    currentSessionId: currentSessionIdForUiPrompts,
  });

  const workbar = useSessionWorkbarPanels({
    apiClient,
    session: currentSession,
    enabled: activePanel === 'chat',
    messages,
    selectedSessionActiveTurnId: currentChatStateActiveTurnId,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadWorkbarSummaries: loadContactMemoryContext,
    loadUiPromptHistory,
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
    resetMemoryState,
    cancelPendingMemoryLoad,
    currentSessionIdForUiPrompts,
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

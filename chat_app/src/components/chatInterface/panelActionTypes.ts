import type {
  SessionMessageResponse,
} from '../../lib/api/client/types/session';
import type {
  TurnRuntimeSnapshotLookupResponse,
} from '../../lib/api/client/types/runtime';
import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../../lib/store/types';
import type { ChatStoreDraft, ChatStoreSet } from '../../lib/store/types';

export interface PanelActionsApiClient {
  submitTaskReviewDecision: (
    reviewId: string,
    payload: {
      action: 'confirm' | 'cancel';
      tasks?: Array<{
        title: string;
        details: string;
        priority: TaskReviewDraft['priority'];
        status: TaskReviewDraft['status'];
        tags: string[];
        due_at?: string | null;
      }>;
      reason?: string;
    },
  ) => Promise<unknown>;
  submitUiPromptResponse: (
    promptId: string,
    payload: UiPromptResponsePayload,
  ) => Promise<unknown>;
  getConversationLatestTurnRuntimeContext?: (
    sessionId: string,
  ) => Promise<TurnRuntimeSnapshotLookupResponse>;
  getConversationTurnRuntimeContextByTurn?: (
    sessionId: string,
    turnId: string,
  ) => Promise<TurnRuntimeSnapshotLookupResponse>;
  getConversationTurnMessagesByTurn?: (
    sessionId: string,
    turnId: string,
  ) => Promise<SessionMessageResponse[]>;
  getConversationTurnMessages?: (
    sessionId: string,
    userMessageId: string,
  ) => Promise<SessionMessageResponse[]>;
}

export interface TaskReviewPanelActionsArgs {
  activeTaskReviewPanel: TaskReviewPanelState | null;
  apiClient: PanelActionsApiClient;
  chatStoreSet?: ChatStoreSet;
  getChatStoreState?: () => ChatStoreDraft;
  preferRealtimeSync?: boolean;
  taskHistoryOpen?: boolean;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  loadCurrentTurnWorkbarTasks: (
    sessionId: string,
    conversationTurnId?: string | null,
    force?: boolean,
  ) => Promise<void>;
  loadHistoryWorkbarTasks: (sessionId: string, force?: boolean) => Promise<void>;
  markHistoryWorkbarTasksStale?: (sessionId: string) => void;
  removePendingTaskReviewCachePanel?: (reviewId: string, sessionId?: string) => void;
}

export interface UiPromptPanelActionsArgs {
  activeUiPromptPanel: UiPromptPanelState | null;
  apiClient: PanelActionsApiClient;
  chatStoreSet?: ChatStoreSet;
  getChatStoreState?: () => ChatStoreDraft;
  preferRealtimeSync?: boolean;
  uiPromptHistoryOpen?: boolean;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<void>;
  markUiPromptHistoryStale?: (sessionId: string) => void;
  removePendingUiPromptCachePanel?: (promptId: string, sessionId?: string) => void;
}

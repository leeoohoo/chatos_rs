import type {
  Message,
  SendMessageRuntimeOptions,
  TurnProcessViewerState,
} from '../../../types';

export interface TaskReviewDraft {
  id: string;
  title: string;
  details: string;
  priority: 'high' | 'medium' | 'low';
  status: 'todo' | 'doing' | 'blocked' | 'done';
  tags: string[];
  dueAt?: string | null;
}

export interface TaskReviewPanelState {
  reviewId: string;
  sessionId: string;
  conversationTurnId: string;
  drafts: TaskReviewDraft[];
  timeoutMs?: number;
  submitting?: boolean;
  error?: string | null;
}

export type UiPromptKind = 'kv' | 'choice' | 'mixed';

export interface UiPromptField {
  key: string;
  label?: string;
  description?: string;
  placeholder?: string;
  default?: string;
  required?: boolean;
  multiline?: boolean;
  secret?: boolean;
}

export interface UiPromptChoiceOption {
  value: string;
  label?: string;
  description?: string;
}

export interface UiPromptChoice {
  multiple?: boolean;
  options: UiPromptChoiceOption[];
  default?: string | string[];
  min_selections?: number;
  max_selections?: number;
}

export interface UiPromptPayloadShape {
  fields?: UiPromptField[];
  choice?: UiPromptChoice;
}

export interface UiPromptPanelState {
  promptId: string;
  sessionId: string;
  conversationTurnId: string;
  toolCallId?: string | null;
  kind: UiPromptKind;
  title?: string;
  message?: string;
  allowCancel?: boolean;
  timeoutMs?: number;
  payload: UiPromptPayloadShape;
  submitting?: boolean;
  error?: string | null;
}

export interface UiPromptResponsePayload {
  status: 'ok' | 'canceled';
  values?: Record<string, string>;
  selection?: string | string[];
  reason?: string;
}

export interface SessionChatState {
  isLoading: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  streamingPhase?: 'thinking' | 'reviewing' | null;
  streamingMessageId: string | null;
  activeTurnId: string | null;
  streamingPreviewText: string;
  streamingTransport?: 'realtime' | 'sse' | null;
  runtimeContextRefreshNonce?: number;
}

export interface RuntimeGuidanceItem {
  guidanceId: string;
  turnId: string | null;
  content: string;
  status: 'queued' | 'applied' | 'dropped';
  createdAt: string;
  appliedAt: string | null;
}

export interface SessionRuntimeGuidanceState {
  pendingCount: number;
  appliedCount: number;
  lastGuidanceAt: string | null;
  lastAppliedAt: string | null;
  items: RuntimeGuidanceItem[];
}

export interface SessionMessagePaginationState {
  nextBefore: string | null;
  loaded: boolean;
}

export interface SessionMessagesSnapshot {
  messages: Message[];
  nextBefore: string | null;
  loaded: boolean;
}

export interface SessionMessagesCacheEntry extends SessionMessagesSnapshot {
  fetchedAt: number;
}

export interface ConversationRuntimeSliceState {
  messages: Message[];
  isLoading: boolean;
  isStreaming: boolean;
  streamingMessageId: string | null;
  hasMoreMessages: boolean;
  sessionChatState: Record<string, SessionChatState>;
  sessionMessagePaginationState: Record<string, SessionMessagePaginationState>;
  sessionMessagesCache: Record<string, SessionMessagesCacheEntry>;
  sessionMessagesCacheOrder: string[];
  sessionRuntimeGuidanceState: Record<string, SessionRuntimeGuidanceState>;
  sessionStreamingMessageDrafts: Record<string, Message | null>;
  sessionTurnProcessCache: Record<string, Record<string, Message[]>>;
  turnProcessViewer: TurnProcessViewerState;
  taskReviewPanel: TaskReviewPanelState | null;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[]>;
  uiPromptPanel: UiPromptPanelState | null;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[]>;
}

export const conversationRuntimeInitialState: ConversationRuntimeSliceState = {
  messages: [],
  isLoading: false,
  isStreaming: false,
  streamingMessageId: null,
  hasMoreMessages: true,
  sessionChatState: {},
  sessionMessagePaginationState: {},
  sessionMessagesCache: {},
  sessionMessagesCacheOrder: [],
  sessionRuntimeGuidanceState: {},
  sessionStreamingMessageDrafts: {},
  sessionTurnProcessCache: {},
  turnProcessViewer: {
    open: false,
    sessionId: null,
    userMessageId: null,
    turnId: null,
  },
  taskReviewPanel: null,
  taskReviewPanelsBySession: {},
  uiPromptPanel: null,
  uiPromptPanelsBySession: {},
};

export interface ConversationRuntimeSliceActions {
  loadMessages: (sessionId: string) => Promise<void>;
  syncSessionMessagesInBackground: (sessionId: string) => Promise<void>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
  openTurnProcessViewer: (
    sessionId: string,
    userMessageId: string,
    options?: { turnId?: string | null }
  ) => void;
  closeTurnProcessViewer: () => void;
  sendMessage: (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => Promise<void>;
  submitRuntimeGuidance: (content: string, options: {
    conversationId: string;
    turnId: string;
    projectId?: string | null;
  }) => Promise<{
    success: boolean;
    guidanceId?: string;
    status?: 'queued' | 'applied' | 'dropped';
    pendingCount?: number;
    turnId?: string;
  }>;
  updateMessage: (messageId: string, updates: Partial<Message>) => Promise<void>;
  deleteMessage: (messageId: string) => Promise<void>;
  startStreaming: (messageId: string) => void;
  updateStreamingMessage: (content: string) => void;
  stopStreaming: () => void;
  abortCurrentConversation: () => void;
  setTaskReviewPanel: (panel: TaskReviewPanelState | null) => void;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  setUiPromptPanel: (panel: UiPromptPanelState | null) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}

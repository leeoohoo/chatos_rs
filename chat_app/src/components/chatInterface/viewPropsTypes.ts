import type { ComponentProps } from 'react';

import type ApiClient from '../../lib/api/client';
import type { AgentConfig, Message, Project, RemoteConnection, Session } from '../../types';
import type { TaskWorkbarItem } from '../TaskWorkbar';
import type { TaskOutcomeDraft } from '../taskWorkbar/TaskOutcomeModal';
import ChatConversationPane from './ChatConversationPane';
import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';
import UiPromptHistoryDrawer from './UiPromptHistoryDrawer';

export interface ChatInterfaceConversationState {
  currentSession: Session | null;
  sessionSummaryPaneVisible: boolean;
  taskHistoryOpen: boolean;
  currentContactName: string;
  currentProjectNameForMemory: string;
  currentProjectIdForMemory: string | null;
  currentContactId: string;
  isTaskRunnerAsyncContactMode: boolean;
  messages: ComponentProps<typeof ChatConversationPane>['messages'];
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  chatStreamingPhase: 'thinking' | 'reviewing' | null;
  chatStreamingPreviewText: string;
  hasMoreMessages: boolean;
  customRenderer: ComponentProps<typeof ChatConversationPane>['customRenderer'];
  sessionMemorySummaries: ComponentProps<typeof ChatConversationPane>['sessionMemorySummaries'];
  agentRecalls: ComponentProps<typeof ChatConversationPane>['agentRecalls'];
  memoryLoading: boolean;
  memoryError: string | null;
  reviewRepairRunning: boolean;
  reviewRepairPendingCount: number | null;
  reviewRepairDisabled: boolean;
  mergedCurrentTurnTasks: ComponentProps<typeof ChatConversationPane>['mergedCurrentTurnTasks'];
  workbarHistoryTasks: ComponentProps<typeof ChatConversationPane>['workbarHistoryTasks'];
  activeConversationTurnId: string | null;
  workbarLoading: boolean;
  workbarHistoryLoading: boolean;
  workbarError: string | null;
  workbarHistoryError: string | null;
  workbarActionLoadingTaskId: string | null;
  taskModalOpen: ComponentProps<typeof ChatConversationPane>['taskModalOpen'];
  taskModalMode: ComponentProps<typeof ChatConversationPane>['taskModalMode'];
  taskModalTask: ComponentProps<typeof ChatConversationPane>['taskModalTask'];
  taskModalError: ComponentProps<typeof ChatConversationPane>['taskModalError'];
  uiPromptHistoryItems: ComponentProps<typeof UiPromptHistoryDrawer>['items'];
  uiPromptHistoryLoading: boolean;
  activeUiPromptPanel: ComponentProps<typeof ChatConversationPane>['activeUiPromptPanel'];
  activeTaskReviewPanel: ComponentProps<typeof ChatConversationPane>['activeTaskReviewPanel'];
  supportedFileTypes: string[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  currentAgent: AgentConfig | null;
  aiModelConfigs: ComponentProps<typeof ChatConversationPane>['availableModels'];
  composerAvailableProjects: Project[];
  currentProject: Project | null;
  composerWorkspaceRoot: string | null;
  currentRemoteConnectionId: string | null;
  remoteConnections: RemoteConnection[];
  composerMcpEnabled: boolean;
  composerEnabledMcpIds: string[];
  composerAutoCreateTask: boolean;
  turnProcessViewer: {
    open: boolean;
    sessionId: string | null;
    userMessageId: string | null;
    turnId: string | null;
  };
  turnProcessCacheBySession: Record<string, Record<string, Message[]>>;
  apiClient: ApiClient;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: ComponentProps<typeof ChatConversationPane>['runtimeGuidanceItems'];
}

export interface ChatInterfaceConversationActions {
  handleLoadMore: () => void;
  handleToggleTurnProcess: (userMessageId: string) => void;
  handleRefreshMemory: (sessionId: string) => void;
  handleRunReviewRepair: (sessionId: string) => Promise<void>;
  handleCloseSummary: () => void;
  toggleSidebar: () => void;
  handleRefreshWorkbar: () => void;
  handleOpenHistory: (sessionId: string) => void;
  setTaskHistoryOpen: (value: boolean) => void;
  handleOpenUiPromptHistory: (sessionId: string) => void;
  handleWorkbarCompleteTask: (task: TaskWorkbarItem) => Promise<void>;
  handleWorkbarDeleteTask: (task: TaskWorkbarItem) => Promise<void>;
  handleWorkbarEditTask: (task: TaskWorkbarItem) => Promise<void>;
  closeTaskModal: () => void;
  submitTaskModal: (draft: TaskOutcomeDraft) => Promise<void>;
  handleUiPromptSubmit: ComponentProps<typeof ChatConversationPane>['onUiPromptSubmit'];
  handleUiPromptCancel: () => void;
  handleTaskReviewConfirm: ComponentProps<typeof ChatConversationPane>['onTaskReviewConfirm'];
  handleTaskReviewCancel: ComponentProps<typeof ChatConversationPane>['onTaskReviewCancel'];
  handleMessageSend: ComponentProps<typeof ChatConversationPane>['onSend'];
  handleRuntimeGuidanceSend: (content: string, attachments?: File[]) => void;
  abortCurrentConversation: () => void;
  updateReasoningEnabled: (enabled: boolean) => void;
  setSelectedModel: (modelId: string | null) => void;
  setSelectedModelName: (modelName: string | null) => void;
  setSelectedThinkingLevel: (level: string | null) => void;
  setModelRuntimeSelection: (selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => void;
  handleComposerProjectChange: (projectId: string | null) => void;
  handleComposerWorkspaceRootChange: (path: string | null) => void;
  handleComposerRemoteConnectionChange: (connectionId: string | null) => void;
  handleComposerMcpEnabledChange: (enabled: boolean) => void;
  handleComposerEnabledMcpIdsChange: (ids: string[]) => void;
  handleComposerAutoCreateTaskChange: (enabled: boolean) => void;
  closeTurnProcessViewer: () => void;
}

export interface ChatInterfaceOverlayState {
  currentSession: { id: string } | null;
  currentSessionId: string | null;
  uiPromptHistoryOpen: boolean;
  uiPromptHistoryItems: ComponentProps<typeof UiPromptHistoryDrawer>['items'];
  uiPromptHistoryLoading: boolean;
  uiPromptHistoryError: string | null;
  runtimeContextOpen: boolean;
  runtimeContextSessionId: string | null;
  runtimeContextLoading: boolean;
  runtimeContextError: string | null;
  runtimeContextData: ComponentProps<typeof TurnRuntimeContextDrawer>['data'];
}

export interface ChatInterfaceOverlayActions {
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<unknown>;
  setUiPromptHistoryOpen: (value: boolean) => void;
  handleRefreshRuntimeContext: () => void;
  setRuntimeContextOpen: (value: boolean) => void;
}

export interface ChatInterfaceViewPropsParams {
  conversation: ChatInterfaceConversationState;
  conversationActions: ChatInterfaceConversationActions;
  overlay: ChatInterfaceOverlayState;
  overlayActions: ChatInterfaceOverlayActions;
}

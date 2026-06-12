import type { ComponentProps } from 'react';

import type { Project, RemoteConnection, Session } from '../../types';
import ChatConversationPane from './ChatConversationPane';
import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';

export interface ChatInterfaceConversationState {
  currentSession: Session | null;
  sessionSummaryPaneVisible: boolean;
  currentContactName: string;
  currentProjectNameForMemory: string;
  currentProjectIdForMemory: string | null;
  messages: ComponentProps<typeof ChatConversationPane>['messages'];
  hasMoreMessages: boolean;
  customRenderer: ComponentProps<typeof ChatConversationPane>['customRenderer'];
  sessionMemorySummaries: ComponentProps<typeof ChatConversationPane>['sessionMemorySummaries'];
  agentRecalls: ComponentProps<typeof ChatConversationPane>['agentRecalls'];
  memoryLoading: boolean;
  memoryError: string | null;
  supportedFileTypes: string[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  aiModelConfigs: ComponentProps<typeof ChatConversationPane>['availableModels'];
  composerAvailableProjects: Project[];
  currentProject: Project | null;
  composerWorkspaceRoot: string | null;
  currentRemoteConnectionId: string | null;
  remoteConnections: RemoteConnection[];
}

export interface ChatInterfaceConversationActions {
  handleLoadMore: () => void;
  handleRefreshMemory: (sessionId: string) => void;
  handleCloseSummary: () => void;
  toggleSidebar: () => void;
  handleMessageSend: ComponentProps<typeof ChatConversationPane>['onSend'];
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
}

export interface ChatInterfaceOverlayState {
  runtimeContextOpen: boolean;
  runtimeContextSessionId: string | null;
  runtimeContextLoading: boolean;
  runtimeContextError: string | null;
  runtimeContextData: ComponentProps<typeof TurnRuntimeContextDrawer>['data'];
}

export interface ChatInterfaceOverlayActions {
  handleRefreshRuntimeContext: () => void;
  setRuntimeContextOpen: (value: boolean) => void;
}

export interface ChatInterfaceViewPropsParams {
  conversation: ChatInterfaceConversationState;
  conversationActions: ChatInterfaceConversationActions;
  overlay: ChatInterfaceOverlayState;
  overlayActions: ChatInterfaceOverlayActions;
}

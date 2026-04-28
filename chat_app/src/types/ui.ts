import type { ReactNode } from 'react';

import type {
  Attachment,
  Message,
  Session,
  ToolCall,
} from './chat';
import type {
  AiModelConfig,
  AgentConfig,
  Application,
  ChatConfig,
  McpConfig,
  SystemContext,
} from './config';
import type { ChatError, UnknownRecord } from './common';
import type {
  GuideMessageHandler,
  SendMessageHandler,
} from './runtime';
import type {
  ContactRecord,
  Project,
  RemoteConnection,
  Terminal,
} from './workspace';
import type { Theme } from './common';

export interface ChatInterfaceProps {
  className?: string;
  onSessionChange?: (sessionId: string) => void;
  onMessageSend?: (content: string, attachments?: File[]) => void;
  customRenderer?: {
    renderMessage?: (message: Message) => ReactNode;
    renderAttachment?: (attachment: Attachment) => ReactNode;
    renderToolCall?: (toolCall: ToolCall) => ReactNode;
  };
}

export interface MessageListProps {
  sessionId?: string | null;
  messages: Message[];
  isLoading?: boolean;
  isStreaming?: boolean;
  isStopping?: boolean;
  streamingPreviewText?: string;
  hasMore?: boolean;
  onLoadMore?: () => void;
  onToggleTurnProcess?: (userMessageId: string) => void;
  onMessageEdit?: (messageId: string, content: string) => void;
  onMessageDelete?: (messageId: string) => void;
  customRenderer?: {
    renderMessage?: (message: Message) => ReactNode;
    renderAttachment?: (attachment: Attachment) => ReactNode;
  };
}

export interface InputAreaProps {
  onSend: SendMessageHandler;
  onGuide?: GuideMessageHandler;
  onStop?: () => void;
  disabled?: boolean;
  isStreaming?: boolean;
  isStopping?: boolean;
  placeholder?: string;
  maxLength?: number;
  allowAttachments?: boolean;
  supportedFileTypes?: string[];
  reasoningSupported?: boolean;
  reasoningEnabled?: boolean;
  onReasoningToggle?: (enabled: boolean) => void;
  showModelSelector?: boolean;
  selectedModelId?: string | null;
  availableModels?: AiModelConfig[];
  onModelChange?: (modelId: string | null) => void;
  availableProjects?: Project[];
  currentProject?: Project | null;
  selectedProjectId?: string | null;
  onProjectChange?: (projectId: string | null) => void;
  showProjectSelector?: boolean;
  showProjectFileButton?: boolean;
  workspaceRoot?: string | null;
  onWorkspaceRootChange?: (path: string | null) => void;
  currentRemoteConnectionId?: string | null;
  currentAgent?: AgentConfig | null;
  availableRemoteConnections?: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  showWorkspaceRootPicker?: boolean;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
  onMcpEnabledChange?: (enabled: boolean) => void;
  onEnabledMcpIdsChange?: (ids: string[]) => void;
}

export interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: unknown;
}

export interface ChatEvents {
  onMessageReceived: (message: Message) => void;
  onMessageUpdated: (message: Message) => void;
  onSessionCreated: (session: Session) => void;
  onSessionUpdated: (session: Session) => void;
  onError: (error: ChatError) => void;
}

export interface ChatPlugin {
  name: string;
  version: string;
  initialize: (config: UnknownRecord) => Promise<void>;
  destroy: () => Promise<void>;
  onMessage?: (message: Message) => Promise<Message | null>;
  onToolCall?: (toolCall: ToolCall) => Promise<unknown>;
}

export interface DatabaseOperations {
  createSession: (title: string) => Promise<Session>;
  getSession: (id: string) => Promise<Session | null>;
  updateSession: (id: string, updates: Partial<Session>) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  listSessions: () => Promise<Session[]>;
  createMessage: (message: Omit<Message, 'id' | 'createdAt'>) => Promise<Message>;
  getMessage: (id: string) => Promise<Message | null>;
  updateMessage: (id: string, updates: Partial<Message>) => Promise<void>;
  deleteMessage: (id: string) => Promise<void>;
  getConversationMessages: (conversationId: string) => Promise<Message[]>;
}

export interface ChatStateSnapshot {
  sessions: Session[];
  messages: Message[];
  chatConfig: ChatConfig;
  theme: Theme;
  mcpConfigs: McpConfig[];
  aiModelConfigs: AiModelConfig[];
  systemContexts: SystemContext[];
  agents: AgentConfig[];
  applications: Application[];
  projects: Project[];
  terminals: Terminal[];
  remoteConnections: RemoteConnection[];
  contacts: ContactRecord[];
}

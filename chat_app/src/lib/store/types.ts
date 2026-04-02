import type { Message, Session, ChatConfig, Theme, McpConfig, AiModelConfig, SystemContext, AgentConfig, Application, Project, Terminal, RemoteConnection } from '../../types';
import type {
  SystemContextDraftEvaluateResponse,
  SystemContextDraftGenerateResponse,
  SystemContextDraftOptimizeResponse,
  SystemContextModelConfigPayload,
  SystemContextResponse,
} from '../api/client/types';

export interface TaskReviewDraft {
  id: string;
  title: string;
  details: string;
  priority: 'high' | 'medium' | 'low';
  status: 'pending_confirm' | 'pending_execute' | 'running' | 'completed' | 'failed' | 'cancelled';
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

export interface SessionAiSelection {
  selectedModelId: string | null;
  selectedAgentId: string | null;
}

export interface SessionCreatePayload {
  title?: string;
  contactAgentId?: string | null;
  contactId?: string | null;
  selectedModelId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
}

export interface SendMessageRuntimeOptions {
  contactAgentId?: string | null;
  contactId?: string | null;
  remoteConnectionId?: string | null;
  projectId?: string | null;
  projectRoot?: string | null;
  workspaceRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
}

export interface SessionSelectOptions {
  keepActivePanel?: boolean;
}

export interface SessionCreateOptions {
  keepActivePanel?: boolean;
}

export interface SessionChatState {
  isLoading: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  streamingMessageId: string | null;
  activeTurnId: string | null;
}

export interface SessionRuntimeGuidanceState {
  pendingCount: number;
  appliedCount: number;
  lastGuidanceAt: string | null;
  lastAppliedAt: string | null;
  items: RuntimeGuidanceItem[];
}

export interface RuntimeGuidanceItem {
  guidanceId: string;
  turnId: string | null;
  content: string;
  status: 'queued' | 'applied' | 'dropped';
  createdAt: string;
  appliedAt: string | null;
}

export interface ContactRecord {
  id: string;
  agentId: string;
  name: string;
  status: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface ChatState {
  // 会话相关
  sessions: Session[];
  currentSessionId: string | null;
  currentSession: Session | null;
  contacts: ContactRecord[];

  // 项目相关
  projects: Project[];
  currentProjectId: string | null;
  currentProject: Project | null;
  activePanel: 'chat' | 'project' | 'terminal' | 'remote_terminal' | 'remote_sftp';

  // 终端相关
  terminals: Terminal[];
  currentTerminalId: string | null;
  currentTerminal: Terminal | null;

  // 远端连接
  remoteConnections: RemoteConnection[];
  currentRemoteConnectionId: string | null;
  currentRemoteConnection: RemoteConnection | null;

  // 消息相关
  messages: Message[];
  isLoading: boolean;
  isStreaming: boolean;
  streamingMessageId: string | null;
  hasMoreMessages: boolean;
  sessionChatState: Record<string, SessionChatState>;
  sessionRuntimeGuidanceState: Record<string, SessionRuntimeGuidanceState>;
  sessionStreamingMessageDrafts: Record<string, Message | null>;
  sessionTurnProcessState: Record<string, Record<string, { expanded: boolean; loaded: boolean; loading: boolean }>>;
  sessionTurnProcessCache: Record<string, Record<string, Message[]>>;
  taskReviewPanel: TaskReviewPanelState | null;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[]>;
  uiPromptPanel: UiPromptPanelState | null;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[]>;

  // UI状态
  sidebarOpen: boolean;
  theme: Theme;

  // 配置相关
  chatConfig: ChatConfig;
  mcpConfigs: McpConfig[];
  aiModelConfigs: AiModelConfig[];
  selectedModelId: string | null;
  agents: AgentConfig[];
  selectedAgentId: string | null;
  sessionAiSelectionBySession: Record<string, SessionAiSelection>;
  systemContexts: SystemContext[];
  activeSystemContext: SystemContext | null;
  // 应用相关
  applications: Application[];
  selectedApplicationId: string | null;

  // 错误处理
  error: string | null;
}

export interface ChatActions {
  // 联系人操作
  loadContacts: () => Promise<ContactRecord[]>;
  createContact: (agentId: string, agentNameSnapshot?: string) => Promise<ContactRecord>;
  deleteContact: (contactId: string) => Promise<void>;
  getContactByAgentId: (agentId: string) => ContactRecord | null;

  // 会话操作
  loadSessions: (options?: { limit?: number; offset?: number; append?: boolean; silent?: boolean }) => Promise<Session[]>;
  createSession: (
    payload?: string | SessionCreatePayload,
    options?: SessionCreateOptions,
  ) => Promise<string>;
  selectSession: (sessionId: string, options?: SessionSelectOptions) => Promise<void>;
  updateSession: (sessionId: string, updates: Partial<Session>) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;

  // 项目操作
  loadProjects: () => Promise<Project[]>;
  createProject: (name: string, rootPath: string, description?: string) => Promise<Project>;
  updateProject: (projectId: string, updates: Partial<Project>) => Promise<Project | null>;
  deleteProject: (projectId: string) => Promise<void>;
  selectProject: (projectId: string) => Promise<void>;
  setActivePanel: (panel: 'chat' | 'project' | 'terminal' | 'remote_terminal' | 'remote_sftp') => void;

  // 终端操作
  loadTerminals: () => Promise<Terminal[]>;
  createTerminal: (cwd: string, name?: string) => Promise<Terminal>;
  deleteTerminal: (terminalId: string) => Promise<void>;
  selectTerminal: (terminalId: string) => Promise<void>;
  loadRemoteConnections: () => Promise<RemoteConnection[]>;
  createRemoteConnection: (payload: {
    name?: string;
    host: string;
    port?: number;
    username: string;
    auth_type?: 'private_key' | 'private_key_cert' | 'password';
    password?: string;
    private_key_path?: string;
    certificate_path?: string;
    default_remote_path?: string;
    host_key_policy?: 'strict' | 'accept_new';
    jump_enabled?: boolean;
    jump_host?: string;
    jump_port?: number;
    jump_username?: string;
    jump_private_key_path?: string;
    jump_password?: string;
  }) => Promise<RemoteConnection>;
  updateRemoteConnection: (connectionId: string, payload: {
    name?: string;
    host?: string;
    port?: number;
    username?: string;
    auth_type?: 'private_key' | 'private_key_cert' | 'password';
    password?: string;
    private_key_path?: string;
    certificate_path?: string;
    default_remote_path?: string;
    host_key_policy?: 'strict' | 'accept_new';
    jump_enabled?: boolean;
    jump_host?: string;
    jump_port?: number;
    jump_username?: string;
    jump_private_key_path?: string;
    jump_password?: string;
  }) => Promise<RemoteConnection | null>;
  deleteRemoteConnection: (connectionId: string) => Promise<void>;
  selectRemoteConnection: (
    connectionId: string | null,
    options?: { activatePanel?: boolean },
  ) => Promise<void>;
  openRemoteSftp: (connectionId: string) => Promise<void>;

  // 消息操作
  loadMessages: (sessionId: string) => Promise<void>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
  toggleTurnProcess: (
    userMessageId: string,
    options?: { forceExpand?: boolean; forceCollapse?: boolean }
  ) => Promise<void>;
  sendMessage: (
    content: string,
    attachments?: any[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => Promise<void>;
  submitRuntimeGuidance: (content: string, options: {
    sessionId: string;
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

  // 流式消息处理
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

  // UI操作
  toggleSidebar: () => void;
  setTheme: (theme: Theme) => void;

  // 配置操作
  updateChatConfig: (config: Partial<ChatConfig>) => Promise<void>;
  loadMcpConfigs: () => Promise<void>;
  updateMcpConfig: (config: McpConfig) => Promise<McpConfig | null>;
  deleteMcpConfig: (id: string) => Promise<void>;
  loadAiModelConfigs: () => Promise<void>;
  updateAiModelConfig: (config: AiModelConfig) => Promise<void>;
  deleteAiModelConfig: (id: string) => Promise<void>;
  setSelectedModel: (modelId: string | null) => void;
  // 智能体
  loadAgents: () => Promise<void>;
  setSelectedAgent: (agentId: string | null) => void;
  loadSystemContexts: () => Promise<void>;
  createSystemContext: (name: string, content: string, appIds?: string[]) => Promise<SystemContextResponse | null>;
  updateSystemContext: (id: string, name: string, content: string, appIds?: string[]) => Promise<SystemContextResponse | null>;
  deleteSystemContext: (id: string) => Promise<void>;
  activateSystemContext: (id: string) => Promise<void>;
  generateSystemContextDraft: (payload: {
    scene: string;
    style?: string;
    language?: string;
    output_format?: string;
    constraints?: string[];
    forbidden?: string[];
    candidate_count?: number;
    ai_model_config?: SystemContextModelConfigPayload;
  }) => Promise<SystemContextDraftGenerateResponse>;
  optimizeSystemContextDraft: (payload: {
    content: string;
    goal?: string;
    keep_intent?: boolean;
    ai_model_config?: SystemContextModelConfigPayload;
  }) => Promise<SystemContextDraftOptimizeResponse>;
  evaluateSystemContextDraft: (payload: {
    content: string;
  }) => Promise<SystemContextDraftEvaluateResponse>;
  // 应用管理
  loadApplications: () => Promise<void>;
  createApplication: (name: string, url: string, iconUrl?: string) => Promise<void>;
  updateApplication: (id: string, updates: Partial<Application>) => Promise<void>;
  deleteApplication: (id: string) => Promise<void>;
  setSelectedApplication: (appId: string | null) => void;
  setSystemContextAppAssociation: (contextId: string, appIds: string[]) => void;
  setAgentAppAssociation: (agentId: string, appIds: string[]) => void;

  // 错误处理
  setError: (error: string | null) => void;
  clearError: () => void;
}

export interface ChatStoreConfig {
  userId?: string;
  projectId?: string;
}

export type ChatStoreShape = ChatState & ChatActions;
export type ChatStoreDraft = ChatState & Partial<ChatActions>;
export type ChatStoreSet = (fn: (state: ChatStoreDraft) => void) => void;
export type ChatStoreGet = () => ChatStoreShape;

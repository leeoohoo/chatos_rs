import type { Message, Session, ChatConfig, Theme, McpConfig, AiModelConfig, SystemContext, AgentConfig, Application, Project, Terminal } from '../../types';

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

export interface ChatState {
  // 会话相关
  sessions: Session[];
  currentSessionId: string | null;
  currentSession: Session | null;

  // 项目相关
  projects: Project[];
  currentProjectId: string | null;
  currentProject: Project | null;
  activePanel: 'chat' | 'project' | 'terminal';

  // 终端相关
  terminals: Terminal[];
  currentTerminalId: string | null;
  currentTerminal: Terminal | null;

  // 消息相关
  messages: Message[];
  isLoading: boolean;
  isStreaming: boolean;
  streamingMessageId: string | null;
  hasMoreMessages: boolean;
  sessionChatState: Record<string, { isLoading: boolean; isStreaming: boolean; streamingMessageId: string | null }>;
  taskReviewPanel: TaskReviewPanelState | null;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[]>;

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
  systemContexts: SystemContext[];
  activeSystemContext: SystemContext | null;
  // 应用相关
  applications: Application[];
  selectedApplicationId: string | null;

  // 错误处理
  error: string | null;
}

export interface ChatActions {
  // 会话操作
  loadSessions: (options?: { limit?: number; offset?: number; append?: boolean; silent?: boolean }) => Promise<Session[]>;
  createSession: (title?: string) => Promise<string>;
  selectSession: (sessionId: string) => Promise<void>;
  updateSession: (sessionId: string, updates: Partial<Session>) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;

  // 项目操作
  loadProjects: () => Promise<Project[]>;
  createProject: (name: string, rootPath: string, description?: string) => Promise<Project>;
  updateProject: (projectId: string, updates: Partial<Project>) => Promise<Project | null>;
  deleteProject: (projectId: string) => Promise<void>;
  selectProject: (projectId: string) => Promise<void>;
  setActivePanel: (panel: 'chat' | 'project' | 'terminal') => void;

  // 终端操作
  loadTerminals: () => Promise<Terminal[]>;
  createTerminal: (cwd: string, name?: string) => Promise<Terminal>;
  deleteTerminal: (terminalId: string) => Promise<void>;
  selectTerminal: (terminalId: string) => Promise<void>;

  // 消息操作
  loadMessages: (sessionId: string) => Promise<void>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
  sendMessage: (content: string, attachments?: any[]) => Promise<void>;
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
  createSystemContext: (name: string, content: string, appIds?: string[]) => Promise<any>;
  updateSystemContext: (id: string, name: string, content: string, appIds?: string[]) => Promise<any>;
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
    ai_model_config?: any;
  }) => Promise<any>;
  optimizeSystemContextDraft: (payload: {
    content: string;
    goal?: string;
    keep_intent?: boolean;
    ai_model_config?: any;
  }) => Promise<any>;
  evaluateSystemContextDraft: (payload: {
    content: string;
  }) => Promise<any>;
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

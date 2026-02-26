import { ReactNode } from 'react';

// 基础消息类型
export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';
export type MessageStatus = 'pending' | 'streaming' | 'completed' | 'error';
export type AttachmentType = 'image' | 'file' | 'audio';

// 主题类型
export type Theme = 'light' | 'dark' | 'auto';

// 消息接口
export interface Message {
  id: string;
  sessionId: string;
  role: MessageRole;
  content: string;
  rawContent?: string;
  summary?: string; // AI生成的内容总结
  tokensUsed?: number;
  status: MessageStatus;
  createdAt: Date;
  updatedAt?: Date;
  metadata?: {
    attachments?: Attachment[];
    toolCalls?: ToolCall[];
    contentSegments?: ContentSegment[];
    currentSegmentIndex?: number;
    model?: string;
    summary?: string; // AI生成的内容总结（也可以存储在这里）
    [key: string]: any;
  };
}

// 会话接口
export interface Session {
  id: string;
  title: string;
  createdAt: Date;
  updatedAt: Date;
  messageCount: number;
  tokenUsage: number;
  tags?: string | null;
  pinned: boolean;
  archived: boolean;
  metadata?: string | null;
}

// 项目接口
export interface Project {
  id: string;
  name: string;
  rootPath: string;
  description?: string | null;
  userId?: string | null;
  createdAt: Date;
  updatedAt: Date;
}

// 终端接口
export interface Terminal {
  id: string;
  name: string;
  cwd: string;
  userId?: string | null;
  status: string;
  busy?: boolean;
  createdAt: Date;
  updatedAt: Date;
  lastActiveAt: Date;
}

export interface TerminalLog {
  id: string;
  terminalId: string;
  logType: string;
  content: string;
  createdAt: Date | string;
}


// 文件系统条目
export interface FsEntry {
  name: string;
  path: string;
  isDir: boolean;
  size?: number | null;
  modifiedAt?: string | null;
}

export interface FsReadResult {
  path: string;
  name: string;
  size: number;
  contentType: string;
  isBinary: boolean;
  modifiedAt?: string | null;
  content: string;
}

export interface ChangeLogItem {
  id: string;
  serverName: string;
  path: string;
  action: string;
  bytes: number;
  sha256?: string | null;
  diff?: string | null;
  sessionId?: string | null;
  runId?: string | null;
  createdAt: string;
  sessionTitle?: string | null;
}

// 系统上下文接口
export interface SystemContext {
  id: string;
  name: string;
  content: string;
  userId: string;
  isActive: boolean;
  createdAt: Date;
  updatedAt: Date;
  // 关联的应用（不选择表示通用）
  app_ids?: string[];
}

// 附件接口
export interface Attachment {
  id: string;
  messageId: string;
  type: AttachmentType;
  name: string;
  url: string;
  size: number;
  mimeType: string;
  createdAt: Date;
}

// 工具调用接口
export interface ToolCall {
  id: string;
  messageId: string;
  name: string;
  arguments: Record<string, any> | string;
  result?: any;
  error?: string;
  createdAt: Date;
}

// 内容分段接口
export interface ContentSegment {
  content: string | ToolCall;
  type: 'text' | 'tool_call' | 'thinking';
  toolCallId?: string;
}

// 聊天配置
export interface ChatConfig {
  model: string;
  temperature: number;
  systemPrompt: string;
  enableMcp: boolean;
  reasoningEnabled: boolean;
}

// MCP配置接口
export interface McpConfig {
  id: string;
  name: string;
  display_name?: string;
  command: string;
  type: 'http' | 'stdio';
  args?: string[] | null;
  env?: Record<string, string> | null;
  cwd?: string | null;
  enabled: boolean;
  readonly?: boolean;
  builtin?: boolean;
  supports_settings?: boolean;
  builtin_kind?: string;
  config?: any;
  createdAt: Date;
  updatedAt: Date;
}

// AI模型配置接口
export interface AiModelConfig {
  id: string;
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model_name: string;
  thinking_level?: string;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
  createdAt: Date;
  updatedAt: Date;
}

// 智能体配置（简化版）
export interface AgentConfig {
  id: string;
  name: string;
  description?: string;
  ai_model_config_id: string;
  enabled: boolean;
  project_id?: string | null;
  workspace_dir?: string | null;
  createdAt: Date;
  updatedAt: Date;
  // 关联的应用（不选择表示通用）
  app_ids?: string[];
}

// AI客户端配置
export interface AiClientConfig {
  apiKey: string;
  baseUrl?: string;
  model: string;
  temperature: number;
  systemPrompt?: string;
  enableStreaming: boolean;
}

// MCP工具配置
export interface McpToolConfig {
  name: string;
  command: string;
  enabled: boolean;
  timeout: number;
  retryCount: number;
}

// 流式响应接口
export interface StreamResponse {
  content: string;
  done: boolean;
  error?: string;
  metadata?: Record<string, any>;
}

// 错误类型
export interface ChatError {
  code: string;
  message: string;
  details?: Record<string, any>;
}

// 查询选项
export interface QueryOptions {
  limit?: number;
  offset?: number;
  sortBy?: string;
  sortOrder?: 'asc' | 'desc';
  filters?: Record<string, any>;
}

// 搜索结果
export interface SearchResult<T> {
  items: T[];
  total: number;
  hasMore: boolean;
}

// 应用类型
export interface Application {
  id: string;
  name: string;
  url: string; // 浏览器访问地址
  iconUrl?: string; // 图标地址
  createdAt: Date;
  updatedAt: Date;
}

// 组件Props类型
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
  messages: Message[];
  isLoading?: boolean;
  isStreaming?: boolean;
  hasMore?: boolean;
  onLoadMore?: () => void;
  onToggleTurnProcess?: (userMessageId: string) => void;
  activeTurnProcessUserMessageId?: string | null;
  loadingTurnProcessUserMessageId?: string | null;
  onMessageEdit?: (messageId: string, content: string) => void;
  onMessageDelete?: (messageId: string) => void;
  customRenderer?: {
    renderMessage?: (message: Message) => ReactNode;
    renderAttachment?: (attachment: Attachment) => ReactNode;
  };
}

export interface InputAreaProps {
  onSend: (content: string, attachments?: File[]) => void;
  onStop?: () => void;
  disabled?: boolean;
  isStreaming?: boolean;
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
  // 智能体选择支持
  selectedAgentId?: string | null;
  availableAgents?: AgentConfig[];
  onAgentChange?: (agentId: string | null) => void;
  // 项目（用于展示智能体关联项目）
  availableProjects?: Project[];
  currentProject?: Project | null;
}

export interface SessionListProps {
  isOpen?: boolean;
  onClose?: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  className?: string;
  store?: any;
}

// 事件类型
export interface ChatEvents {
  onMessageReceived: (message: Message) => void;
  onMessageUpdated: (message: Message) => void;
  onSessionCreated: (session: Session) => void;
  onSessionUpdated: (session: Session) => void;
  onError: (error: ChatError) => void;
}

// 插件接口
export interface ChatPlugin {
  name: string;
  version: string;
  initialize: (config: Record<string, any>) => Promise<void>;
  destroy: () => Promise<void>;
  onMessage?: (message: Message) => Promise<Message | null>;
  onToolCall?: (toolCall: ToolCall) => Promise<any>;
}

// 数据库相关类型
export interface DatabaseOperations {
  // 会话操作
  createSession: (title: string) => Promise<Session>;
  getSession: (id: string) => Promise<Session | null>;
  updateSession: (id: string, updates: Partial<Session>) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  listSessions: () => Promise<Session[]>;

  // 消息操作
  createMessage: (message: Omit<Message, 'id' | 'createdAt'>) => Promise<Message>;
  getMessage: (id: string) => Promise<Message | null>;
  updateMessage: (id: string, updates: Partial<Message>) => Promise<void>;
  deleteMessage: (id: string) => Promise<void>;
  getSessionMessages: (sessionId: string) => Promise<Message[]>;
}

// 导出所有类型
export type {
  ReactNode,
};

import type { UnknownRecord } from './common';

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';
export type MessageStatus = 'pending' | 'streaming' | 'completed' | 'error';
export type AttachmentType = 'image' | 'file' | 'audio';

export interface DraftUserMessageSnapshot {
  id: string;
  content: string;
  createdAt: string;
}

export interface MessageHistoryProcessState {
  hasProcess: boolean;
  toolCallCount: number;
  thinkingCount: number;
  unavailableToolCount?: number;
  processMessageCount: number;
  userMessageId: string;
  turnId: string;
  finalAssistantMessageId: string | null;
  expanded: boolean;
  loaded: boolean;
  loading: boolean;
}

export interface UnavailableToolInfo {
  id: string;
  serverName: string;
  toolName: string;
  reason: string;
  createdAt?: string;
}

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

export interface ToolCall {
  id: string;
  messageId: string;
  name: string;
  arguments: UnknownRecord | string;
  result?: unknown;
  error?: string;
  createdAt: Date;
}

export interface ContentSegment {
  content: string | ToolCall;
  type: 'text' | 'tool_call' | 'thinking';
  toolCallId?: string;
}

export interface MessageMetadata extends UnknownRecord {
  attachments?: Attachment[];
  toolCalls?: ToolCall[];
  contentSegments?: ContentSegment[];
  currentSegmentIndex?: number;
  model?: string;
  summary?: string;
  type?: string;
  conversation_turn_id?: string;
  historyProcess?: MessageHistoryProcessState;
  historyProcessInlineMessages?: Message[];
  historyFinalForUserMessageId?: string;
  historyFinalForTurnId?: string;
  historyProcessUserMessageId?: string;
  historyProcessTurnId?: string;
  historyProcessPlaceholder?: boolean;
  historyProcessLoaded?: boolean;
  historyProcessExpanded?: boolean;
  historyDraftUserMessage?: DraftUserMessageSnapshot;
  unavailableTools?: UnavailableToolInfo[];
  requestError?: string;
}

export interface Message {
  id: string;
  sessionId: string;
  role: MessageRole;
  content: string;
  rawContent?: string;
  summary?: string;
  tokensUsed?: number;
  status: MessageStatus;
  createdAt: Date;
  updatedAt?: Date;
  metadata?: MessageMetadata;
}

export interface Session {
  id: string;
  title: string;
  userId?: string | null;
  user_id?: string | null;
  projectId?: string | null;
  project_id?: string | null;
  createdAt: Date;
  updatedAt: Date;
  messageCount: number;
  tokenUsage: number;
  tags?: string | null;
  pinned: boolean;
  archived: boolean;
  status?: 'active' | 'archiving' | 'archived' | string;
  metadata?: UnknownRecord | string | null;
}

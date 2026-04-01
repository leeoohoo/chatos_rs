import type {
  AiModelConfig,
  Attachment,
  ContentSegment,
  Message,
  ToolCall,
} from '../../../../types';

export interface MessageHistoryProcessState {
  hasProcess: boolean;
  toolCallCount: number;
  thinkingCount: number;
  processMessageCount: number;
  userMessageId: string;
  turnId: string;
  finalAssistantMessageId: string | null;
  expanded: boolean;
  loaded: boolean;
  loading: boolean;
}

export interface DraftUserMessageSnapshot {
  id: string;
  content: string;
  createdAt: string;
}

export interface MessageModelConfigMetadata {
  id: string;
  name: string;
  base_url?: string | null;
  model_name?: string | null;
}

export interface StreamingToolCall extends ToolCall {
  arguments: Record<string, unknown> | string;
  result?: unknown;
  finalResult?: string;
  streamLog?: string;
  completed?: boolean;
}

export type StreamingContentSegment = ContentSegment;

export interface StreamingMessageMetadata extends Record<string, unknown> {
  attachments?: Attachment[];
  toolCalls?: StreamingToolCall[];
  contentSegments?: StreamingContentSegment[];
  currentSegmentIndex?: number;
  model?: string;
  conversation_turn_id?: string;
  modelConfig?: MessageModelConfigMetadata;
  historyProcess?: MessageHistoryProcessState;
  historyFinalForUserMessageId?: string;
  historyFinalForTurnId?: string;
  historyProcessExpanded?: boolean;
  historyDraftUserMessage?: DraftUserMessageSnapshot;
  requestError?: string;
}

export type StreamingMessage = Message;

export interface PreviewAttachment extends Attachment {}

export interface ApiAttachmentPayload {
  name: string;
  mimeType: string;
  size: number;
  type: 'image' | 'file';
  dataUrl?: string;
  text?: string;
}

export interface StreamChatLogPayload {
  session_id: string;
  turn_id: string;
  message: string;
  model_config: {
    model: string;
    provider: string;
    base_url: string;
    api_key: string;
    temperature: number;
    thinking_level?: string | null;
    supports_images: boolean;
    supports_reasoning: boolean;
  };
  system_context: string;
  attachments: ApiAttachmentPayload[];
  reasoning_enabled: boolean;
  contact_agent_id: string | null;
  remote_connection_id: string | null;
  project_id: string;
  project_root: string | null;
  mcp_enabled: boolean;
  enabled_mcp_ids: string[];
}

export interface StreamChatRuntimeOptions {
  turnId: string;
  contactAgentId: string | null;
  remoteConnectionId: string | null;
  projectId: string;
  projectRoot: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
}

export interface StreamEventPayload {
  type?: string;
  content?: unknown;
  data?: unknown;
  result?: {
    content?: unknown;
  } | null;
  success?: boolean;
  is_error?: boolean;
  code?: string;
  message?: string;
  [key: string]: unknown;
}

export interface RawToolFunctionPayload {
  name?: string;
  arguments?: string | Record<string, unknown>;
}

export interface RawToolCallPayload {
  id?: string;
  tool_call_id?: string;
  toolCallId?: string;
  name?: string;
  function?: RawToolFunctionPayload;
  arguments?: string | Record<string, unknown>;
  result?: unknown;
  finalResult?: string;
  final_result?: string;
  streamLog?: string;
  stream_log?: string;
  completed?: boolean;
  error?: string;
  createdAt?: Date | string;
  created_at?: Date | string;
}

export interface RawToolResultPayload {
  id?: string;
  toolCallId?: string;
  tool_call_id?: string;
  result?: unknown;
  content?: unknown;
  output?: unknown;
  success?: boolean;
  is_error?: boolean;
  error?: string;
  chunk?: unknown;
  data?: unknown;
  is_stream?: boolean;
}

export const ensureStreamingMetadata = (
  message: StreamingMessage,
): StreamingMessageMetadata => {
  if (!message.metadata) {
    message.metadata = {};
  }
  return message.metadata;
};

export const ensureStreamingToolCalls = (
  metadata: StreamingMessageMetadata,
): StreamingToolCall[] => {
  if (!Array.isArray(metadata.toolCalls)) {
    metadata.toolCalls = [];
  }
  return metadata.toolCalls as StreamingToolCall[];
};

export const ensureContentSegments = (
  metadata: StreamingMessageMetadata,
): StreamingContentSegment[] => {
  if (!Array.isArray(metadata.contentSegments)) {
    metadata.contentSegments = [];
  }
  return metadata.contentSegments as StreamingContentSegment[];
};

export const touchStreamingMessage = (message: StreamingMessage): void => {
  message.updatedAt = new Date();
};

export const createDefaultHistoryProcessState = ({
  userMessageId,
  turnId,
  finalAssistantMessageId,
}: {
  userMessageId: string;
  turnId: string;
  finalAssistantMessageId: string | null;
}): MessageHistoryProcessState => ({
  hasProcess: false,
  toolCallCount: 0,
  thinkingCount: 0,
  processMessageCount: 0,
  userMessageId,
  turnId,
  finalAssistantMessageId,
  expanded: false,
  loaded: false,
  loading: false,
});

export const buildModelConfigMetadata = (
  selectedModel: AiModelConfig | null | undefined,
): Partial<Pick<StreamingMessageMetadata, 'modelConfig'>> => (
  selectedModel
    ? {
        modelConfig: {
          id: selectedModel.id,
          name: selectedModel.name,
          base_url: selectedModel.base_url,
          model_name: selectedModel.model_name,
        },
      }
    : {}
);

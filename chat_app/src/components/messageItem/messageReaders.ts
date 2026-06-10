import type { Message, ToolCall } from '../../types';
import { asRecord, readBoolean, readValue, type UnknownRecord } from '../../lib/store/helpers/normalizerUtils';

export type MessageToolCallLike = ToolCall & {
  finalResult?: unknown;
  streamLog?: string;
  completed?: boolean;
};

export type MessageUnavailableToolLike = {
  id: string;
  serverName: string;
  toolName: string;
  reason: string;
  createdAt?: string;
};

export const UNAVAILABLE_TOOL_REASON_FALLBACK_KEY = 'message.unavailableToolFallback';

type ToolCallSeed = Partial<Omit<MessageToolCallLike, 'createdAt'>> & {
  createdAt?: unknown;
};

export const normalizeMetaId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const getMessageRecord = (message: Message): UnknownRecord | null => asRecord(message);

const readArray = (record: UnknownRecord | null, key: string): unknown[] => {
  const value = readValue(record, key);
  return Array.isArray(value) ? value : [];
};

const normalizeDateWithFallback = (value: unknown, fallback: Date): Date => {
  if (value instanceof Date) {
    return value;
  }
  if (typeof value === 'string' || typeof value === 'number') {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return fallback;
};

const toFiniteNumber = (value: unknown): number => {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'string' && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return 0;
};

export const getMessageMetadataRecord = (message: Message): UnknownRecord | null => (
  asRecord(message.metadata)
);

export const getMessageHistoryProcessRecord = (message: Message): UnknownRecord | null => (
  asRecord(readValue(getMessageMetadataRecord(message), 'historyProcess'))
);

export const isMessageHistoryProcessExpanded = (message: Message): boolean => (
  readBoolean(getMessageHistoryProcessRecord(message), 'expanded') === true
);

export const hasMessageHistoryProcess = (message: Message): boolean => (
  readBoolean(getMessageHistoryProcessRecord(message), 'hasProcess') === true
);

export const getMessageHistoryProcessToolCount = (message: Message): number => (
  toFiniteNumber(readValue(getMessageHistoryProcessRecord(message), 'toolCallCount'))
);

export const getMessageHistoryProcessThinkingCount = (message: Message): number => (
  toFiniteNumber(readValue(getMessageHistoryProcessRecord(message), 'thinkingCount'))
);

export const getMessageHistoryProcessUnavailableToolCount = (message: Message): number => (
  toFiniteNumber(readValue(getMessageHistoryProcessRecord(message), 'unavailableToolCount'))
);

export const getMessageHistoryProcessTurnId = (message: Message): string => {
  const metadataRecord = getMessageMetadataRecord(message);
  return normalizeTurnId(
    readValue(metadataRecord, 'historyProcessTurnId')
    || readValue(getMessageHistoryProcessRecord(message), 'turnId'),
  );
};

export const getMessageHistoryProcessFinalAssistantMessageId = (message: Message): string => (
  normalizeMetaId(readValue(getMessageHistoryProcessRecord(message), 'finalAssistantMessageId'))
);

export const getMessageHistoryProcessUserMessageId = (message: Message): string => (
  normalizeMetaId(readValue(getMessageMetadataRecord(message), 'historyProcessUserMessageId'))
);

export const getMessageHistoryFinalForUserMessageId = (message: Message): string => (
  normalizeMetaId(readValue(getMessageMetadataRecord(message), 'historyFinalForUserMessageId'))
);

export const getMessageHistoryFinalForTurnId = (message: Message): string => (
  normalizeTurnId(readValue(getMessageMetadataRecord(message), 'historyFinalForTurnId'))
);

export const isMessageHistoryProcessPlaceholder = (message: Message): boolean => (
  readValue(getMessageMetadataRecord(message), 'historyProcessPlaceholder') === true
);

export const getMessageConversationTurnId = (message: Message): string => (
  normalizeTurnId(
    readValue(getMessageMetadataRecord(message), 'conversation_turn_id')
    || readValue(getMessageMetadataRecord(message), 'conversationTurnId'),
  )
);

export const getMessageContentSegments = (message: Message): unknown[] => (
  readArray(getMessageMetadataRecord(message), 'contentSegments')
);

const buildArgumentsValue = (
  value: unknown,
  fallback: ToolCall['arguments'] = {},
): ToolCall['arguments'] => {
  if (typeof value === 'string') {
    return value;
  }
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    return value as ToolCall['arguments'];
  }
  return fallback;
};

export const buildRenderableToolCall = (
  toolCall: ToolCallSeed | null | undefined,
  message: Message,
  defaults: Partial<Pick<ToolCall, 'id' | 'messageId' | 'name' | 'arguments' | 'createdAt'>> = {},
): MessageToolCallLike => ({
  id: normalizeMetaId(toolCall?.id) || normalizeMetaId(defaults.id) || '',
  messageId: normalizeMetaId(toolCall?.messageId) || defaults.messageId || message.id,
  name: typeof toolCall?.name === 'string' ? toolCall.name : (defaults.name || ''),
  arguments: buildArgumentsValue(toolCall?.arguments, defaults.arguments ?? {}),
  result: toolCall?.result,
  error: typeof toolCall?.error === 'string' ? toolCall.error : undefined,
  createdAt: defaults.createdAt || normalizeDateWithFallback(toolCall?.createdAt, message.createdAt),
  finalResult: toolCall?.finalResult,
  streamLog: typeof toolCall?.streamLog === 'string' ? toolCall.streamLog : undefined,
  completed: toolCall?.completed === true,
});

const normalizeToolCall = (value: unknown, message: Message): MessageToolCallLike => {
  const record = asRecord(value);
  if (!record) {
    return buildRenderableToolCall(null, message);
  }

  const argumentsValue = readValue(record, 'arguments');
  const normalizedArguments = buildArgumentsValue(argumentsValue);

  return buildRenderableToolCall({
    id: normalizeMetaId(readValue(record, 'id')),
    messageId: normalizeMetaId(
      readValue(record, 'messageId') || readValue(record, 'message_id'),
    ),
    name: typeof readValue(record, 'name') === 'string'
      ? String(readValue(record, 'name'))
      : '',
    arguments: normalizedArguments,
    result: readValue(record, 'result'),
    error: typeof readValue(record, 'error') === 'string'
      ? String(readValue(record, 'error'))
      : undefined,
    createdAt: readValue(record, 'createdAt') || readValue(record, 'created_at'),
    finalResult: readValue(record, 'finalResult') || readValue(record, 'final_result'),
    streamLog: typeof readValue(record, 'streamLog') === 'string'
      ? String(readValue(record, 'streamLog'))
      : undefined,
    completed: readBoolean(record, 'completed') === true,
  }, message);
};

export const getMessageMetadataToolCalls = (message: Message): MessageToolCallLike[] => (
  readArray(getMessageMetadataRecord(message), 'toolCalls').map((toolCall) => normalizeToolCall(toolCall, message))
);

export const getMessageTopLevelToolCalls = (message: Message): MessageToolCallLike[] => (
  readArray(getMessageRecord(message), 'toolCalls').map((toolCall) => normalizeToolCall(toolCall, message))
);

export const getMessagePrimaryToolCalls = (message: Message): MessageToolCallLike[] => {
  const messageRecord = getMessageRecord(message);
  const topLevel = readArray(messageRecord, 'toolCalls');
  if (topLevel.length > 0) {
    return topLevel.map((toolCall) => normalizeToolCall(toolCall, message));
  }

  return getMessageMetadataToolCalls(message);
};

export const getMessageAllToolCalls = (message: Message): MessageToolCallLike[] => (
  [
    ...getMessageMetadataToolCalls(message),
    ...getMessageTopLevelToolCalls(message),
  ]
);

export const getMessageToolResultCallId = (message: Message): string => {
  const messageRecord = getMessageRecord(message);
  const metadataRecord = getMessageMetadataRecord(message);
  const value = (
    readValue(messageRecord, 'tool_call_id')
    || readValue(messageRecord, 'toolCallId')
    || readValue(metadataRecord, 'tool_call_id')
    || readValue(metadataRecord, 'toolCallId')
  );
  return value ? String(value) : '';
};

export const normalizeUnavailableTools = (value: unknown): MessageUnavailableToolLike[] => {
  if (!Array.isArray(value)) {
    return [];
  }

  return value.map((item, index) => {
    const record = asRecord(item);
    return {
      id: normalizeMetaId(readValue(record, 'id')) || `unavailable_${index}`,
      serverName: normalizeMetaId(
        readValue(record, 'serverName') || readValue(record, 'server_name'),
      ) || 'unknown_server',
      toolName: normalizeMetaId(
        readValue(record, 'toolName') || readValue(record, 'tool_name'),
      ) || 'unknown_tool',
      reason: normalizeMetaId(readValue(record, 'reason')) || UNAVAILABLE_TOOL_REASON_FALLBACK_KEY,
      createdAt: normalizeMetaId(
        readValue(record, 'createdAt') || readValue(record, 'created_at'),
      ) || undefined,
    };
  });
};

export const getMessageUnavailableTools = (message: Message): MessageUnavailableToolLike[] => (
  normalizeUnavailableTools(readValue(getMessageMetadataRecord(message), 'unavailableTools'))
);

export const getMessageKeepLastN = (message: Message): number | null => {
  const value = readValue(getMessageMetadataRecord(message), 'keepLastN');
  return typeof value === 'number' ? value : null;
};

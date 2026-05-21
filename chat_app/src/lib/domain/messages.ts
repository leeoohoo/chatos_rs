import type { Attachment, ContentSegment, Message, ToolCall } from '../../types';
import type { SessionMessageResponse } from '../api/client/types';
import {
  asRecord,
  normalizeDate,
  readValue,
  type UnknownRecord,
} from './normalizerUtils';

type ToolCallWithExtras = ToolCall & {
  completed?: boolean;
  finalResult?: unknown;
  streamLog?: string;
};

type NormalizedContentSegment =
  | Pick<ContentSegment, 'type' | 'content'>
  | Pick<ContentSegment, 'type' | 'toolCallId'>;

type NormalizedRawMessage = {
  content: string;
  createdAt: Date;
  id: string;
  metadata: UnknownRecord | null;
  reasoning: unknown;
  role: Message['role'];
  sessionId: string;
  status: Message['status'];
  summary?: string;
  summaryId?: string | null;
  summarizedAt?: string | null;
  summaryStatus?: string | null;
  toolCallId: string;
  topLevelToolCalls: UnknownRecord[];
};

type MessageMetadata = NonNullable<Message['metadata']>;

const parseMaybeJson = (value: unknown): unknown => {
  if (typeof value !== 'string') return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
};

const normalizeToolCallsArray = (value: unknown): UnknownRecord[] => {
  const parsed = parseMaybeJson(value);
  if (Array.isArray(parsed)) {
    return parsed
      .map((item) => asRecord(item))
      .filter((item): item is UnknownRecord => item !== null);
  }
  const parsedRecord = asRecord(parsed);
  if (parsedRecord) return [parsedRecord];
  return [];
};

const normalizeContentSegmentsArray = (value: unknown): NormalizedContentSegment[] => {
  const parsed = parseMaybeJson(value);
  if (!Array.isArray(parsed)) {
    return [];
  }

  return parsed
    .map((segment): NormalizedContentSegment | null => {
      const segmentRecord = asRecord(segment);
      if (!segmentRecord) {
        return null;
      }

      const rawType = String(readValue(segmentRecord, 'type') || '').trim().toLowerCase();
      const normalizedType =
        rawType === 'tool' || rawType === 'toolcall'
          ? 'tool_call'
          : rawType === 'tool_call' || rawType === 'thinking' || rawType === 'text'
            ? rawType
            : '';

      if (!normalizedType) {
        return null;
      }

      if (normalizedType === 'tool_call') {
        const toolCallId =
          readValue(segmentRecord, 'toolCallId')
          || readValue(segmentRecord, 'tool_call_id')
          || readValue(segmentRecord, 'tool_callId')
          || readValue(segmentRecord, 'toolCallID')
          || readValue(segmentRecord, 'tool_call_ID');

        if (!toolCallId) {
          return null;
        }

        return {
          type: 'tool_call',
          toolCallId: String(toolCallId),
        };
      }

      const rawContent = readValue(segmentRecord, 'content');
      const content = typeof rawContent === 'string' ? rawContent : String(rawContent ?? '');
      if (!content) {
        return null;
      }

      return {
        type: normalizedType,
        content,
      };
    })
    .filter((item): item is NormalizedContentSegment => item !== null);
};

export const isNonEmptyString = (value: unknown): value is string => (
  typeof value === 'string' && value.trim().length > 0
);

const normalizeToolCallId = (value: unknown): string => {
  if (!value) {
    return '';
  }
  return String(value).trim();
};

const extractStructuredToolResult = (metadata: UnknownRecord | null | undefined): unknown => {
  const metadataRecord = metadata ?? null;
  return readValue(metadataRecord, 'structured_result') ?? readValue(metadataRecord, 'structuredResult');
};

export const isMeaningfulReasoning = (value: unknown): value is string => {
  if (!isNonEmptyString(value)) {
    return false;
  }

  const normalized = value.trim().toLowerCase();
  return !['minimal', 'low', 'medium', 'high', 'detailed'].includes(normalized);
};

const parseMessageMetadata = (metadata: unknown): UnknownRecord | null => {
  if (!metadata) {
    return null;
  }
  if (typeof metadata !== 'string') {
    return asRecord(metadata);
  }
  try {
    return asRecord(JSON.parse(metadata));
  } catch {
    return {};
  }
};

const normalizeMessageStatus = (value: unknown): Message['status'] => {
  const normalized = typeof value === 'string' ? value.trim().toLowerCase() : '';
  if (normalized === 'in_progress' || normalized === 'queued' || normalized === 'pending' || normalized === 'incomplete') {
    return 'streaming';
  }
  if (normalized === 'streaming') {
    return 'streaming';
  }
  if (normalized === 'error' || normalized === 'failed' || normalized === 'cancelled' || normalized === 'canceled') {
    return 'error';
  }
  if (normalized === 'completed' || normalized === 'complete' || normalized === 'done') {
    return 'completed';
  }
  return 'completed';
};

export const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const getConversationTurnId = (message: { metadata?: unknown }): string => {
  const metadataRecord = asRecord(message.metadata);
  return normalizeTurnId(
    readValue(metadataRecord, 'conversation_turn_id') || readValue(metadataRecord, 'conversationTurnId'),
  );
};

const normalizeAttachments = (
  metadata: unknown,
  messageId: string,
  createdAt: Date,
): Attachment[] | undefined => {
  const metadataRecord = asRecord(metadata);
  const rawAttachments = readValue(metadataRecord, 'attachments');
  if (!Array.isArray(rawAttachments) || rawAttachments.length === 0) {
    return undefined;
  }

  return rawAttachments.map((attachment, index) => {
    const attachmentRecord = asRecord(attachment) ?? {};
    const mime = String(
      readValue(attachmentRecord, 'mimeType')
      || readValue(attachmentRecord, 'mime')
      || 'application/octet-stream',
    );
    const hasPreview = Boolean(readValue(attachmentRecord, 'preview') || readValue(attachmentRecord, 'url'));
    const baseType = mime.startsWith('image/')
      ? 'image'
      : mime.startsWith('audio/')
        ? 'audio'
        : 'file';
    const typeValue = readValue(attachmentRecord, 'type');
    const type = hasPreview
      ? (typeof typeValue === 'string' ? typeValue : baseType)
      : (baseType === 'image' ? 'file' : baseType);

    return {
      id: String(readValue(attachmentRecord, 'id') || `${messageId}_att_${index}`),
      messageId,
      type: type as Attachment['type'],
      name: String(readValue(attachmentRecord, 'name') || `attachment-${index + 1}`),
      url: String(readValue(attachmentRecord, 'preview') || readValue(attachmentRecord, 'url') || ''),
      size: Number(readValue(attachmentRecord, 'size') || 0),
      mimeType: mime,
      createdAt,
    };
  });
};

export const normalizeRawMessages = (
  rawMessages: SessionMessageResponse[],
  sessionId: string,
): Message[] => {
  const parsedMessages: NormalizedRawMessage[] = rawMessages.map((message) => {
    const messageRecord = asRecord(message) ?? {};
    const metadata = parseMessageMetadata(readValue(messageRecord, 'metadata'));
    const topLevelToolCalls = normalizeToolCallsArray(
      readValue(messageRecord, 'toolCalls') ?? readValue(messageRecord, 'tool_calls'),
    );
    const rawContent = readValue(messageRecord, 'content');
    const conversationId = readValue(messageRecord, 'conversation_id') ?? readValue(messageRecord, 'conversationId');

    return {
      id: String(readValue(messageRecord, 'id') ?? ''),
      sessionId: typeof conversationId === 'string' ? conversationId : sessionId,
      role: readValue(messageRecord, 'role') as Message['role'],
      content: typeof rawContent === 'string' ? rawContent : String(rawContent ?? ''),
      status: normalizeMessageStatus(
        readValue(metadata, 'response_status')
        ?? readValue(metadata, 'responseStatus')
        ?? readValue(metadata, 'finish_reason')
        ?? readValue(metadata, 'finishReason')
        ?? readValue(metadata, 'status'),
      ),
      summary: readValue(messageRecord, 'summary') as string | undefined,
      summaryStatus: typeof readValue(messageRecord, 'summary_status') === 'string'
        ? String(readValue(messageRecord, 'summary_status'))
        : null,
      summaryId: typeof readValue(messageRecord, 'summary_id') === 'string'
        ? String(readValue(messageRecord, 'summary_id'))
        : null,
      summarizedAt: typeof readValue(messageRecord, 'summarized_at') === 'string'
        ? String(readValue(messageRecord, 'summarized_at'))
        : null,
      toolCallId: normalizeToolCallId(
        readValue(messageRecord, 'tool_call_id') ?? readValue(messageRecord, 'toolCallId'),
      ),
      reasoning: readValue(messageRecord, 'reasoning'),
      metadata,
      topLevelToolCalls,
      createdAt: normalizeDate(readValue(messageRecord, 'created_at') ?? readValue(messageRecord, 'createdAt')),
    };
  });

  const toolResultsMap = new Map<string, { content: unknown; error?: string }>();
  parsedMessages.forEach((message) => {
    const toolCallId = normalizeToolCallId(message.toolCallId);
    if (message.role !== 'tool' || !toolCallId) {
      return;
    }

    const metadataRecord = message.metadata;
    const isError = readValue(metadataRecord, 'isError') || readValue(metadataRecord, 'is_error') || false;
    const structuredResult = extractStructuredToolResult(message.metadata);
    toolResultsMap.set(toolCallId, {
      content: structuredResult !== undefined ? structuredResult : message.content,
      error: isError ? message.content : undefined,
    });
  });

  return parsedMessages.map((message) => {
    let toolCalls: ToolCallWithExtras[] | undefined;
    const metadataRecord = message.metadata;
    const sourceToolCalls = message.topLevelToolCalls.length > 0
      ? message.topLevelToolCalls
      : normalizeToolCallsArray(
        readValue(metadataRecord, 'toolCalls') ?? readValue(metadataRecord, 'tool_calls'),
      );

    if (message.role === 'assistant' && sourceToolCalls.length > 0) {
      toolCalls = sourceToolCalls.map((toolCall) => {
        const toolCallRecord = asRecord(toolCall) ?? {};
        const toolCallId = normalizeToolCallId(
          readValue(toolCallRecord, 'id')
            || readValue(toolCallRecord, 'tool_call_id')
            || readValue(toolCallRecord, 'toolCallId'),
        ) || `tool_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
        const toolResult = toolResultsMap.get(toolCallId);
        const functionRecord = asRecord(readValue(toolCallRecord, 'function'));

        if (functionRecord) {
          let parsedArguments: ToolCall['arguments'] = {};
          const rawArguments = readValue(functionRecord, 'arguments');
          try {
            parsedArguments = typeof rawArguments === 'string'
              ? JSON.parse(rawArguments) as Record<string, unknown>
              : (rawArguments as ToolCall['arguments']);
          } catch {
            parsedArguments = {};
          }

          return {
            id: toolCallId,
            messageId: message.id,
            name: String(readValue(functionRecord, 'name') || ''),
            arguments: parsedArguments,
            result: toolResult?.content,
            error: toolResult?.error,
            createdAt: message.createdAt,
          };
        }

        let parsedArguments = (readValue(toolCallRecord, 'arguments') ?? readValue(toolCallRecord, 'args') ?? {}) as ToolCall['arguments'];
        if (typeof parsedArguments === 'string') {
          try {
            parsedArguments = JSON.parse(parsedArguments) as Record<string, unknown>;
          } catch {
            // keep the original string when parsing fails
          }
        }

        const toolError = readValue(toolCallRecord, 'error');
        const rawCreatedAt = readValue(toolCallRecord, 'createdAt') ?? readValue(toolCallRecord, 'created_at');

        return {
          id: toolCallId,
          messageId: message.id,
          name: String(
            readValue(toolCallRecord, 'name')
            || readValue(toolCallRecord, 'tool_name')
            || readValue(toolCallRecord, 'toolName')
            || 'unknown_tool',
          ),
          arguments: parsedArguments,
          result: readValue(toolCallRecord, 'result')
            ?? readValue(toolCallRecord, 'finalResult')
            ?? readValue(toolCallRecord, 'final_result')
            ?? toolResult?.content,
          finalResult: readValue(toolCallRecord, 'finalResult') ?? readValue(toolCallRecord, 'final_result'),
          streamLog: String(readValue(toolCallRecord, 'streamLog') ?? readValue(toolCallRecord, 'stream_log') ?? ''),
          completed: readValue(toolCallRecord, 'completed') === true,
          error: typeof toolError === 'string' ? toolError : toolResult?.error || undefined,
          createdAt: rawCreatedAt !== undefined ? normalizeDate(rawCreatedAt) : message.createdAt,
        };
      });
    }

    const existingContentSegments = normalizeContentSegmentsArray(
      readValue(metadataRecord, 'contentSegments') ?? readValue(metadataRecord, 'content_segments'),
    );

    const fallbackContentSegments: NormalizedContentSegment[] = [];
    if (message.role === 'assistant' && isMeaningfulReasoning(message.reasoning)) {
      fallbackContentSegments.push({ type: 'thinking', content: message.reasoning });
    }
    if (toolCalls && toolCalls.length > 0) {
      toolCalls.forEach((toolCall) => {
        if (toolCall?.id) {
          fallbackContentSegments.push({ type: 'tool_call', toolCallId: toolCall.id });
        }
      });
    }
    if (isNonEmptyString(message.content)) {
      fallbackContentSegments.push({ type: 'text', content: message.content });
    }

    const attachments = normalizeAttachments(message.metadata, message.id, message.createdAt);

    return {
      id: message.id,
      sessionId: message.sessionId,
      role: message.role,
      content: message.content,
      rawContent: message.summary,
      tokensUsed: undefined,
      status: message.status,
      createdAt: message.createdAt,
      updatedAt: undefined,
      summaryStatus: message.summaryStatus,
      summaryId: message.summaryId,
      summarizedAt: message.summarizedAt,
      toolCallId: message.toolCallId,
      metadata: {
        ...((message.metadata !== null && typeof message.metadata === 'object')
          ? message.metadata as Record<string, unknown>
          : {}),
        ...(attachments ? { attachments } : {}),
        toolCalls: toolCalls as MessageMetadata['toolCalls'],
        contentSegments: (
          existingContentSegments.length > 0 ? existingContentSegments : fallbackContentSegments
        ) as MessageMetadata['contentSegments'],
      },
    };
  });
};

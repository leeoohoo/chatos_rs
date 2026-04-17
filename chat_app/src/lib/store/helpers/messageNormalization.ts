import type { Message } from '../../../types';

const parseMaybeJson = (value: any): any => {
  if (typeof value !== 'string') return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
};

const normalizeToolCallsArray = (value: any): any[] => {
  const parsed = parseMaybeJson(value);
  if (Array.isArray(parsed)) return parsed;
  if (parsed && typeof parsed === 'object') return [parsed];
  return [];
};

const normalizeContentSegmentsArray = (value: any): any[] => {
  const parsed = parseMaybeJson(value);
  if (!Array.isArray(parsed)) {
    return [];
  }

  return parsed
    .map((segment: any) => {
      if (!segment || typeof segment !== 'object') {
        return null;
      }

      const rawType = String(segment.type || '').trim().toLowerCase();
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
          segment.toolCallId ||
          segment.tool_call_id ||
          segment.tool_callId ||
          segment.toolCallID ||
          segment.tool_call_ID;

        if (!toolCallId) {
          return null;
        }

        return {
          type: 'tool_call',
          toolCallId: String(toolCallId),
        };
      }

      const content = typeof segment.content === 'string' ? segment.content : String(segment.content ?? '');
      if (!content) {
        return null;
      }

      return {
        type: normalizedType,
        content,
      };
    })
    .filter(Boolean);
};

export const isNonEmptyString = (value: unknown): value is string => (
  typeof value === 'string' && value.trim().length > 0
);

const normalizeToolCallId = (value: any): string => {
  if (!value) {
    return '';
  }
  return String(value).trim();
};

const extractStructuredToolResult = (metadata: any): any => (
  metadata?.structured_result ?? metadata?.structuredResult
);

export const isMeaningfulReasoning = (value: unknown): value is string => {
  if (!isNonEmptyString(value)) {
    return false;
  }

  const normalized = value.trim().toLowerCase();
  return !['minimal', 'low', 'medium', 'high', 'detailed'].includes(normalized);
};

const parseMessageMetadata = (metadata: any): any => {
  if (!metadata) {
    return undefined;
  }
  if (typeof metadata !== 'string') {
    return metadata;
  }
  try {
    return JSON.parse(metadata);
  } catch {
    return {};
  }
};

const normalizeDate = (value: unknown): Date => {
  const date = new Date(value as any);
  return Number.isNaN(date.getTime()) ? new Date() : date;
};

export const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const getConversationTurnId = (message: any): string => (
  normalizeTurnId(message?.metadata?.conversation_turn_id || message?.metadata?.conversationTurnId)
);

const normalizeAttachments = (metadata: any, messageId: string, createdAt: Date): any[] | undefined => {
  const rawAttachments = metadata?.attachments;
  if (!Array.isArray(rawAttachments) || rawAttachments.length === 0) {
    return undefined;
  }

  return rawAttachments.map((attachment: any, index: number) => {
    const mime = attachment.mimeType || attachment.mime || 'application/octet-stream';
    const hasPreview = Boolean(attachment.preview || attachment.url);
    const baseType = mime.startsWith('image/')
      ? 'image'
      : mime.startsWith('audio/')
        ? 'audio'
        : 'file';
    const type = hasPreview ? (attachment.type || baseType) : (baseType === 'image' ? 'file' : baseType);

    return {
      id: attachment.id || `${messageId}_att_${index}`,
      messageId,
      type,
      name: attachment.name || `attachment-${index + 1}`,
      url: attachment.preview || attachment.url || '',
      size: attachment.size || 0,
      mimeType: mime,
      createdAt,
    };
  });
};

export const normalizeRawMessages = (rawMessages: any[], sessionId: string): Message[] => {
  const parsedMessages = rawMessages.map((message: any) => {
    const metadata = parseMessageMetadata(message.metadata);
    const topLevelToolCalls = normalizeToolCallsArray(message.toolCalls ?? message.tool_calls);

    return {
      id: message.id,
      sessionId: message.conversation_id ?? message.conversationId ?? sessionId,
      role: message.role as 'user' | 'assistant' | 'system' | 'tool',
      content: typeof message.content === 'string' ? message.content : String(message.content ?? ''),
      summary: message.summary,
      toolCallId: message.tool_call_id ?? message.toolCallId,
      reasoning: message.reasoning,
      metadata,
      topLevelToolCalls,
      createdAt: normalizeDate(message.created_at ?? message.createdAt),
    };
  });

  const toolResultsMap = new Map<string, { content: any; error?: string }>();
  parsedMessages.forEach((message) => {
    const toolCallId = normalizeToolCallId(message.toolCallId);
    if (message.role !== 'tool' || !toolCallId) {
      return;
    }

    const isError = message.metadata?.isError || message.metadata?.is_error || false;
    const structuredResult = extractStructuredToolResult(message.metadata);
    toolResultsMap.set(toolCallId, {
      content: structuredResult !== undefined ? structuredResult : message.content,
      error: isError ? message.content : undefined,
    });
  });

  return parsedMessages.map((message) => {
    let toolCalls: any[] | undefined;
    const sourceToolCalls = message.topLevelToolCalls.length > 0
      ? message.topLevelToolCalls
      : normalizeToolCallsArray(message.metadata?.toolCalls ?? message.metadata?.tool_calls);

    if (message.role === 'assistant' && sourceToolCalls.length > 0) {
      toolCalls = sourceToolCalls.map((toolCall: any) => {
        const toolCallId = normalizeToolCallId(
          toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId,
        ) || `tool_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
        const toolResult = toolResultsMap.get(toolCallId);

        if (toolCall.function) {
          let parsedArguments: Record<string, any> | string = {};
          try {
            parsedArguments = typeof toolCall.function.arguments === 'string'
              ? JSON.parse(toolCall.function.arguments)
              : toolCall.function.arguments;
          } catch {
            parsedArguments = {};
          }

          return {
            id: toolCallId,
            messageId: message.id,
            name: toolCall.function.name,
            arguments: parsedArguments,
            result: toolResult?.content,
            error: toolResult?.error,
            createdAt: message.createdAt,
          };
        }

        let parsedArguments = toolCall.arguments ?? toolCall.args ?? {};
        if (typeof parsedArguments === 'string') {
          try {
            parsedArguments = JSON.parse(parsedArguments);
          } catch {
            // keep the original string when parsing fails
          }
        }

        return {
          id: toolCallId,
          messageId: message.id,
          name: toolCall.name || toolCall.tool_name || toolCall.toolName || 'unknown_tool',
          arguments: parsedArguments,
          result: toolCall.result ?? toolCall.finalResult ?? toolCall.final_result ?? toolResult?.content,
          finalResult: toolCall.finalResult ?? toolCall.final_result,
          streamLog: toolCall.streamLog ?? toolCall.stream_log ?? '',
          completed: toolCall.completed === true,
          error: toolCall.error || toolResult?.error || undefined,
          createdAt: toolCall.createdAt || toolCall.created_at || message.createdAt,
        };
      });
    }

    const existingContentSegments = normalizeContentSegmentsArray(
      message.metadata?.contentSegments ?? message.metadata?.content_segments,
    );

    const fallbackContentSegments: any[] = [];
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
      status: 'completed' as const,
      createdAt: message.createdAt,
      updatedAt: undefined,
      toolCallId: message.toolCallId,
      metadata: {
        ...message.metadata,
        ...(attachments ? { attachments } : {}),
        toolCalls,
        contentSegments: existingContentSegments.length > 0 ? existingContentSegments : fallbackContentSegments,
      },
    };
  });
};

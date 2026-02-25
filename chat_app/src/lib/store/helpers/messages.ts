import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';

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

const isNonEmptyString = (value: unknown): value is string => (
  typeof value === 'string' && value.trim().length > 0
);

const normalizeToolCallId = (value: any): string => {
  if (!value) {
    return '';
  }
  return String(value).trim();
};

const isMeaningfulReasoning = (value: unknown): value is string => {
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

const normalizeRawMessages = (rawMessages: any[], sessionId: string): Message[] => {
  const parsedMessages = rawMessages.map((message: any) => {
    const metadata = parseMessageMetadata(message.metadata);
    const topLevelToolCalls = normalizeToolCallsArray(message.toolCalls ?? message.tool_calls);

    return {
      id: message.id,
      sessionId: message.session_id ?? message.sessionId ?? sessionId,
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

  const toolResultsMap = new Map<string, { content: string; error?: string }>();
  parsedMessages.forEach((message) => {
    const toolCallId = normalizeToolCallId(message.toolCallId);
    if (message.role !== 'tool' || !toolCallId) {
      return;
    }

    const isError = message.metadata?.isError || message.metadata?.is_error || false;
    toolResultsMap.set(toolCallId, {
      content: message.content,
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



const countThinkingSegments = (message: Message): number => {
  const segments = message.metadata?.contentSegments;
  if (!Array.isArray(segments) || segments.length === 0) {
    return 0;
  }

  return segments.filter((segment: any) => (
    segment?.type === 'thinking' && isMeaningfulReasoning(segment?.content)
  )).length;
};

const isSessionSummaryMessage = (message: Message): boolean => (
  message.role === 'assistant' && message.metadata?.type === 'session_summary'
);

const ensureCompactHistoryShape = (messages: Message[]): Message[] => {
  if (!Array.isArray(messages) || messages.length === 0) {
    return messages;
  }

  const hasServerCompactMarkers = messages.some((message) => (
    message.role === 'user' && Boolean(message.metadata?.historyProcess)
  ));
  if (hasServerCompactMarkers) {
    return messages
      .filter((message) => message.metadata?.historyProcessPlaceholder !== true)
      .map((message) => {
        if (!message.metadata?.historyFinalForUserMessageId) {
          return message;
        }

        return {
          ...message,
          metadata: {
            ...(message.metadata || {}),
            historyProcessExpanded: message.metadata?.historyProcessExpanded === true,
          },
        };
      });
  }

  const userIndexes: number[] = [];
  messages.forEach((message, index) => {
    if (message.role === 'user') {
      userIndexes.push(index);
    }
  });

  if (userIndexes.length === 0) {
    return messages;
  }

  const result: Message[] = [];

  userIndexes.forEach((userIndex, userPos) => {
    const nextUserIndex = userPos + 1 < userIndexes.length
      ? userIndexes[userPos + 1]
      : messages.length;
    const userMessage = messages[userIndex];
    const userMessageId = userMessage.id;

    let finalAssistantIndex = -1;
    for (let i = nextUserIndex - 1; i > userIndex; i -= 1) {
      const candidate = messages[i];
      if (candidate.role !== 'assistant' || isSessionSummaryMessage(candidate)) {
        continue;
      }
      finalAssistantIndex = i;
      if (isNonEmptyString(candidate.content)) {
        break;
      }
    }

    let toolCallCount = 0;
    let thinkingCount = 0;
    const inlineProcessMessages: Message[] = [];

    for (let i = userIndex + 1; i < nextUserIndex; i += 1) {
      const message = messages[i];
      if (message.role === 'assistant' && !isSessionSummaryMessage(message)) {
        toolCallCount += message.metadata?.toolCalls?.length || 0;
        thinkingCount += countThinkingSegments(message);
      }

      if (i !== finalAssistantIndex && (message.role === 'assistant' || message.role === 'tool')) {
        if (message.role === 'assistant' && isSessionSummaryMessage(message)) {
          continue;
        }

        inlineProcessMessages.push({
          ...message,
          metadata: {
            ...(message.metadata || {}),
            hidden: false,
            historyProcessPlaceholder: false,
            historyProcessLoaded: true,
            historyProcessUserMessageId: userMessageId,
          },
        });
      }
    }

    const processMessageCount = inlineProcessMessages.length;

    result.push({
      ...userMessage,
      metadata: {
        ...(userMessage.metadata || {}),
        historyProcess: {
          hasProcess: processMessageCount > 0,
          toolCallCount,
          thinkingCount,
          processMessageCount,
          userMessageId,
          finalAssistantMessageId: finalAssistantIndex >= 0 ? messages[finalAssistantIndex].id : null,
        },
        ...(processMessageCount > 0 ? { historyProcessInlineMessages: inlineProcessMessages } : {}),
      },
    });

    if (finalAssistantIndex < 0) {
      return;
    }

    const finalAssistant = messages[finalAssistantIndex];
    const textSegments = Array.isArray(finalAssistant.metadata?.contentSegments)
      ? finalAssistant.metadata?.contentSegments.filter((segment: any) => segment?.type === 'text')
      : [];

    result.push({
      ...finalAssistant,
      metadata: {
        ...(finalAssistant.metadata || {}),
        toolCalls: [],
        contentSegments: textSegments,
        hidden: false,
        historyFinalForUserMessageId: userMessageId,
        historyProcessExpanded: false,
      },
    });
  });

  return result;
};

export const fetchSessionMessages = async (
  client: ApiClient,
  sessionId: string,
  options: { limit?: number; offset?: number } = { limit: 50, offset: 0 },
): Promise<Message[]> => {
  const limit = options.limit ?? 50;
  const offset = options.offset ?? 0;

  const rawMessages = await client.getSessionMessages(sessionId, {
    limit,
    offset,
    compact: true,
  });

  const normalized = ensureCompactHistoryShape(normalizeRawMessages(rawMessages, sessionId));
  debugLog('[Store] Loaded compact session messages', {
    sessionId,
    requested: { limit, offset },
    received: normalized.length,
  });
  return normalized;
};

export const fetchTurnProcessMessages = async (
  client: ApiClient,
  sessionId: string,
  userMessageId: string,
): Promise<Message[]> => {
  if (!userMessageId) {
    return [];
  }

  const rawMessages = await client.getSessionTurnProcessMessages(sessionId, userMessageId);
  const normalized = normalizeRawMessages(rawMessages, sessionId);

  return normalized.map((message) => ({
    ...message,
    metadata: {
      ...message.metadata,
      hidden: false,
      historyProcessPlaceholder: false,
      historyProcessLoaded: true,
      historyProcessUserMessageId: userMessageId,
      historyProcessExpanded: true,
    },
  }));
};


export type TurnProcessState = {
  expanded: boolean;
  loaded: boolean;
  loading: boolean;
};

const withUserProcessMeta = (
  message: Message,
  state?: Partial<TurnProcessState>,
): Message => {
  if (message.role !== 'user') {
    return message;
  }

  const historyProcess = message.metadata?.historyProcess;
  if (!historyProcess || typeof historyProcess !== 'object') {
    return message;
  }

  const nextHistoryProcess = {
    ...historyProcess,
    ...(state || {}),
  };

  return {
    ...message,
    metadata: {
      ...(message.metadata || {}),
      historyProcess: nextHistoryProcess,
    },
  };
};

export const setTurnProcessExpanded = (
  messages: Message[],
  userMessageId: string,
  expanded: boolean,
): Message[] => {
  return messages.map((message) => {
    if (message.id === userMessageId) {
      return withUserProcessMeta(message, { expanded });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    if (finalForUserMessageId === userMessageId) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: expanded,
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    if (turnUserMessageId !== userMessageId) {
      return message;
    }

    return {
      ...message,
      metadata: {
        ...(message.metadata || {}),
        hidden: !expanded,
        historyProcessExpanded: expanded,
      },
    };
  });
};

export const mergeTurnProcessMessages = (
  messages: Message[],
  userMessageId: string,
  processMessages: Message[],
  expanded: boolean,
): Message[] => {
  const processById = new Map<string, Message>();
  processMessages.forEach((message) => {
    processById.set(message.id, message);
  });

  const merged = messages.map((message) => {
    if (message.id === userMessageId) {
      return withUserProcessMeta(message, { expanded, loaded: true, loading: false });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    if (finalForUserMessageId === userMessageId) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: expanded,
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    if (turnUserMessageId !== userMessageId) {
      return message;
    }

    const hydrated = processById.get(message.id) || message;
    return {
      ...hydrated,
      metadata: {
        ...(hydrated.metadata || {}),
        hidden: !expanded,
        historyProcessPlaceholder: false,
        historyProcessLoaded: true,
        historyProcessUserMessageId: userMessageId,
        historyProcessExpanded: expanded,
      },
    };
  });

  const existingIds = new Set(merged.map((message) => message.id));
  const missingMessages = processMessages.filter((message) => !existingIds.has(message.id));
  if (missingMessages.length === 0) {
    return merged;
  }

  const insertionIndex = merged.findIndex(
    (message) => message.metadata?.historyFinalForUserMessageId === userMessageId,
  );

  const normalizedMissing = missingMessages.map((message) => ({
    ...message,
    metadata: {
      ...(message.metadata || {}),
      hidden: !expanded,
      historyProcessPlaceholder: false,
      historyProcessLoaded: true,
      historyProcessUserMessageId: userMessageId,
      historyProcessExpanded: expanded,
    },
  }));

  if (insertionIndex < 0) {
    return [...merged, ...normalizedMissing];
  }

  return [
    ...merged.slice(0, insertionIndex),
    ...normalizedMissing,
    ...merged.slice(insertionIndex),
  ];
};

export const applyTurnProcessCache = (
  messages: Message[],
  processCache?: Record<string, Message[]>,
  processState?: Record<string, TurnProcessState>,
): Message[] => {
  if (!processCache && !processState) {
    return messages;
  }

  return messages.map((message) => {
    if (message.role === 'user') {
      const userMessageId = message.id;
      const state = processState?.[userMessageId];
      if (!state) {
        return message;
      }
      return withUserProcessMeta(message, {
        expanded: state.expanded,
        loading: state.loading,
        loaded: state.loaded,
      });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    if (finalForUserMessageId) {
      const turnState = processState?.[finalForUserMessageId];
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: turnState?.expanded === true,
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    if (!turnUserMessageId) {
      return message;
    }

    const turnState = processState?.[turnUserMessageId];
    const expanded = turnState?.expanded === true;
    const loaded = turnState?.loaded === true;
    const visible = expanded && loaded;
    const cached = processCache?.[turnUserMessageId]?.find((item) => item.id === message.id);
    if (!cached) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          hidden: !visible,
          historyProcessExpanded: expanded,
        },
      };
    }

    return {
      ...cached,
      metadata: {
        ...(cached.metadata || {}),
        hidden: !visible,
        historyProcessUserMessageId: turnUserMessageId,
        historyProcessLoaded: true,
        historyProcessPlaceholder: false,
        historyProcessExpanded: expanded,
      },
    };
  });
};

import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';
import {
  getConversationTurnId,
  isMeaningfulReasoning,
  isNonEmptyString,
  normalizeImConversationMessages,
  normalizeRawMessages,
  normalizeTurnId,
} from './messageNormalization';
import { readSessionImConversationId } from './sessionRuntime';

const parseMaybeJsonObject = (value: unknown): Record<string, any> => {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, any>;
  }
  if (typeof value !== 'string') {
    return {};
  }
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, any>
      : {};
  } catch {
    return {};
  }
};

const isTaskNoticeRawMessage = (message: any): boolean => {
  const messageMode = typeof message?.message_mode === 'string'
    ? message.message_mode.trim().toLowerCase()
    : typeof message?.messageMode === 'string'
      ? message.messageMode.trim().toLowerCase()
      : '';
  if (messageMode === 'task_notice') {
    return true;
  }

  const metadata = parseMaybeJsonObject(message?.metadata);
  const metadataType = typeof metadata?.type === 'string'
    ? metadata.type.trim().toLowerCase()
    : '';
  return metadataType === 'task_execution_notice';
};

const mergeAndSortMessages = (messages: Message[]): Message[] => {
  const deduped = new Map<string, Message>();
  messages.forEach((message) => {
    if (!message?.id) {
      return;
    }
    deduped.set(message.id, message);
  });

  return Array.from(deduped.values()).sort((left, right) => (
    new Date(left.createdAt || 0).getTime() - new Date(right.createdAt || 0).getTime()
  ));
};

const resolveUserProcessKey = (message: any): string => (
  getConversationTurnId(message)
  || normalizeTurnId(message?.metadata?.historyProcess?.turnId)
  || String(message?.id || '').trim()
);

const resolveFinalAssistantProcessKey = (message: any): string => {
  const finalUserId = typeof message?.metadata?.historyFinalForUserMessageId === 'string'
    ? message.metadata.historyFinalForUserMessageId.trim()
    : '';
  const finalTurnId = normalizeTurnId(message?.metadata?.historyFinalForTurnId);
  if (!finalUserId && !finalTurnId) {
    return '';
  }
  return finalTurnId || getConversationTurnId(message) || finalUserId;
};

const resolveProcessMessageKey = (message: any): string => (
  normalizeTurnId(message?.metadata?.historyProcessTurnId)
  || getConversationTurnId(message)
  || (typeof message?.metadata?.historyProcessUserMessageId === 'string'
    ? message.metadata.historyProcessUserMessageId.trim()
    : '')
);

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

const buildProcessMessageSignature = (message: Message): string => {
  const metadata = message.metadata || {};
  const raw = message as any;
  const segments = Array.isArray(metadata.contentSegments) ? metadata.contentSegments : [];
  const metadataToolCalls = Array.isArray(metadata.toolCalls) ? metadata.toolCalls : [];
  const topLevelToolCalls = Array.isArray(raw?.toolCalls) ? raw.toolCalls : [];
  const toolCallIds = [...metadataToolCalls, ...topLevelToolCalls]
    .map((toolCall: any) => {
      const id = typeof toolCall?.id === 'string' ? toolCall.id.trim() : '';
      const name = typeof toolCall?.name === 'string' ? toolCall.name.trim() : '';
      return id || name;
    })
    .filter(Boolean);
  const segmentSignature = segments.map((segment: any) => {
    const type = typeof segment?.type === 'string' ? segment.type.trim() : '';
    if (type === 'tool_call') {
      const toolCallId = typeof segment?.toolCallId === 'string' ? segment.toolCallId.trim() : '';
      return `tool:${toolCallId}`;
    }
    if (type === 'thinking') {
      const content = typeof segment?.content === 'string' ? segment.content.trim() : '';
      return `thinking:${content}`;
    }
    if (type === 'text') {
      const content = typeof segment?.content === 'string' ? segment.content.trim() : '';
      return `text:${content}`;
    }
    return '';
  }).filter(Boolean);
  const toolResultId = typeof raw?.tool_call_id === 'string'
    ? raw.tool_call_id.trim()
    : typeof raw?.toolCallId === 'string'
      ? raw.toolCallId.trim()
      : typeof metadata.tool_call_id === 'string'
        ? metadata.tool_call_id.trim()
        : typeof metadata.toolCallId === 'string'
          ? metadata.toolCallId.trim()
          : '';
  const content = typeof message.content === 'string' ? message.content.trim() : '';

  return JSON.stringify({
    role: message.role,
    status: String(message.status || ''),
    content,
    toolResultId,
    toolCallIds,
    segmentSignature,
  });
};

const dedupeProcessMessages = (messages: Message[]): Message[] => {
  if (!Array.isArray(messages) || messages.length <= 1) {
    return messages;
  }

  const seen = new Set<string>();
  const deduped: Message[] = [];

  messages.forEach((message) => {
    const signature = buildProcessMessageSignature(message);
    if (!signature || !seen.has(signature)) {
      if (signature) {
        seen.add(signature);
      }
      deduped.push(message);
    }
  });

  return deduped;
};

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
        if (message.role === 'user') {
          const process = message.metadata?.historyProcess;
          if (!process || typeof process !== 'object') {
            return message;
          }

          const turnId = getConversationTurnId(message);
          if (!turnId || process.turnId === turnId) {
            return message;
          }

          return {
            ...message,
            metadata: {
              ...(message.metadata || {}),
              historyProcess: {
                ...process,
                turnId,
              },
            },
          };
        }

        if (!message.metadata?.historyFinalForUserMessageId) {
          return message;
        }

        const finalTurnId = normalizeTurnId(message.metadata?.historyFinalForTurnId)
          || getConversationTurnId(message);
        return {
          ...message,
          metadata: {
            ...(message.metadata || {}),
            historyProcessExpanded: message.metadata?.historyProcessExpanded === true,
            ...(finalTurnId ? { historyFinalForTurnId: finalTurnId } : {}),
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
    const conversationTurnId = getConversationTurnId(userMessage);

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
            ...(conversationTurnId ? { historyProcessTurnId: conversationTurnId } : {}),
          },
        });
      }
    }

    const dedupedInlineProcessMessages = dedupeProcessMessages(inlineProcessMessages);
    const processMessageCount = dedupedInlineProcessMessages.length;

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
          ...(conversationTurnId ? { turnId: conversationTurnId } : {}),
          finalAssistantMessageId: finalAssistantIndex >= 0 ? messages[finalAssistantIndex].id : null,
        },
        ...(processMessageCount > 0 ? { historyProcessInlineMessages: dedupedInlineProcessMessages } : {}),
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
        ...(conversationTurnId ? { historyFinalForTurnId: conversationTurnId } : {}),
        historyProcessExpanded: false,
      },
    });
  });

  return result;
};

export const fetchSessionMessages = async (
  client: ApiClient,
  sessionId: string,
  options: {
    limit?: number;
    offset?: number;
    session?: { metadata?: unknown } | null;
  } = { limit: 50, offset: 0 },
): Promise<Message[]> => {
  const limit = options.limit ?? 50;
  const offset = options.offset ?? 0;
  const imConversationId = readSessionImConversationId(options.session?.metadata);

  if (imConversationId) {
    const fetchLimit = Math.max(1, limit + offset);
    const [rawImMessages, rawSessionMessages] = await Promise.all([
      client.getImConversationMessages(imConversationId, {
        limit: fetchLimit,
        order: 'desc',
      }),
      client.getSessionMessages(sessionId, {
        limit: fetchLimit,
        offset: 0,
        compact: false,
      }).catch(() => []),
    ]);
    const ascendingImMessages = normalizeImConversationMessages(rawImMessages, sessionId).reverse();
    const taskNoticeMessages = normalizeRawMessages(
      (Array.isArray(rawSessionMessages) ? rawSessionMessages : []).filter(isTaskNoticeRawMessage),
      sessionId,
    );
    const mergedMessages = mergeAndSortMessages([
      ...ascendingImMessages,
      ...taskNoticeMessages,
    ]);
    const olderCount = offset > 0
      ? Math.max(0, mergedMessages.length - offset)
      : mergedMessages.length;
    const pagedMessages = offset > 0
      ? mergedMessages.slice(0, olderCount)
      : mergedMessages;

    debugLog('[Store] Loaded IM session messages', {
      sessionId,
      imConversationId,
      requested: { limit, offset, fetchLimit },
      taskNoticeCount: taskNoticeMessages.length,
      received: pagedMessages.length,
    });
    return pagedMessages;
  }

  const rawMessages = await client.getSessionMessages(sessionId, {
    limit,
    offset,
    compact: true,
    strategy: 'v2',
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
  options: { turnId?: string } = {},
): Promise<Message[]> => {
  const turnId = typeof options.turnId === 'string' ? options.turnId.trim() : '';
  if (!userMessageId && !turnId) {
    return [];
  }

  let rawMessages: any[] = [];
  if (turnId) {
    rawMessages = await client.getSessionTurnProcessMessagesByTurn(sessionId, turnId);
    if (rawMessages.length === 0 && userMessageId) {
      rawMessages = await client.getSessionTurnProcessMessages(sessionId, userMessageId);
    }
  } else {
    rawMessages = await client.getSessionTurnProcessMessages(sessionId, userMessageId);
  }
  const normalized = dedupeProcessMessages(normalizeRawMessages(rawMessages, sessionId));

  return normalized.map((message) => ({
    ...message,
    metadata: {
      ...message.metadata,
      hidden: false,
      historyProcessPlaceholder: false,
      historyProcessLoaded: true,
      historyProcessUserMessageId: userMessageId,
      ...((turnId || getConversationTurnId(message))
        ? { historyProcessTurnId: turnId || getConversationTurnId(message) }
        : {}),
      historyProcessExpanded: true,
    },
  }));
};

export const resolveTurnProcessKeyForUserMessage = (
  messages: Message[],
  userMessageId: string,
): string => {
  if (!userMessageId) {
    return '';
  }

  const userMessage = (messages || []).find((message) => (
    message?.id === userMessageId && message?.role === 'user'
  ));
  if (!userMessage) {
    return userMessageId;
  }

  return resolveUserProcessKey(userMessage);
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
  options: { processKey?: string } = {},
): Message[] => {
  const processKey = normalizeTurnId(options.processKey) || userMessageId;
  const hasTurnProcessKey = Boolean(processKey && processKey !== userMessageId);

  return messages.map((message) => {
    if (message.id === userMessageId) {
      return withUserProcessMeta(message, { expanded });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    const finalProcessKey = resolveFinalAssistantProcessKey(message);
    const isFinalMatch = finalForUserMessageId === userMessageId
      || (Boolean(processKey) && finalProcessKey === processKey);
    if (isFinalMatch) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: expanded,
          historyFinalForUserMessageId: finalForUserMessageId || userMessageId,
          ...(hasTurnProcessKey ? { historyFinalForTurnId: processKey } : {}),
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    const turnProcessKey = resolveProcessMessageKey(message);
    const isProcessMatch = turnUserMessageId === userMessageId
      || (Boolean(processKey) && turnProcessKey === processKey);
    if (!isProcessMatch) {
      return message;
    }

    return {
      ...message,
      metadata: {
        ...(message.metadata || {}),
        hidden: !expanded,
        historyProcessUserMessageId: turnUserMessageId || userMessageId,
        ...(hasTurnProcessKey ? { historyProcessTurnId: processKey } : {}),
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
  options: { processKey?: string } = {},
): Message[] => {
  const processKey = normalizeTurnId(options.processKey) || userMessageId;
  const hasTurnProcessKey = Boolean(processKey && processKey !== userMessageId);
  const dedupedProcessMessages = dedupeProcessMessages(processMessages);

  const processById = new Map<string, Message>();
  dedupedProcessMessages.forEach((message) => {
    processById.set(message.id, message);
  });

  const merged = messages.map((message) => {
    if (message.id === userMessageId) {
      return withUserProcessMeta(
        {
          ...message,
          metadata: {
            ...(message.metadata || {}),
            historyProcess: {
              ...(message.metadata?.historyProcess || {}),
              userMessageId,
              ...(hasTurnProcessKey ? { turnId: processKey } : {}),
            },
          },
        },
        { expanded, loaded: true, loading: false },
      );
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    const finalProcessKey = resolveFinalAssistantProcessKey(message);
    const isFinalMatch = finalForUserMessageId === userMessageId
      || (Boolean(processKey) && finalProcessKey === processKey);
    if (isFinalMatch) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: expanded,
          historyFinalForUserMessageId: finalForUserMessageId || userMessageId,
          ...(hasTurnProcessKey ? { historyFinalForTurnId: processKey } : {}),
        },
      };
    }

    const turnUserMessageId = message.metadata?.historyProcessUserMessageId;
    const turnProcessKey = resolveProcessMessageKey(message);
    const isProcessMatch = turnUserMessageId === userMessageId
      || (Boolean(processKey) && turnProcessKey === processKey);
    if (!isProcessMatch) {
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
        historyProcessUserMessageId: turnUserMessageId || userMessageId,
        ...(hasTurnProcessKey ? { historyProcessTurnId: processKey } : {}),
        historyProcessExpanded: expanded,
      },
    };
  });

  const existingIds = new Set(merged.map((message) => message.id));
  const missingMessages = dedupedProcessMessages.filter((message) => !existingIds.has(message.id));
  if (missingMessages.length === 0) {
    return merged;
  }

  const insertionIndex = merged.findIndex(
    (message) => (
      message.metadata?.historyFinalForUserMessageId === userMessageId
      || resolveFinalAssistantProcessKey(message) === processKey
    ),
  );

  const normalizedMissing = missingMessages.map((message) => ({
    ...message,
    metadata: {
      ...(message.metadata || {}),
      hidden: !expanded,
      historyProcessPlaceholder: false,
      historyProcessLoaded: true,
      historyProcessUserMessageId: userMessageId,
      ...(hasTurnProcessKey ? { historyProcessTurnId: processKey } : {}),
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

  const resolveState = (processKey: string, fallbackUserMessageId: string): TurnProcessState | undefined => {
    if (!processState) {
      return undefined;
    }
    if (processKey && processState[processKey]) {
      return processState[processKey];
    }
    if (fallbackUserMessageId && processState[fallbackUserMessageId]) {
      return processState[fallbackUserMessageId];
    }
    return undefined;
  };

  const resolveCache = (processKey: string, fallbackUserMessageId: string): Message[] | undefined => {
    if (!processCache) {
      return undefined;
    }
    if (processKey && processCache[processKey]) {
      return processCache[processKey];
    }
    if (fallbackUserMessageId && processCache[fallbackUserMessageId]) {
      return processCache[fallbackUserMessageId];
    }
    return undefined;
  };

  return messages.map((message) => {
    if (message.role === 'user') {
      const userMessageId = message.id;
      const processKey = resolveUserProcessKey(message);
      const turnId = getConversationTurnId(message);
      const state = resolveState(processKey, userMessageId);
      if (!state) {
        return message;
      }
      const withTurnId = {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcess: {
            ...(message.metadata?.historyProcess || {}),
            userMessageId,
            ...(turnId ? { turnId } : {}),
          },
        },
      };
      return withUserProcessMeta(withTurnId, {
        expanded: state.expanded,
        loading: state.loading,
        loaded: state.loaded,
      });
    }

    const finalForUserMessageId = message.metadata?.historyFinalForUserMessageId;
    const finalProcessKey = resolveFinalAssistantProcessKey(message);
    const explicitFinalTurnId = normalizeTurnId(message.metadata?.historyFinalForTurnId)
      || getConversationTurnId(message);
    if (finalForUserMessageId || finalProcessKey) {
      const turnState = resolveState(finalProcessKey, finalForUserMessageId || '');
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          historyProcessExpanded: turnState?.expanded === true,
          ...(explicitFinalTurnId ? { historyFinalForTurnId: explicitFinalTurnId } : {}),
        },
      };
    }

    const turnUserMessageId = typeof message.metadata?.historyProcessUserMessageId === 'string'
      ? message.metadata.historyProcessUserMessageId
      : '';
    const turnProcessKey = resolveProcessMessageKey(message);
    const explicitProcessTurnId = normalizeTurnId(message.metadata?.historyProcessTurnId)
      || getConversationTurnId(message);
    if (!turnUserMessageId && !turnProcessKey) {
      return message;
    }

    const turnState = resolveState(turnProcessKey, turnUserMessageId);
    const expanded = turnState?.expanded === true;
    const loaded = turnState?.loaded === true;
    const visible = expanded && loaded;
    const cachedItems = resolveCache(turnProcessKey, turnUserMessageId) || [];
    const cached = cachedItems.find((item) => item.id === message.id);
    if (!cached) {
      return {
        ...message,
        metadata: {
          ...(message.metadata || {}),
          hidden: !visible,
          ...(turnUserMessageId ? { historyProcessUserMessageId: turnUserMessageId } : {}),
          ...(explicitProcessTurnId ? { historyProcessTurnId: explicitProcessTurnId } : {}),
          historyProcessExpanded: expanded,
        },
      };
    }

    return {
      ...cached,
      metadata: {
        ...(cached.metadata || {}),
        hidden: !visible,
        ...(turnUserMessageId ? { historyProcessUserMessageId: turnUserMessageId } : {}),
        ...(explicitProcessTurnId ? { historyProcessTurnId: explicitProcessTurnId } : {}),
        historyProcessLoaded: true,
        historyProcessPlaceholder: false,
        historyProcessExpanded: expanded,
      },
    };
  });
};

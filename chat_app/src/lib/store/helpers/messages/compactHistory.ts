import type { Message } from '../../../../types';
import type ApiClient from '../../../api/client';
import { debugLog } from '@/lib/utils';
import {
  getConversationTurnId,
  isMeaningfulReasoning,
  isNonEmptyString,
  normalizeRawMessages,
  normalizeTurnId,
} from '../messageNormalization';
import { createDefaultHistoryProcessState } from '../../actions/sendMessage/types';

const countThinkingSegments = (message: Message): number => {
  const segments = message.metadata?.contentSegments;
  if (!Array.isArray(segments) || segments.length === 0) {
    return 0;
  }

  return segments.filter((segment) => (
    segment?.type === 'thinking'
      && typeof segment.content === 'string'
      && isMeaningfulReasoning(segment.content)
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

    const processMessageCount = inlineProcessMessages.length;

    result.push({
      ...userMessage,
      metadata: {
        ...(userMessage.metadata || {}),
        historyProcess: {
          ...createDefaultHistoryProcessState({
            userMessageId,
            turnId: conversationTurnId,
            finalAssistantMessageId: finalAssistantIndex >= 0 ? messages[finalAssistantIndex].id : null,
          }),
          hasProcess: processMessageCount > 0,
          toolCallCount,
          thinkingCount,
          processMessageCount,
        },
        ...(processMessageCount > 0 ? { historyProcessInlineMessages: inlineProcessMessages } : {}),
      },
    });

    if (finalAssistantIndex < 0) {
      return;
    }

    const finalAssistant = messages[finalAssistantIndex];
    const textSegments = Array.isArray(finalAssistant.metadata?.contentSegments)
      ? finalAssistant.metadata?.contentSegments.filter((segment) => segment?.type === 'text')
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
  options: { limit?: number; offset?: number } = { limit: 50, offset: 0 },
): Promise<Message[]> => {
  const limit = options.limit ?? 50;
  const offset = options.offset ?? 0;

  const rawMessages = await client.getConversationMessages(sessionId, {
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

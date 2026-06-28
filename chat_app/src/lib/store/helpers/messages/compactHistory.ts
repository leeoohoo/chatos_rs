import type { Message } from '../../../../types';
import type ApiClient from '../../../api/client';
import { debugLogLazy } from '@/lib/utils';
import {
  getConversationTurnId,
  isTaskRunnerCallbackMessage,
  isMeaningfulReasoning,
  isNonEmptyString,
  normalizeRawMessages,
  normalizeTurnId,
} from '../messageNormalization';
import { createDefaultHistoryProcessState } from '../../actions/sendMessage/types';

export type CompactHistoryPageResult = {
  messages: Message[];
  hasMore: boolean;
  nextBefore: string | null;
};

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

const stripTaskRunnerCallbackTurnLinkage = (message: Message): Message => {
  if (!isTaskRunnerCallbackMessage(message)) {
    return message;
  }

  const sourceTurnId = normalizeTurnId(
    message.metadata?.task_runner_async?.source_turn_id
      || message.metadata?.conversation_turn_id
      || message.metadata?.conversationTurnId
      || message.metadata?.historyFinalForTurnId
      || message.metadata?.historyProcessTurnId
      || message.metadata?.historyProcess?.turnId,
  );
  const metadata = { ...(message.metadata || {}) };
  delete metadata.conversation_turn_id;
  delete metadata.conversationTurnId;
  delete metadata.historyFinalForUserMessageId;
  delete metadata.historyFinalForTurnId;
  delete metadata.historyProcessUserMessageId;
  delete metadata.historyProcessTurnId;
  delete metadata.historyProcessPlaceholder;
  if (sourceTurnId) {
    metadata.task_runner_async = {
      ...(metadata.task_runner_async || {}),
      source_turn_id: sourceTurnId,
    };
  }

  return {
    ...message,
    metadata,
  };
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
        if (isTaskRunnerCallbackMessage(message)) {
          return stripTaskRunnerCallbackTurnLinkage(message);
        }

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
    const callbackUpdates: Message[] = [];

    let finalAssistantIndex = -1;
    for (let i = nextUserIndex - 1; i > userIndex; i -= 1) {
      const candidate = messages[i];
      if (
        candidate.role !== 'assistant'
        || isSessionSummaryMessage(candidate)
        || isTaskRunnerCallbackMessage(candidate)
      ) {
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
      if (isTaskRunnerCallbackMessage(message)) {
        callbackUpdates.push(stripTaskRunnerCallbackTurnLinkage(message));
        continue;
      }

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
      },
    });

    if (finalAssistantIndex < 0) {
      result.push(...callbackUpdates);
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
      },
    });

    result.push(...callbackUpdates);
  });

  return result;
};

export const fetchSessionMessages = async (
  client: ApiClient,
  sessionId: string,
  options: { limit?: number; before?: string | null } = { limit: 50, before: null },
): Promise<CompactHistoryPageResult> => {
  const limit = options.limit ?? 50;

  const response = await client.getConversationCompactHistory(sessionId, {
    limit,
    before: options.before ?? null,
  });

  const rawMessages = Array.isArray(response?.items) ? response.items : [];
  const messages = ensureCompactHistoryShape(normalizeRawMessages(rawMessages, sessionId));
  const hasMore = response?.has_more === true;
  const nextBefore = typeof response?.next_before === 'string' && response.next_before.trim().length > 0
    ? response.next_before.trim()
    : null;
  debugLogLazy(() => ['[Store] Loaded compact session messages', {
    sessionId,
    requested: { limit, before: options.before ?? null },
    received: rawMessages.length,
    returned: messages.length,
    hasMore,
    nextBefore,
  }]);
  return {
    messages,
    hasMore,
    nextBefore,
  };
};

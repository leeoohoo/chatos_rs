// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../types';
import { isTaskRunnerCallbackMessage } from '../../lib/domain/messages';
import {
  getMessageAllToolCalls,
  getMessageConversationTurnId,
  getMessageHistoryFinalForTurnId,
  getMessageHistoryFinalForUserMessageId,
  getMessageHistoryProcessTurnId,
  getMessageHistoryProcessUserMessageId,
  getMessageMetadataRecord,
  getMessageToolResultCallId,
  normalizeMetaId,
  normalizeTurnId,
  type MessageToolCallLike,
} from '../messageItem/messageReaders';

type ParsedMessageForList = {
  message: Message;
  id: string;
  role: Message['role'];
  status: string;
  visible: boolean;
  time: number;
  assistantToolCalls: Array<{ id: string; toolCall: MessageToolCallLike }>;
  toolResultCallId: string;
  conversationTurnId: string;
  historyProcessTurnId: string;
  historyProcessUserMessageId: string;
  historyFinalForUserMessageId: string;
  historyFinalForTurnId: string;
  isTaskRunnerCallbackAssistant: boolean;
};

export type ParsedMessageCacheEntry = {
  ref: Message;
  metadataRef: unknown;
  content: string;
  status: unknown;
  updatedAt: unknown;
  parsed: ParsedMessageForList;
};

const getTimeValue = (value: unknown): number => {
  if (!value) return 0;
  if (value instanceof Date) return value.getTime();
  if (typeof value !== 'string' && typeof value !== 'number') return 0;
  const parsed = new Date(value).getTime();
  return Number.isNaN(parsed) ? 0 : parsed;
};

const readMessageContentLength = (message: Message): number => (
  typeof message?.content === 'string' ? message.content.trim().length : 0
);

const readNonProcessAssistantDedupKey = (parsed: ParsedMessageForList): string => {
  if (parsed.role !== 'assistant') {
    return '';
  }
  if (parsed.isTaskRunnerCallbackAssistant) {
    return '';
  }
  if (parsed.historyProcessUserMessageId || parsed.historyProcessTurnId) {
    return '';
  }

  const turnId = normalizeTurnId(parsed.historyFinalForTurnId || parsed.conversationTurnId);
  if (turnId) {
    return `turn:${turnId}`;
  }

  const userId = normalizeMetaId(parsed.historyFinalForUserMessageId);
  if (userId) {
    return `user:${userId}`;
  }

  return '';
};

export const parseMessageForList = (message: Message): ParsedMessageForList => {
  const metadataRecord = getMessageMetadataRecord(message);
  const allToolCalls = getMessageAllToolCalls(message);
  const assistantToolCalls: Array<{ id: string; toolCall: MessageToolCallLike }> = [];

  if (message.role === 'assistant') {
    allToolCalls.forEach((toolCall) => {
      const id = normalizeMetaId(toolCall.id);
      if (id) {
        assistantToolCalls.push({ id, toolCall });
      }
    });
  }

  const conversationTurnId = getMessageConversationTurnId(message);
  const historyProcessTurnId = getMessageHistoryProcessTurnId(message);
  const historyProcessUserMessageId = getMessageHistoryProcessUserMessageId(message);
  const historyFinalForUserMessageId = getMessageHistoryFinalForUserMessageId(message);
  const historyFinalForTurnId = getMessageHistoryFinalForTurnId(message);
  const isTaskRunnerCallbackAssistant = Boolean(
    message.role === 'assistant' && isTaskRunnerCallbackMessage(message),
  );

  return {
    message,
    id: message.id,
    role: message.role,
    status: String(message.status || ''),
    visible: metadataRecord?.hidden !== true && message.role !== 'tool',
    time: message.updatedAt ? getTimeValue(message.updatedAt) : getTimeValue(message.createdAt),
    assistantToolCalls,
    toolResultCallId: getMessageToolResultCallId(message),
    conversationTurnId,
    historyProcessTurnId,
    historyProcessUserMessageId,
    historyFinalForUserMessageId,
    historyFinalForTurnId,
    isTaskRunnerCallbackAssistant,
  };
};

export const buildVisibleMessageState = (parsedMessages: ParsedMessageForList[]) => {
  const visibleCandidates: ParsedMessageForList[] = [];
  const toolResultMap = new Map<string, Message>();
  const toolResultMetaMap = new Map<string, { id: string; time: number }>();
  const assistantToolById = new Map<string, MessageToolCallLike>();
  const assistantToolMetaById = new Map<string, { messageId: string; time: number }>();

  parsedMessages.forEach((parsed) => {
    const isInlineProcessMessage = parsed.role !== 'user' && Boolean(
      parsed.historyProcessUserMessageId
      || parsed.historyProcessTurnId,
    );

    if (parsed.visible && !isInlineProcessMessage) {
      visibleCandidates.push(parsed);
    }

    if (parsed.toolResultCallId) {
      toolResultMap.set(parsed.toolResultCallId, parsed.message);
      toolResultMetaMap.set(parsed.toolResultCallId, { id: parsed.id, time: parsed.time });
    }

    if (parsed.role === 'assistant') {
      parsed.assistantToolCalls.forEach(({ id, toolCall }) => {
        if (!assistantToolById.has(id)) {
          assistantToolById.set(id, toolCall);
        }
        if (!assistantToolMetaById.has(id)) {
          assistantToolMetaById.set(id, { messageId: parsed.id, time: parsed.time });
        }
      });
    }

  });

  const visible = (() => {
    if (visibleCandidates.length <= 1) {
      return visibleCandidates.map((item) => item.message);
    }

    const bestFinalAssistantByKey = new Map<string, ParsedMessageForList>();

    visibleCandidates.forEach((parsed) => {
      const dedupKey = readNonProcessAssistantDedupKey(parsed);
      if (!dedupKey) {
        return;
      }

      const existing = bestFinalAssistantByKey.get(dedupKey);
      if (!existing) {
        bestFinalAssistantByKey.set(dedupKey, parsed);
        return;
      }

      const parsedStatus = String(parsed.status || '');
      const existingStatus = String(existing.status || '');
      const parsedIsTerminal = parsedStatus === 'completed' || parsedStatus === 'error';
      const existingIsTerminal = existingStatus === 'completed' || existingStatus === 'error';
      const parsedContentLength = readMessageContentLength(parsed.message);
      const existingContentLength = readMessageContentLength(existing.message);

      const shouldReplace = (
        Number(parsedIsTerminal) > Number(existingIsTerminal)
        || (
          parsedIsTerminal === existingIsTerminal
          && (
            parsedContentLength > existingContentLength
            || (parsedContentLength === existingContentLength && parsed.time >= existing.time)
          )
        )
      );

      if (shouldReplace) {
        bestFinalAssistantByKey.set(dedupKey, parsed);
      }
    });

    return visibleCandidates.filter((parsed) => {
      const dedupKey = readNonProcessAssistantDedupKey(parsed);
      if (!dedupKey) {
        return true;
      }
      return bestFinalAssistantByKey.get(dedupKey)?.id === parsed.id;
    }).map((parsed) => parsed.message);
  })();

  return {
    visibleMessages: visible,
    toolResultById: toolResultMap,
    toolResultMetaById: toolResultMetaMap,
    assistantToolCallById: assistantToolById,
    assistantToolCallMetaById: assistantToolMetaById,
  };
};

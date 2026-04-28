import type { Message } from '../../../../types';
import {
  getConversationTurnId,
  normalizeTurnId,
} from '../messageNormalization';

export const resolveUserProcessKey = (message: Message): string => (
  getConversationTurnId(message)
  || normalizeTurnId(message?.metadata?.historyProcess?.turnId)
  || String(message?.id || '').trim()
);

export const resolveFinalAssistantProcessKey = (message: Message): string => {
  const finalUserId = typeof message?.metadata?.historyFinalForUserMessageId === 'string'
    ? message.metadata.historyFinalForUserMessageId.trim()
    : '';
  const finalTurnId = normalizeTurnId(message?.metadata?.historyFinalForTurnId);
  if (!finalUserId && !finalTurnId) {
    return '';
  }
  return finalTurnId || getConversationTurnId(message) || finalUserId;
};

export const resolveProcessMessageKey = (message: Message): string => (
  normalizeTurnId(message?.metadata?.historyProcessTurnId)
  || getConversationTurnId(message)
  || (typeof message?.metadata?.historyProcessUserMessageId === 'string'
    ? message.metadata.historyProcessUserMessageId.trim()
    : '')
);

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

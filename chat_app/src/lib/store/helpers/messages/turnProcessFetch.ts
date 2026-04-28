import type { Message } from '../../../../types';
import type ApiClient from '../../../api/client';
import type { SessionMessageResponse } from '../../../api/client/types';
import {
  getConversationTurnId,
  normalizeRawMessages,
} from '../messageNormalization';

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

  let rawMessages: SessionMessageResponse[] = [];
  if (turnId) {
    rawMessages = await client.getConversationTurnProcessMessagesByTurn(sessionId, turnId);
    if (rawMessages.length === 0 && userMessageId) {
      rawMessages = await client.getConversationTurnProcessMessages(sessionId, userMessageId);
    }
  } else {
    rawMessages = await client.getConversationTurnProcessMessages(sessionId, userMessageId);
  }
  const normalized = normalizeRawMessages(rawMessages, sessionId);

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

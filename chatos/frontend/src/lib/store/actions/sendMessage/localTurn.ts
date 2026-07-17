// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../../../types';
import type { StreamChatCommandResponse } from '../../../api/client/types';
import type { ChatStoreSet } from '../../types';
import {
  extractCompactHistoryMessages,
  writeSessionMessagesCache,
} from '../sessionsUtils';
import { normalizePersistedMessage } from './persistedTurnMessages';
import { createDefaultSessionChatState } from './sessionState';

interface ApplyLocalTurnResponseParams {
  set: ChatStoreSet;
  sessionId: string;
  optimisticUserMessageId: string;
  response: StreamChatCommandResponse;
}

export const applyLocalTurnResponse = ({
  set,
  sessionId,
  optimisticUserMessageId,
  response,
}: ApplyLocalTurnResponseParams): void => {
  const userMessage = normalizePersistedMessage(response.user_message, sessionId);
  const assistantMessage = normalizePersistedMessage(response.assistant_message, sessionId);
  if (!userMessage || !assistantMessage) {
    throw new Error('本地运行时返回的消息不完整');
  }
  const processMessages = (Array.isArray(response.process_messages)
    ? response.process_messages
    : [])
    .map((message) => normalizePersistedMessage(message, sessionId))
    .filter((message): message is Message => message !== null);
  const persistedMessages = [userMessage, ...processMessages, assistantMessage];

  set((state) => {
    const replacedIds = new Set([
      optimisticUserMessageId,
      ...persistedMessages.map((message) => message.id),
    ]);
    const nextMessages = [
      ...state.messages.filter((message) => !replacedIds.has(message.id)),
      ...persistedMessages,
    ].sort(compareMessages);
    state.messages = nextMessages;

    const previous = state.sessionChatState[sessionId] || createDefaultSessionChatState();
    state.sessionChatState[sessionId] = {
      ...previous,
      isLoading: false,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
      streamingMessageId: null,
      activeTurnId: null,
      streamingPreviewText: '',
      streamingTransport: 'local',
    };
    if (state.currentSessionId === sessionId) {
      state.isLoading = false;
      state.isStreaming = false;
      state.streamingMessageId = null;
      state.error = null;
    }

    writeSessionMessagesCache(state, sessionId, {
      messages: extractCompactHistoryMessages(nextMessages),
      nextBefore: state.sessionMessagePaginationState?.[sessionId]?.nextBefore ?? null,
      loaded: true,
    });
  });
};

const compareMessages = (left: Message, right: Message): number => {
  const leftSequence = Number(left.metadata?.local_sequence_no);
  const rightSequence = Number(right.metadata?.local_sequence_no);
  if (
    Number.isFinite(leftSequence)
    && Number.isFinite(rightSequence)
    && leftSequence !== rightSequence
  ) {
    return leftSequence - rightSequence;
  }
  const leftTime = left.createdAt instanceof Date ? left.createdAt.getTime() : 0;
  const rightTime = right.createdAt instanceof Date ? right.createdAt.getTime() : 0;
  if (Number.isFinite(leftTime) && Number.isFinite(rightTime) && leftTime !== rightTime) {
    return leftTime - rightTime;
  }
  return left.id.localeCompare(right.id);
};

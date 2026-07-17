// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ApiRequestError } from '../../../api/client/shared';
import type ApiClient from '../../../api/client';
import { normalizeRawMessages } from '../../../domain/messages';
import type { ChatStoreSet } from '../../types';
import {
  extractCompactHistoryMessages,
  writeSessionMessagesCache,
} from '../sessionsUtils';
import { createDefaultSessionChatState } from './sessionState';

export const isLocalTurnCancellationError = (error: unknown): boolean => (
  error instanceof ApiRequestError && error.code === 'local_runtime_turn_cancelled'
);

export const applyCancelledLocalTurn = async ({
  client,
  set,
  sessionId,
}: {
  client: ApiClient;
  set: ChatStoreSet;
  sessionId: string;
}): Promise<void> => {
  const response = await client.getMessages(sessionId, { limit: 500, offset: 0 });
  const persisted = Array.isArray(response?.data?.messages)
    ? normalizeRawMessages(response.data.messages, sessionId)
    : [];
  set((state) => {
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
      state.messages = persisted;
      state.isLoading = false;
      state.isStreaming = false;
      state.streamingMessageId = null;
      state.error = null;
    }
    writeSessionMessagesCache(state, sessionId, {
      messages: extractCompactHistoryMessages(persisted),
      nextBefore: null,
      loaded: true,
    });
  });
};

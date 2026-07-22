// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type ApiClient from '../../../api/client';
import type { TurnRuntimeSnapshotLookupResponse } from '../../../api/client/types';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../../types';
import { createDefaultSessionChatState } from '../sendMessage/sessionState';

const normalizeText = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const applyRuntimeSnapshotRecovery = (
  state: ChatStoreDraft,
  sessionId: string,
  payload: TurnRuntimeSnapshotLookupResponse,
): void => {
  const turnId = normalizeText(payload.turn_id || payload.snapshot?.turn_id);
  const status = normalizeText(payload.status || payload.snapshot?.status).toLowerCase();
  const activeInRuntime = payload.active_in_runtime === true && status === 'running' && Boolean(turnId);
  const previous = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
  const currentActiveTurnId = normalizeText(previous.activeTurnId);

  if (activeInRuntime) {
    if (currentActiveTurnId && currentActiveTurnId !== turnId) {
      return;
    }
    state.sessionChatState[sessionId] = {
      ...previous,
      isLoading: true,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
      streamingMessageId: null,
      activeTurnId: turnId,
      streamingPreviewText: '',
      streamingTransport: 'realtime',
    };
    if (state.currentSessionId === sessionId) {
      state.isLoading = true;
      state.isStreaming = false;
      state.streamingMessageId = null;
    }
    return;
  }

  if (!currentActiveTurnId || !turnId || currentActiveTurnId !== turnId) {
    return;
  }
  state.sessionChatState[sessionId] = {
    ...previous,
    isLoading: false,
    isStreaming: false,
    isStopping: false,
    streamingPhase: null,
    streamingMessageId: null,
    activeTurnId: null,
    streamingPreviewText: '',
    streamingTransport: null,
  };
  if (state.currentSessionId === sessionId) {
    state.isLoading = false;
    state.isStreaming = false;
    state.streamingMessageId = null;
  }
};

export const restoreSessionRuntimeState = async ({
  client,
  set,
  get,
  sessionId,
}: {
  client: ApiClient;
  set: ChatStoreSet;
  get: ChatStoreGet;
  sessionId: string;
}): Promise<void> => {
  if (typeof client.getConversationLatestTurnRuntimeContext !== 'function') {
    return;
  }
  try {
    const payload = await client.getConversationLatestTurnRuntimeContext(sessionId);
    set((state) => {
      if (!get().sessions.some((session) => session.id === sessionId)) {
        return;
      }
      applyRuntimeSnapshotRecovery(state, sessionId, payload);
    });
  } catch {
    // Runtime recovery is best-effort; compact history remains usable without it.
  }
};

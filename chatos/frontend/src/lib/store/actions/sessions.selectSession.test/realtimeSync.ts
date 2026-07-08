// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';

import { createSelectSessionActions } from '../sessions/selectSession';
import {
  createMessage,
  createSession,
  fetchSession,
  fetchSessionMessages,
  installBackgroundSyncSpy,
  setRealtimeConnectionStateSnapshot,
  writeSessionMessagesCache,
  type ChatStoreDraft,
  type ChatStoreShape,
} from './testUtils';

describe('selectSession', () => {
  it('skips background compact-history sync when realtime is connected and cache is fresh', async () => {
    setRealtimeConnectionStateSnapshot('connected');
    const now = Date.now();
    const state = {
      sessions: [createSession('session_1')],
      currentSessionId: null,
      currentSession: null,
      activePanel: 'chat',
      messages: [],
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      hasMoreMessages: true,
      error: null,
      selectedModelId: null,
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionChatState: {},
      sessionMessagePaginationState: {},
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [createMessage('session_1', 'msg_1', 'cached 1', {
        conversation_turn_id: 'turn_1',
      })],
      hasMore: true,
      nextBefore: 'turn_1',
    });

    writeSessionMessagesCache(state, 'session_1', {
      messages: [createMessage('session_1', 'msg_1', 'cached 1', {
        conversation_turn_id: 'turn_1',
      })],
      nextBefore: 'turn_1',
      loaded: true,
    });
    const cacheEntry = state.sessionMessagesCache.session_1;
    if (cacheEntry) {
      cacheEntry.fetchedAt = now;
    }
    const session = createSession('session_1');
    session.updatedAt = new Date(now - 1000);
    session.createdAt = new Date(now - 1000);
    state.sessions = [session];

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_1');

    expect(fetchSessionMessages).toHaveBeenCalledTimes(0);
    expect(state.messages.map((message) => message.id)).toEqual(['msg_1']);
  });

  it('keeps background compact-history sync when realtime is disconnected', async () => {
    setRealtimeConnectionStateSnapshot('disconnected');
    const state = {
      sessions: [createSession('session_1')],
      currentSessionId: null,
      currentSession: null,
      activePanel: 'chat',
      messages: [],
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      hasMoreMessages: true,
      error: null,
      selectedModelId: null,
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionChatState: {},
      sessionMessagePaginationState: {},
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [createMessage('session_1', 'msg_1', 'cached 1', {
        conversation_turn_id: 'turn_1',
      })],
      hasMore: true,
      nextBefore: 'turn_1',
    });

    writeSessionMessagesCache(state, 'session_1', {
      messages: [createMessage('session_1', 'msg_1', 'cached 1', {
        conversation_turn_id: 'turn_1',
      })],
      nextBefore: 'turn_1',
      loaded: true,
    });

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_1');

    expect(fetchSessionMessages).toHaveBeenCalledTimes(1);
  });

});

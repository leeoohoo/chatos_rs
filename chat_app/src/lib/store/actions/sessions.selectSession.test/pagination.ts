import { describe, expect, it, vi } from 'vitest';

import { createSelectSessionActions } from '../sessions/selectSession';
import {
  createMessage,
  createSession,
  fetchSession,
  fetchSessionMessages,
  installBackgroundSyncSpy,
  readSessionMessagesCache,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  setRealtimeConnectionStateSnapshot,
  writeSessionMessagesCache,
  type ChatStoreDraft,
  type ChatStoreShape,
  type FetchSessionMessagesResult,
  type Message,
} from './testUtils';

describe('selectSession', () => {
  it('restores only the most recent cached compact-history page when revisiting a session', async () => {
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
    const backgroundSync = installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    const cachedMessages: Message[] = [];
    for (let index = 1; index <= SESSION_MESSAGES_INITIAL_PAGE_SIZE + 5; index += 1) {
      cachedMessages.push({
        id: `user_${index}`,
        sessionId: 'session_1',
        role: 'user',
        content: `user_${index}`,
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {
          conversation_turn_id: `turn_${index}`,
        },
      });
      cachedMessages.push({
        id: `assistant_${index}`,
        sessionId: 'session_1',
        role: 'assistant',
        content: `assistant_${index}`,
        status: 'completed',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        metadata: {
          conversation_turn_id: `turn_${index}`,
          historyFinalForUserMessageId: `user_${index}`,
          historyFinalForTurnId: `turn_${index}`,
        },
      });
    }

    writeSessionMessagesCache(state, 'session_1', {
      messages: cachedMessages,
      nextBefore: 'turn_1',
      loaded: true,
    });

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => createSession(sessionId));
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: cachedMessages.slice(-2),
      hasMore: true,
      nextBefore: `turn_${SESSION_MESSAGES_INITIAL_PAGE_SIZE + 4}`,
    });

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_1');
    await backgroundSync.mock.results[0]?.value;

    expect(state.messages[0]?.id).toBe('user_6');
    expect(state.messages).toHaveLength(SESSION_MESSAGES_INITIAL_PAGE_SIZE * 2);
    expect(state.hasMoreMessages).toBe(true);
    expect(state.sessionMessagePaginationState.session_1).toEqual({
      nextBefore: 'turn_6',
      loaded: true,
    });
  });

  it('touches cached session history so an active cache hit becomes most recently used', async () => {
    const state = {
      sessions: [createSession('session_1'), createSession('session_2'), createSession('session_3')],
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
    const backgroundSync = installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    writeSessionMessagesCache(state, 'session_1', {
      messages: [createMessage('session_1', 'msg_1_cached', 'cached 1', {
        conversation_turn_id: 'turn_1',
      })],
      nextBefore: 'turn_1',
      loaded: true,
    });
    writeSessionMessagesCache(state, 'session_2', {
      messages: [createMessage('session_2', 'msg_2_cached', 'cached 2', {
        conversation_turn_id: 'turn_2',
      })],
      nextBefore: 'turn_2',
      loaded: true,
    });
    writeSessionMessagesCache(state, 'session_3', {
      messages: [createMessage('session_3', 'msg_3_cached', 'cached 3', {
        conversation_turn_id: 'turn_3',
      })],
      nextBefore: 'turn_3',
      loaded: true,
    });

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));

    let resolveMessages!: (result: FetchSessionMessagesResult) => void;
    vi.mocked(fetchSessionMessages).mockImplementation(() => (
      new Promise((resolve) => {
        resolveMessages = resolve;
      })
    ));

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    const selectionPromise = actions.selectSession('session_1');

    expect(state.sessionMessagesCacheOrder[0]).toBe('session_1');
    expect(state.messages.map((message) => message.id)).toEqual(['msg_1_cached']);

    resolveMessages({
      messages: [createMessage('session_1', 'msg_1_cached', 'cached 1', {
        conversation_turn_id: 'turn_1',
      })],
      hasMore: true,
      nextBefore: 'turn_1',
    });
    await selectionPromise;
    await backgroundSync.mock.results[0]?.value;

    expect(state.sessionMessagesCacheOrder[0]).toBe('session_1');
    expect(readSessionMessagesCache(state, 'session_1')).toMatchObject({
      nextBefore: 'turn_1',
      loaded: true,
    });
  });

  it('still backgrounds compact-history sync when cached snapshot is older than the session update', async () => {
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
      cacheEntry.fetchedAt = now - 60_000;
    }
    const session = createSession('session_1');
    session.updatedAt = new Date(now);
    session.createdAt = new Date(now);
    state.sessions = [session];

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_1');

    expect(fetchSessionMessages).toHaveBeenCalledTimes(1);
  });

  it('uses the requested initial page size and can skip background sync', async () => {
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
    const backgroundSync = installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [createMessage('session_1', 'msg_1', 'latest page', {
        conversation_turn_id: 'turn_1',
      })],
      hasMore: true,
      nextBefore: 'turn_1',
    });

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_1', {
      initialPageSize: 1,
      skipBackgroundSync: true,
    });

    expect(vi.mocked(fetchSessionMessages).mock.calls[0]).toEqual([
      {} as never,
      'session_1',
      { limit: 1, before: null },
    ]);
    expect(backgroundSync).not.toHaveBeenCalled();
  });
});

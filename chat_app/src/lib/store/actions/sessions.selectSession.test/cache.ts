import { describe, expect, it, vi } from 'vitest';

import { createSelectSessionActions } from '../sessions/selectSession';
import {
  createMessage,
  createSession,
  fetchSession,
  fetchSessionMessages,
  installBackgroundSyncSpy,
  readSessionMessagesCache,
  writeSessionMessagesCache,
  type ChatStoreDraft,
  type ChatStoreShape,
} from './testUtils';

describe('selectSession', () => {
  it('writes the current visible session snapshot back to cache before switching away', async () => {
    const state = {
      sessions: [createSession('session_1'), createSession('session_2')],
      currentSessionId: 'session_1',
      currentSession: createSession('session_1'),
      activePanel: 'chat',
      messages: [createMessage('session_1', 'msg_latest', 'latest visible message')],
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      hasMoreMessages: false,
      error: null,
      selectedModelId: null,
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionChatState: {
        session_1: {
          isLoading: false,
          isStreaming: false,
          isStopping: false,
          streamingMessageId: null,
          activeTurnId: null,
          streamingPreviewText: '',
          streamingTransport: null,
          runtimeContextRefreshNonce: 0,
        },
      },
      sessionMessagePaginationState: {
        session_1: {
          nextBefore: null,
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    installBackgroundSyncSpy(state);

    writeSessionMessagesCache(state, 'session_1', {
      messages: [createMessage('session_1', 'msg_stale', 'stale cached message')],
      nextBefore: null,
      loaded: true,
    });

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
    vi.mocked(fetchSessionMessages).mockImplementation(async (_client, sessionId) => ({
      messages: [createMessage(sessionId, `msg_${sessionId}`, `network ${sessionId}`)],
      hasMore: false,
      nextBefore: null,
    }));

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_2', { skipBackgroundSync: true });

    expect(readSessionMessagesCache(state, 'session_1')?.messages.map((message) => message.id)).toEqual(['msg_latest']);

    await actions.selectSession('session_1', { skipBackgroundSync: true });

    expect(state.messages.map((message) => message.id)).toEqual(['msg_latest']);
  });

  it('preserves already loaded older history from cache when selecting the session again', async () => {
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
      messages: [createMessage('session_1', 'msg_latest', 'latest page', {
        conversation_turn_id: 'turn_latest',
      })],
      hasMore: true,
      nextBefore: 'turn_latest',
    });

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    state.messages = [
      createMessage('session_1', 'msg_older', 'cached older', {
        conversation_turn_id: 'turn_older',
      }),
      createMessage('session_1', 'msg_latest', 'cached latest', {
        conversation_turn_id: 'turn_latest',
      }),
    ];
    state.currentSessionId = 'session_1';
    state.currentSession = createSession('session_1');
    state.sessionMessagePaginationState.session_1 = {
      nextBefore: 'turn_older',
      loaded: true,
    };

    await actions.selectSession('session_1');
    await backgroundSync.mock.results[0]?.value;

    expect(state.messages.map((message) => message.id)).toEqual(['msg_older', 'msg_latest']);
    expect(state.sessionMessagePaginationState.session_1).toEqual({
      nextBefore: 'turn_older',
      loaded: true,
    });
    expect(state.hasMoreMessages).toBe(true);
    expect(readSessionMessagesCache(state, 'session_1')).toMatchObject({
      nextBefore: 'turn_older',
      loaded: true,
    });
    expect(readSessionMessagesCache(state, 'session_1')?.messages.map((message) => message.id)).toEqual(['msg_older', 'msg_latest']);
  });

  it('can force refresh messages instead of serving a cached session snapshot', async () => {
    const state = {
      sessions: [createSession('session_1')],
      currentSessionId: null,
      currentSession: null,
      activePanel: 'project',
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

    writeSessionMessagesCache(state, 'session_1', {
      messages: [createMessage('session_1', 'msg_stale', 'stale cached message')],
      nextBefore: null,
      loaded: true,
    });

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [createMessage('session_1', 'msg_latest', 'latest network message')],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    await actions.selectSession('session_1', {
      forceRefreshMessages: true,
      initialPageSize: 25,
      skipBackgroundSync: true,
    });

    expect(fetchSessionMessages).toHaveBeenCalledWith(
      {} as never,
      'session_1',
      { limit: 25, before: null },
    );
    expect(state.activePanel).toBe('chat');
    expect(state.messages.map((message) => message.id)).toEqual(['msg_latest']);
  });

});

import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Message, Session } from '../../../types';
import type {
  ChatStoreDraft,
  ChatStoreShape,
} from '../types';
import { createSelectSessionActions } from './sessions/selectSession';
import {
  mergeLatestCompactHistorySnapshot,
  readSessionMessagesCache,
  readVisibleSessionMessagesSnapshot,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  writeSessionMessagesCache,
} from './sessionsUtils';
import { setRealtimeConnectionStateSnapshot } from '../../realtime/state';

vi.mock('../helpers/sessions', () => ({
  fetchSession: vi.fn(),
}));

vi.mock('../helpers/messages', () => ({
  fetchSessionMessages: vi.fn(),
}));

import { fetchSession } from '../helpers/sessions';
import { fetchSessionMessages } from '../helpers/messages';

type FetchSessionMessagesResult = Awaited<ReturnType<typeof fetchSessionMessages>>;

afterEach(() => {
  setRealtimeConnectionStateSnapshot('idle');
  vi.clearAllMocks();
});

const createSession = (id: string): Session => ({
  id,
  title: id,
  userId: 'user_1',
  user_id: 'user_1',
  projectId: null,
  project_id: null,
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  updatedAt: new Date('2026-01-01T00:00:00.000Z'),
  messageCount: 0,
  tokenUsage: 0,
  pinned: false,
  archived: false,
  status: 'active',
  tags: null,
  metadata: null,
});

const createMessage = (
  sessionId: string,
  id: string,
  content: string,
  metadata: Message['metadata'] = {},
): Message => ({
  id,
  sessionId,
  role: 'assistant',
  content,
  status: 'completed',
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  metadata,
});

const installBackgroundSyncSpy = (state: ChatStoreShape) => {
  const syncSessionMessagesInBackground = vi.fn(async (sessionId: string) => {
    const result = await fetchSessionMessages({} as never, sessionId, { limit: 50, before: null });
    const preservedSnapshot = (
      readVisibleSessionMessagesSnapshot(state, sessionId)
      ?? readSessionMessagesCache(state, sessionId)
    );
    const mergedSnapshot = mergeLatestCompactHistorySnapshot(
      result.messages,
      result.nextBefore,
      preservedSnapshot,
    );
    writeSessionMessagesCache(state, sessionId, {
      messages: mergedSnapshot.messages,
      nextBefore: mergedSnapshot.nextBefore,
      loaded: true,
    });
  });
  (state as ChatStoreShape & {
    syncSessionMessagesInBackground: typeof syncSessionMessagesInBackground;
  }).syncSessionMessagesInBackground = syncSessionMessagesInBackground;
  return syncSessionMessagesInBackground;
};

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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
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

  it('ignores stale results from an earlier slower selection request', async () => {
    const state = {
      sessions: [createSession('session_1'), createSession('session_2')],
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
    } as unknown as ChatStoreShape;
    installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    let resolveSession1!: (result: FetchSessionMessagesResult) => void;
    let resolveSession2!: (result: FetchSessionMessagesResult) => void;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
    vi.mocked(fetchSessionMessages).mockImplementation((_client, sessionId) => {
      if (sessionId === 'session_1') {
        return new Promise((resolve) => {
          resolveSession1 = resolve;
        });
      }
      return new Promise((resolve) => {
        resolveSession2 = resolve;
      });
    });

    const actions = createSelectSessionActions({
      set,
      get,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    const firstSelection = actions.selectSession('session_1');
    const secondSelection = actions.selectSession('session_2');

    resolveSession2({
      messages: [createMessage('session_2', 'msg_2', 'session 2')],
      hasMore: false,
      nextBefore: null,
    });
    await secondSelection;

    resolveSession1({
      messages: [createMessage('session_1', 'msg_1', 'session 1')],
      hasMore: false,
      nextBefore: null,
    });
    await firstSelection;

    expect(state.currentSessionId).toBe('session_2');
    expect(state.currentSession?.id).toBe('session_2');
    expect(state.messages.map((message) => message.id)).toEqual(['msg_2']);
    expect(state.isLoading).toBe(false);
  });

  it('switches back to chat immediately for an existing session before messages finish loading', async () => {
    const state = {
      sessions: [createSession('session_1')],
      projects: [{
        id: 'project_1',
        name: 'Project 1',
        rootPath: '/tmp/project-1',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      }],
      currentSessionId: null,
      currentSession: null,
      currentProjectId: 'project_1',
      currentProject: {
        id: 'project_1',
        name: 'Project 1',
        rootPath: '/tmp/project-1',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      },
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
    } as unknown as ChatStoreShape;
    installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    let resolveMessages!: (result: FetchSessionMessagesResult) => void;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
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

    expect(state.currentSessionId).toBe('session_1');
    expect(state.currentSession?.id).toBe('session_1');
    expect(state.activePanel).toBe('chat');
    expect(state.sessionChatState.session_1).toMatchObject({
      isLoading: true,
      isStreaming: false,
      isStopping: false,
    });

    resolveMessages({
      messages: [createMessage('session_1', 'msg_1', 'session 1')],
      hasMore: false,
      nextBefore: null,
    });
    await selectionPromise;

    expect(state.sessionChatState.session_1?.isLoading).toBe(false);
    expect(state.messages.map((message) => message.id)).toEqual(['msg_1']);
    expect(state.sessionMessagePaginationState.session_1).toEqual({
      nextBefore: null,
      loaded: true,
    });
  });

  it('clears previous session messages immediately when switching to an uncached session', async () => {
    const state = {
      sessions: [createSession('session_1'), createSession('session_2')],
      currentSessionId: 'session_1',
      currentSession: createSession('session_1'),
      activePanel: 'chat',
      messages: [createMessage('session_1', 'msg_old', 'session 1 old message')],
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      hasMoreMessages: true,
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
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
    } as unknown as ChatStoreShape;
    installBackgroundSyncSpy(state);

    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    let resolveMessages!: (result: FetchSessionMessagesResult) => void;

    vi.mocked(fetchSession).mockImplementation(async (_client, sessionId) => (
      createSession(sessionId)
    ));
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

    const selectionPromise = actions.selectSession('session_2');

    expect(state.currentSessionId).toBe('session_2');
    expect(state.currentSession?.id).toBe('session_2');
    expect(state.messages).toEqual([]);
    expect(state.hasMoreMessages).toBe(false);
    expect(state.sessionMessagePaginationState.session_2).toEqual({
      nextBefore: null,
      loaded: false,
    });

    resolveMessages({
      messages: [createMessage('session_2', 'msg_new', 'session 2 message')],
      hasMore: false,
      nextBefore: null,
    });
    await selectionPromise;

    expect(state.messages.map((message) => message.id)).toEqual(['msg_new']);
    expect(state.sessionMessagePaginationState.session_2).toEqual({
      nextBefore: null,
      loaded: true,
    });
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
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
      sessionStreamingMessageDrafts: {},
      sessionTurnProcessCache: {},
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
});

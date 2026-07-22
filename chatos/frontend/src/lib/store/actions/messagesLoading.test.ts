// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it, vi } from 'vitest';
import { produce } from 'immer';

import type { Message } from '../../../types';
import type {
  ChatStoreDraft,
  ChatStoreShape,
  SessionMessagesSnapshot,
} from '../types';
import { createMessageLoadingActions } from './messagesLoading';
import {
  readSessionMessagesCache,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  writeSessionMessagesCache,
} from './sessionsUtils';

vi.mock('../helpers/messages', () => ({
  fetchSessionMessages: vi.fn(),
}));

import { fetchSessionMessages } from '../helpers/messages';

const createMessage = (
  id: string,
  content: string,
  status: Message['status'] = 'completed',
  metadata: Message['metadata'] = undefined,
): Message => ({
  id,
  sessionId: 'session_2',
  role: 'assistant',
  content,
  status,
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  metadata,
});

const createUserMessage = (
  id: string,
  content: string,
  metadata: Message['metadata'] = undefined,
): Message => ({
  id,
  sessionId: 'session_2',
  role: 'user',
  content,
  status: 'completed',
  createdAt: new Date('2026-01-01T00:00:00.000Z'),
  metadata,
});

describe('syncSessionMessagesInBackground', () => {
  const writeCache = (state: ChatStoreShape, sessionId: string, snapshot: SessionMessagesSnapshot) => {
    writeSessionMessagesCache(state, sessionId, snapshot);
  };

  const readCache = (state: ChatStoreShape, sessionId: string) => readSessionMessagesCache(state, sessionId);

  it('does not clear global loading or error state while settling another session', async () => {
    const finalAssistant = {
      ...createMessage('assistant_final', 'final from server'),
      metadata: {
        conversation_turn_id: 'turn_2',
        historyFinalForTurnId: 'turn_2',
      },
    } as Message;
    const state = {
      currentSessionId: 'session_1',
      messages: [],
      hasMoreMessages: true,
      isLoading: true,
      isStreaming: false,
      streamingMessageId: null,
      error: 'keep-existing-error',
      sessionChatState: {
        session_2: {
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          streamingMessageId: 'assistant_temp',
          activeTurnId: 'turn_2',
          streamingPreviewText: 'stale local draft',
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [finalAssistant],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.syncSessionMessagesInBackground('session_2');

    expect(state.isLoading).toBe(true);
    expect(state.error).toBe('keep-existing-error');
    expect(state.messages).toEqual([]);
    expect(readCache(state, 'session_2')).toMatchObject({
      nextBefore: null,
      loaded: true,
    });
    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual(['assistant_final']);
  });

  it('preserves already loaded older compact history during background sync of latest page', async () => {
    const newest = createMessage('newest', 'latest from server', 'completed', {
      conversation_turn_id: 'turn_latest',
    });
    const older = createMessage('older', 'older already loaded', 'completed', {
      conversation_turn_id: 'turn_older',
    });
    const state = {
      currentSessionId: 'session_2',
      messages: [older, newest],
      hasMoreMessages: true,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [newest],
      hasMore: true,
      nextBefore: 'turn_older',
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.syncSessionMessagesInBackground('session_2');

    expect(state.messages.map((message) => message.id)).toEqual(['older', 'newest']);
    expect(state.hasMoreMessages).toBe(true);
    expect(state.sessionMessagePaginationState.session_2).toEqual({
      nextBefore: 'turn_older',
      loaded: true,
    });
    expect(readCache(state, 'session_2')).toMatchObject({
      nextBefore: 'turn_older',
      loaded: true,
    });
    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual(['older', 'newest']);
  });

  it('does not erase a just-sent optimistic turn when reconciliation beats cloud persistence', async () => {
    const oldUser = createUserMessage('user_old', 'old message', {
      conversation_turn_id: 'turn_old',
    });
    const optimisticUser = createUserMessage('user_reserved', 'new message', {
      clientOptimistic: true,
      conversation_turn_id: 'turn_new',
      task_runner_async: {
        mode: 'contact_async',
        overall_status: 'processing',
        source_user_message_id: 'user_reserved',
        source_turn_id: 'turn_new',
      },
    });
    const state = {
      currentSessionId: 'session_2',
      messages: [oldUser, optimisticUser],
      hasMoreMessages: false,
      isLoading: true,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {
        session_2: {
          isLoading: true,
          isStreaming: false,
          isStopping: false,
          streamingMessageId: null,
          activeTurnId: 'turn_new',
          streamingPreviewText: '',
        },
      },
      sessionMessagePaginationState: {
        session_2: { nextBefore: null, loaded: true },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    writeCache(state, 'session_2', {
      messages: [oldUser, optimisticUser],
      nextBefore: null,
      loaded: true,
    });
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;
    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [oldUser],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({ set, get, client: {} as never });
    await actions.syncSessionMessagesInBackground('session_2');

    expect(state.messages.map((message) => message.id)).toEqual(['user_old', 'user_reserved']);
    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual([
      'user_old',
      'user_reserved',
    ]);
  });

  it('derives hasMore from nextBefore when loading older compact history', async () => {
    const existing = createMessage('existing', 'already loaded', 'completed', {
      conversation_turn_id: 'turn_existing',
    });
    const older = createMessage('older', 'older message', 'completed', {
      conversation_turn_id: 'turn_older',
    });
    const state = {
      currentSessionId: 'session_2',
      messages: [existing],
      hasMoreMessages: true,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [older],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.loadMoreMessages('session_2');

    expect(state.messages.map((message) => message.id)).toEqual(['older', 'existing']);
    expect(state.hasMoreMessages).toBe(false);
    expect(state.sessionMessagePaginationState.session_2).toEqual({
      nextBefore: null,
      loaded: true,
    });
    expect(readCache(state, 'session_2')).toMatchObject({
      nextBefore: null,
      loaded: true,
    });
    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual(['older', 'existing']);
  });

  it('does not keep hasMore enabled when server reports hasMore without a nextBefore cursor', async () => {
    const existing = createMessage('existing', 'already loaded', 'completed', {
      conversation_turn_id: 'turn_existing',
    });
    const older = createMessage('older', 'older message', 'completed', {
      conversation_turn_id: 'turn_older',
    });
    const state = {
      currentSessionId: 'session_2',
      messages: [existing],
      hasMoreMessages: true,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [older],
      hasMore: true,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.loadMoreMessages('session_2');

    expect(state.messages.map((message) => message.id)).toEqual(['older', 'existing']);
    expect(state.hasMoreMessages).toBe(false);
    expect(state.sessionMessagePaginationState.session_2).toEqual({
      nextBefore: null,
      loaded: true,
    });
  });

  it('preserves cached older compact history when syncing a non-current session in background', async () => {
    const newest = createMessage('newest', 'latest from server', 'completed', {
      conversation_turn_id: 'turn_latest',
    });
    const older = createMessage('older', 'older already loaded', 'completed', {
      conversation_turn_id: 'turn_older',
    });
    const currentOtherSessionMessage = createMessage('other_session_msg', 'current session message');
    const state = {
      currentSessionId: 'session_1',
      messages: [currentOtherSessionMessage],
      hasMoreMessages: true,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    writeCache(state, 'session_2', {
      messages: [older, newest],
      nextBefore: 'turn_older',
      loaded: true,
    });

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [newest],
      hasMore: true,
      nextBefore: 'turn_older',
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.syncSessionMessagesInBackground('session_2');

    expect(state.messages.map((message) => message.id)).toEqual(['other_session_msg']);
    expect(state.sessionMessagePaginationState.session_2).toEqual({
      nextBefore: 'turn_older',
      loaded: true,
    });
    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual(['older', 'newest']);
  });

  it('keeps a realtime task-runner callback when latest background sync has not caught up yet', async () => {
    const userMessage = createUserMessage('user_2', 'run async task', {
      conversation_turn_id: 'turn_2',
    });
    const finalAssistant = {
      ...createMessage('assistant_2', 'plan accepted', 'completed', {
        conversation_turn_id: 'turn_2',
        historyFinalForUserMessageId: 'user_2',
        historyFinalForTurnId: 'turn_2',
      }),
      sessionId: 'session_2',
    } as Message;
    const callback = {
      ...createMessage('task_runner_callback::user_2::task_1::task.completed::run_1', 'task completed', 'completed', {
        task_runner_async: {
          message_kind: 'task_terminal_update',
          source_user_message_id: 'user_2',
          source_turn_id: 'turn_2',
        },
      }),
      messageMode: 'task_runner_callback',
    } as Message;
    const state = {
      currentSessionId: 'session_2',
      messages: [userMessage, finalAssistant, callback],
      hasMoreMessages: false,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: null,
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [userMessage, finalAssistant],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.syncSessionMessagesInBackground('session_2');

    expect(state.messages.map((message) => message.id)).toEqual([
      'user_2',
      'assistant_2',
      'task_runner_callback::user_2::task_1::task.completed::run_1',
    ]);
    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual([
      'user_2',
      'assistant_2',
      'task_runner_callback::user_2::task_1::task.completed::run_1',
    ]);
  });

  it('uses the smaller initial compact-history page size for first load and load-more', async () => {
    vi.mocked(fetchSessionMessages).mockClear();
    const state = {
      currentSessionId: 'session_2',
      messages: [],
      hasMoreMessages: true,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [createMessage('latest', 'latest')],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await actions.loadMessages('session_2');
    state.sessionMessagePaginationState.session_2 = {
      nextBefore: 'turn_older',
      loaded: true,
    };
    await actions.loadMoreMessages('session_2');

    expect(vi.mocked(fetchSessionMessages).mock.calls).toEqual([
      [
        {} as never,
        'session_2',
        { limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE, before: null },
      ],
      [
        {} as never,
        'session_2',
        { limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE, before: 'turn_older' },
      ],
    ]);
  });

  it('does not leak revoked immer proxies when caching older loaded messages', async () => {
    const existing = createMessage('existing', 'already loaded', 'completed', {
      conversation_turn_id: 'turn_existing',
    });
    const older = createMessage('older', 'older message', 'completed', {
      conversation_turn_id: 'turn_older',
    });
    let state = {
      currentSessionId: 'session_2',
      messages: [existing],
      hasMoreMessages: true,
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
      error: null,
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_2: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;
    const set = vi.fn((updater: (draftState: ChatStoreDraft) => void) => {
      state = produce(state, (draft) => {
        updater(draft as unknown as ChatStoreDraft);
      }) as unknown as ChatStoreShape;
    });
    const get = () => state;

    vi.mocked(fetchSessionMessages).mockResolvedValue({
      messages: [older],
      hasMore: false,
      nextBefore: null,
    });

    const actions = createMessageLoadingActions({
      set,
      get,
      client: {} as never,
    });

    await expect(actions.loadMoreMessages('session_2')).resolves.toBeUndefined();

    expect(readCache(state, 'session_2')?.messages.map((message) => message.id)).toEqual(['older', 'existing']);
  });
});

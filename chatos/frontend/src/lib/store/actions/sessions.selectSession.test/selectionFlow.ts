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
  type ChatStoreDraft,
  type ChatStoreShape,
  type FetchSessionMessagesResult,
} from './testUtils';

describe('selectSession', () => {
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

});

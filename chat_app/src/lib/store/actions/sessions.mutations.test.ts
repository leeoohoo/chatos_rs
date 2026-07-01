// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type {
  ChatStoreDraft,
  ChatStoreShape,
  SessionMessagesSnapshot,
} from '../types';
import { createSessionMutationActions } from './sessions/mutations';
import {
  readSessionMessagesCache,
  writeSessionMessagesCache,
} from './sessionsUtils';

describe('applyRealtimeSessionSnapshot', () => {
  const writeCache = (state: ChatStoreShape, sessionId: string, snapshot: SessionMessagesSnapshot) => {
    writeSessionMessagesCache(state, sessionId, snapshot);
  };

  it('removeSessionLocally clears session pagination and cache state', () => {
    const state = {
      sessions: [{
        id: 'session_1',
        title: 'session_1',
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
      }],
      currentSessionId: 'session_1',
      currentSession: null,
      currentProjectId: null,
      currentProject: null,
      activePanel: 'chat',
      messages: [],
      isLoading: true,
      isStreaming: true,
      streamingMessageId: 'streaming_1',
      hasMoreMessages: true,
      selectedModelId: null,
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_1: {
          nextBefore: 'turn_older',
          loaded: true,
        },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;

    writeCache(state, 'session_1', {
      messages: [],
      nextBefore: 'turn_older',
      loaded: true,
    });

    const set = (updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    };

    const actions = createSessionMutationActions({
      set,
      get: (() => state) as never,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    actions.removeSessionLocally('session_1');

    expect(state.sessionMessagePaginationState.session_1).toBeUndefined();
    expect(readSessionMessagesCache(state, 'session_1')).toBeNull();
    expect(state.currentSessionId).toBeNull();
    expect(state.currentSession).toBeNull();
    expect(state.messages).toEqual([]);
    expect(state.isLoading).toBe(false);
    expect(state.isStreaming).toBe(false);
    expect(state.streamingMessageId).toBeNull();
    expect(state.hasMoreMessages).toBe(false);
  });

  it('synchronizes current project when the active session project scope changes', () => {
    const state = {
      contacts: [],
      sessions: [{
        id: 'session_1',
        title: 'session_1',
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
      }],
      projects: [{
        id: 'project_2',
        name: 'Project 2',
        rootPath: '/tmp/project-2',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      }],
      currentSessionId: 'session_1',
      currentSession: {
        id: 'session_1',
        title: 'session_1',
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
      },
      currentProjectId: null,
      currentProject: null,
      selectedModelId: null,
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;

    const set = (updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    };

    const actions = createSessionMutationActions({
      set,
      get: (() => state) as never,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    actions.applyRealtimeSessionSnapshot({
      id: 'session_1',
      title: 'session_1',
      user_id: 'user_1',
      project_id: 'project_2',
      created_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-01T00:00:00.000Z',
      metadata: {
        chat_runtime: {
          project_id: 'project_2',
        },
      },
    });

    expect(state.currentSession?.projectId).toBe('project_2');
    expect(state.currentProjectId).toBe('project_2');
    expect(state.currentProject?.id).toBe('project_2');
  });

  it('clears current project when the active session switches back to no project scope', () => {
    const state = {
      contacts: [],
      sessions: [{
        id: 'session_1',
        title: 'session_1',
        userId: 'user_1',
        user_id: 'user_1',
        projectId: 'project_2',
        project_id: 'project_2',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        tags: null,
        metadata: {
          chat_runtime: {
            project_id: 'project_2',
          },
        },
      }],
      projects: [{
        id: 'project_2',
        name: 'Project 2',
        rootPath: '/tmp/project-2',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      }],
      currentSessionId: 'session_1',
      currentSession: {
        id: 'session_1',
        title: 'session_1',
        userId: 'user_1',
        user_id: 'user_1',
        projectId: 'project_2',
        project_id: 'project_2',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        tags: null,
        metadata: {
          chat_runtime: {
            project_id: 'project_2',
          },
        },
      },
      currentProjectId: 'project_2',
      currentProject: {
        id: 'project_2',
        name: 'Project 2',
        rootPath: '/tmp/project-2',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      },
      selectedModelId: null,
      selectedAgentId: null,
      sessionAiSelectionBySession: {},
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
    } as unknown as ChatStoreShape;

    const set = (updater: (draftState: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    };

    const actions = createSessionMutationActions({
      set,
      get: (() => state) as never,
      client: {} as never,
      getSessionParams: () => ({ userId: 'user_1', projectId: '' }),
    });

    actions.applyRealtimeSessionSnapshot({
      id: 'session_1',
      title: 'session_1',
      user_id: 'user_1',
      project_id: '-1',
      created_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-01T00:00:00.000Z',
      metadata: {
        chat_runtime: {
          project_id: '-1',
        },
      },
    });

    expect(state.currentSession?.projectId).toBe('-1');
    expect(state.currentProjectId).toBeNull();
    expect(state.currentProject).toBeNull();
  });
});

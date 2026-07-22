// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { ChatStoreShape } from '../../types';
import {
  applySessionRuntimeMetadata,
  beginUserTurnInState,
  replaceOptimisticUserMessageId,
  setTaskRunnerAsyncUserMessageStatus,
} from './sessionState';

describe('applySessionRuntimeMetadata', () => {
  it('synchronizes current project from updated runtime metadata for the active session', () => {
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
      projects: [{
        id: 'project_3',
        name: 'Project 3',
        rootPath: '/tmp/project-3',
        createdAt: new Date('2026-01-01T00:00:00.000Z'),
        updatedAt: new Date('2026-01-01T00:00:00.000Z'),
      }],
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
    } as unknown as ChatStoreShape;

    applySessionRuntimeMetadata(state, 'session_1', {
      chat_runtime: {
        project_id: 'project_3',
      },
    });

    expect(state.currentSession?.metadata).toEqual({
      chat_runtime: {
        project_id: 'project_3',
      },
    });
    expect(state.currentSession?.projectId).toBe('project_3');
    expect(state.currentSession?.project_id).toBe('project_3');
    expect(state.sessions[0]?.projectId).toBe('project_3');
    expect(state.sessions[0]?.project_id).toBe('project_3');
    expect(state.currentProjectId).toBe('project_3');
    expect(state.currentProject?.id).toBe('project_3');
  });
});

describe('optimistic user turn state', () => {
  it('writes the optimistic message to visible state and session cache atomically', () => {
    const createdAt = new Date('2026-07-21T08:48:00.000Z');
    const state = {
      sessions: [{
        id: 'session_1',
        messageCount: 2,
        updatedAt: new Date('2026-07-21T08:00:00.000Z'),
      }],
      currentSessionId: 'session_1',
      currentSession: {
        id: 'session_1',
        messageCount: 2,
        updatedAt: new Date('2026-07-21T08:00:00.000Z'),
      },
      messages: [],
      sessionChatState: {},
      sessionMessagePaginationState: {
        session_1: { nextBefore: 'turn_old', loaded: true },
      },
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
    } as unknown as ChatStoreShape;
    const userMessage = {
      id: 'temp_user_1',
      sessionId: 'session_1',
      role: 'user' as const,
      content: 'hello',
      status: 'completed' as const,
      createdAt,
      metadata: {
        clientOptimistic: true,
        conversation_turn_id: 'turn_1',
        task_runner_async: {
          mode: 'contact_async',
          overall_status: 'pending',
        },
      },
    };

    beginUserTurnInState(state, {
      sessionId: 'session_1',
      userMessage,
      conversationTurnId: 'turn_1',
    });
    replaceOptimisticUserMessageId(state, 'session_1', 'temp_user_1', 'user_1');
    setTaskRunnerAsyncUserMessageStatus(state, 'session_1', 'user_1', 'processing');

    expect(state.messages.map((message) => message.id)).toEqual(['user_1']);
    expect(state.sessionMessagesCache.session_1?.messages.map((message) => message.id)).toEqual([
      'user_1',
    ]);
    expect(state.sessionMessagesCache.session_1?.messages[0]?.metadata?.task_runner_async)
      .toMatchObject({ overall_status: 'processing', source_user_message_id: 'user_1' });
    expect(state.sessions[0]?.messageCount).toBe(3);
    expect(state.currentSession?.messageCount).toBe(3);
  });

  it('does not append a delayed send into a different active session view', () => {
    const state = {
      sessions: [],
      currentSessionId: 'session_2',
      currentSession: null,
      messages: [{
        id: 'session_2_message',
        sessionId: 'session_2',
        role: 'assistant',
        content: 'other session',
        status: 'completed',
        createdAt: new Date('2026-07-21T08:00:00.000Z'),
      }],
      sessionChatState: {},
      sessionMessagePaginationState: {},
      sessionMessagesCache: {},
      sessionMessagesCacheOrder: [],
      isLoading: false,
      isStreaming: false,
      streamingMessageId: null,
    } as unknown as ChatStoreShape;
    const userMessage = {
      id: 'temp_user_1',
      sessionId: 'session_1',
      role: 'user' as const,
      content: 'hello',
      status: 'completed' as const,
      createdAt: new Date('2026-07-21T08:48:00.000Z'),
      metadata: { clientOptimistic: true, conversation_turn_id: 'turn_1' },
    };

    beginUserTurnInState(state, {
      sessionId: 'session_1',
      userMessage,
      conversationTurnId: 'turn_1',
    });

    expect(state.messages.map((message) => message.id)).toEqual(['session_2_message']);
    expect(state.sessionMessagesCache.session_1?.messages.map((message) => message.id)).toEqual([
      'temp_user_1',
    ]);
  });
});

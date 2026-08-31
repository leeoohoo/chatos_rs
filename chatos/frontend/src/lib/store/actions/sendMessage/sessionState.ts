// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../../../types';
import type {
  ChatStoreDraft,
  SessionChatState,
} from '../../types';
import {
  resolveSessionProjectScopeId,
  syncCurrentProjectFromSession,
} from '../sessionsUtils';

export const createDefaultSessionChatState = (): SessionChatState => ({
  isLoading: false,
  isStreaming: false,
  isStopping: false,
  streamingPhase: null,
  streamingMessageId: null,
  activeTurnId: null,
  streamingPreviewText: '',
  streamingTransport: null,
  runtimeContextRefreshNonce: 0,
});

const resolveSessionChatState = (
  state: ChatStoreDraft,
  sessionId: string,
): SessionChatState => state.sessionChatState[sessionId] || createDefaultSessionChatState();

const updateSessionMessageList = (
  messages: Message[],
  messageId: string,
  update: (message: Message) => Message,
): { messages: Message[]; updated: boolean } => {
  let updated = false;
  const nextMessages = messages.map((message) => {
    if (message.id !== messageId || message.role !== 'user') {
      return message;
    }
    updated = true;
    return update(message);
  });
  return { messages: nextMessages, updated };
};

const markSessionMessageCreated = (
  state: ChatStoreDraft,
  sessionId: string,
  createdAt: Date,
) => {
  const sessionIndex = state.sessions.findIndex((session) => session.id === sessionId);
  const listMessageCount = sessionIndex >= 0
    ? Number(state.sessions[sessionIndex].messageCount || 0)
    : 0;
  const currentMessageCount = state.currentSession?.id === sessionId
    ? Number(state.currentSession.messageCount || 0)
    : 0;
  const nextMessageCount = Math.max(listMessageCount, currentMessageCount, 0) + 1;
  if (sessionIndex >= 0) {
    const session = state.sessions[sessionIndex];
    session.messageCount = nextMessageCount;
    session.updatedAt = createdAt;
  }
  if (state.currentSession?.id === sessionId) {
    state.currentSession.messageCount = nextMessageCount;
    state.currentSession.updatedAt = createdAt;
  }
};

export const applySessionRuntimeMetadata = (
  state: ChatStoreDraft,
  sessionId: string,
  runtimeMetadata: Record<string, unknown>,
) => {
  const sessionIndex = state.sessions.findIndex((session) => session.id === sessionId);
  if (sessionIndex >= 0) {
    state.sessions[sessionIndex].metadata = runtimeMetadata;
    const nextProjectId = resolveSessionProjectScopeId({
      ...state.sessions[sessionIndex],
      metadata: runtimeMetadata,
    });
    state.sessions[sessionIndex].projectId = nextProjectId || null;
    state.sessions[sessionIndex].project_id = nextProjectId || null;
    if (state.currentSession?.id === sessionId) {
      syncCurrentProjectFromSession(state, state.sessions[sessionIndex]);
    }
  }

  if (state.currentSession?.id === sessionId) {
    state.currentSession.metadata = runtimeMetadata;
    const nextProjectId = resolveSessionProjectScopeId({
      ...state.currentSession,
      metadata: runtimeMetadata,
    });
    state.currentSession.projectId = nextProjectId || null;
    state.currentSession.project_id = nextProjectId || null;
    syncCurrentProjectFromSession(state, state.currentSession);
  }
};

export const setTaskRunnerAsyncUserMessageStatus = (
  state: ChatStoreDraft,
  sessionId: string,
  userMessageId: string | null | undefined,
  overallStatus: 'pending' | 'processing' | 'completed' | 'failed' | 'cancelled',
) => {
  const normalizedUserMessageId = typeof userMessageId === 'string'
    ? userMessageId.trim()
    : '';
  if (!normalizedUserMessageId) {
    return;
  }

  const update = (existingUser: Message): Message => {
    const existingMetadata = existingUser.metadata || {};
    const existingTaskRunnerAsync = (
      existingMetadata.task_runner_async
      && typeof existingMetadata.task_runner_async === 'object'
    ) ? existingMetadata.task_runner_async : {};
    return {
      ...existingUser,
      metadata: {
        ...existingMetadata,
        task_runner_async: {
          ...existingTaskRunnerAsync,
          mode: 'contact_async',
          overall_status: overallStatus,
        },
      },
    };
  };

  if (state.currentSessionId === sessionId) {
    state.messages = updateSessionMessageList(state.messages, normalizedUserMessageId, update).messages;
  }
};

export const replaceOptimisticUserMessageId = (
  state: ChatStoreDraft,
  sessionId: string,
  tempUserMessageId: string,
  persistedUserMessageId: string | null | undefined,
) => {
  const normalizedPersistedId = typeof persistedUserMessageId === 'string'
    ? persistedUserMessageId.trim()
    : '';
  if (!normalizedPersistedId || normalizedPersistedId === tempUserMessageId) {
    return tempUserMessageId;
  }

  const update = (existingUser: Message): Message => {
    const existingMetadata = existingUser.metadata || {};
    const existingTaskRunnerAsync = (
      existingMetadata.task_runner_async
      && typeof existingMetadata.task_runner_async === 'object'
    ) ? existingMetadata.task_runner_async : {};
    return {
      ...existingUser,
      id: normalizedPersistedId,
      metadata: {
        ...existingMetadata,
        task_runner_async: {
          ...existingTaskRunnerAsync,
          source_user_message_id: normalizedPersistedId,
        },
      },
    };
  };

  if (state.currentSessionId === sessionId) {
    state.messages = updateSessionMessageList(state.messages, tempUserMessageId, update).messages;
  }

  return normalizedPersistedId;
};

export const beginUserTurnInState = (
  state: ChatStoreDraft,
  {
    sessionId,
    userMessage,
    conversationTurnId,
  }: {
    sessionId: string;
    userMessage: Message;
    conversationTurnId: string;
  },
) => {
  const appendUserMessage = (messages: Message[]): Message[] => [
    ...messages.filter((message) => message.id !== userMessage.id),
    userMessage,
  ];
  if (state.currentSessionId === sessionId) {
    state.messages = appendUserMessage(state.messages || []);
  }
  markSessionMessageCreated(state, sessionId, userMessage.createdAt);

  const prev = resolveSessionChatState(state, sessionId);
  state.sessionChatState[sessionId] = {
    ...prev,
    isLoading: true,
    isStreaming: false,
    isStopping: false,
    streamingPhase: null,
    streamingMessageId: null,
    activeTurnId: conversationTurnId,
    streamingPreviewText: '',
    streamingTransport: null,
  };

  if (state.currentSessionId === sessionId) {
    state.isLoading = true;
    state.isStreaming = false;
    state.streamingMessageId = null;
  }
};

export const failSendMessageState = (
  state: ChatStoreDraft,
  {
    sessionId,
    tempAssistantId,
    tempAssistantMessage,
    failureContent,
    readableError,
  }: {
    sessionId: string;
    tempAssistantId: string | null;
    tempAssistantMessage: Message;
    failureContent: string;
    readableError: string;
  },
) => {
  const previousChatState = resolveSessionChatState(state, sessionId);
  const failedTurnId = String(previousChatState.activeTurnId || '').trim();
  const markFailedUserTurn = (messages: Message[]): Message[] => messages.map((message) => {
    const taskRunnerAsync = message.metadata?.task_runner_async;
    const messageTurnId = String(
      message.metadata?.conversation_turn_id
      || taskRunnerAsync?.source_turn_id
      || '',
    ).trim();
    if (message.role !== 'user' || !failedTurnId || messageTurnId !== failedTurnId) {
      return message;
    }
    return {
      ...message,
      metadata: {
        ...(message.metadata || {}),
        clientOptimistic: false,
        task_runner_async: {
          ...(taskRunnerAsync || {}),
          mode: 'contact_async',
          overall_status: 'failed',
        },
      },
    };
  });
  const existingAssistantIndex = state.currentSessionId === sessionId && tempAssistantId
    ? state.messages.findIndex((message) => message.id === tempAssistantId)
    : -1;
  const baseAssistant = existingAssistantIndex !== -1
    ? state.messages[existingAssistantIndex]
    : {
        ...tempAssistantMessage,
        metadata: {
          ...(tempAssistantMessage.metadata || {}),
          contentSegments: [{ content: failureContent, type: 'text' as const }],
          currentSegmentIndex: 0,
        },
      };
  const failureAssistantMessage: Message = {
    ...baseAssistant,
    role: 'assistant',
    status: 'error',
    content: failureContent,
    metadata: {
      ...(baseAssistant?.metadata || {}),
      contentSegments: [{ content: failureContent, type: 'text' as const }],
      currentSegmentIndex: 0,
      requestError: readableError,
    },
    updatedAt: new Date(),
  };

  if (existingAssistantIndex !== -1) {
    state.messages[existingAssistantIndex] = failureAssistantMessage;
  } else if (state.currentSessionId === sessionId) {
    state.messages.push(failureAssistantMessage);
  }

  if (state.currentSessionId === sessionId) {
    state.messages = markFailedUserTurn(state.messages);
  }

  state.sessionChatState[sessionId] = {
    ...previousChatState,
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
    state.error = readableError;
  }
};

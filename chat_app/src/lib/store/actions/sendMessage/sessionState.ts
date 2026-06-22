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
    state.sessions[sessionIndex].projectId = nextProjectId === '0' ? '0' : nextProjectId || null;
    state.sessions[sessionIndex].project_id = nextProjectId === '0' ? '0' : nextProjectId || null;
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
    state.currentSession.projectId = nextProjectId === '0' ? '0' : nextProjectId || null;
    state.currentSession.project_id = nextProjectId === '0' ? '0' : nextProjectId || null;
    syncCurrentProjectFromSession(state, state.currentSession);
  }
};

export const setTaskRunnerAsyncUserMessageStatus = (
  state: ChatStoreDraft,
  userMessageId: string | null | undefined,
  overallStatus: 'pending' | 'processing' | 'completed',
) => {
  const normalizedUserMessageId = typeof userMessageId === 'string'
    ? userMessageId.trim()
    : '';
  if (!normalizedUserMessageId) {
    return;
  }

  const userIndex = state.messages.findIndex((message) => (
    message.id === normalizedUserMessageId && message.role === 'user'
  ));
  if (userIndex < 0) {
    return;
  }

  const existingUser = state.messages[userIndex];
  const existingMetadata = existingUser.metadata || {};
  const existingTaskRunnerAsync = (
    existingMetadata.task_runner_async
    && typeof existingMetadata.task_runner_async === 'object'
  ) ? existingMetadata.task_runner_async : {};

  state.messages[userIndex] = {
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

export const replaceOptimisticUserMessageId = (
  state: ChatStoreDraft,
  tempUserMessageId: string,
  persistedUserMessageId: string | null | undefined,
) => {
  const normalizedPersistedId = typeof persistedUserMessageId === 'string'
    ? persistedUserMessageId.trim()
    : '';
  if (!normalizedPersistedId || normalizedPersistedId === tempUserMessageId) {
    return tempUserMessageId;
  }

  const userIndex = state.messages.findIndex((message) => (
    message.id === tempUserMessageId && message.role === 'user'
  ));
  if (userIndex < 0) {
    return normalizedPersistedId;
  }

  const existingUser = state.messages[userIndex];
  const existingMetadata = existingUser.metadata || {};
  const existingTaskRunnerAsync = (
    existingMetadata.task_runner_async
    && typeof existingMetadata.task_runner_async === 'object'
  ) ? existingMetadata.task_runner_async : {};

  state.messages[userIndex] = {
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
  state.messages.push(userMessage);

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
  const existingAssistantIndex = tempAssistantId
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

  const prev = resolveSessionChatState(state, sessionId);
  state.sessionChatState[sessionId] = {
    ...prev,
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

import type { Message } from '../../../../types';
import type {
  ChatStoreDraft,
  SessionChatState,
} from '../../types';
import {
  resolveSessionProjectScopeId,
  syncCurrentProjectFromSession,
} from '../sessionsUtils';
import {
  createEmptySessionRuntimeGuidanceState,
  resetRuntimeGuidancePendingCount,
} from './runtimeGuidanceState';
import { cloneStreamingMessageDraft } from './streamText';

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
    isStreaming: true,
    isStopping: false,
    streamingPhase: 'thinking',
    activeTurnId: conversationTurnId,
    streamingPreviewText: '',
    streamingTransport: null,
  };
  state.sessionRuntimeGuidanceState[sessionId] = createEmptySessionRuntimeGuidanceState();

  if (state.currentSessionId === sessionId) {
    state.isLoading = true;
    state.isStreaming = true;
  }
};

export const beginAssistantDraftInState = (
  state: ChatStoreDraft,
  {
    sessionId,
    userMessageId,
    assistantMessage,
    conversationTurnId,
  }: {
    sessionId: string;
    userMessageId: string;
    assistantMessage: Message;
    conversationTurnId: string;
  },
) => {
  state.messages.push(assistantMessage);

  const linkedUserMessage = state.messages.find(
    (message) => message.id === userMessageId && message.role === 'user',
  );
  if (linkedUserMessage?.metadata?.historyProcess) {
    linkedUserMessage.metadata.historyProcess.finalAssistantMessageId = assistantMessage.id;
  }

  const prev = resolveSessionChatState(state, sessionId);
  state.sessionChatState[sessionId] = {
    ...prev,
    isLoading: true,
    isStreaming: true,
    isStopping: false,
    streamingPhase: 'thinking',
    streamingMessageId: assistantMessage.id,
    activeTurnId: conversationTurnId,
    streamingPreviewText: '',
  };

  if (!state.sessionStreamingMessageDrafts) {
    state.sessionStreamingMessageDrafts = {};
  }
  state.sessionStreamingMessageDrafts[sessionId] = cloneStreamingMessageDraft(assistantMessage);

  if (state.currentSessionId === sessionId) {
    state.streamingMessageId = assistantMessage.id;
  }
};

export const finalizeStreamingSessionState = (
  state: ChatStoreDraft,
  {
    sessionId,
    assistantMessageId,
    sawDone,
  }: {
    sessionId: string;
    assistantMessageId: string;
    sawDone: boolean;
  },
) => {
  const currentDraft = state.sessionStreamingMessageDrafts?.[sessionId];

  if (currentDraft) {
    const finalizedDraft = cloneStreamingMessageDraft(currentDraft);
    finalizedDraft.status = sawDone ? 'completed' : 'error';

    const existingIndex = state.messages.findIndex((message) => message.id === assistantMessageId);
    if (existingIndex !== -1) {
      state.messages[existingIndex] = {
        ...state.messages[existingIndex],
        ...finalizedDraft,
      };
    } else if (state.currentSessionId === sessionId) {
      state.messages.push(finalizedDraft);
    }
  }

  if (state.sessionStreamingMessageDrafts) {
    state.sessionStreamingMessageDrafts[sessionId] = null;
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
  resetRuntimeGuidancePendingCount(state, sessionId);

  if (state.currentSessionId === sessionId) {
    state.isLoading = false;
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
  const currentDraft = state.sessionStreamingMessageDrafts?.[sessionId];
  const baseAssistant = existingAssistantIndex !== -1
    ? state.messages[existingAssistantIndex]
    : (
      currentDraft
        ? cloneStreamingMessageDraft(currentDraft)
        : {
            ...tempAssistantMessage,
            metadata: {
              ...(tempAssistantMessage.metadata || {}),
              contentSegments: [{ content: failureContent, type: 'text' as const }],
              currentSegmentIndex: 0,
            },
          }
    );
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

  if (state.sessionStreamingMessageDrafts) {
    state.sessionStreamingMessageDrafts[sessionId] = (
      existingAssistantIndex !== -1 || state.currentSessionId === sessionId
    )
      ? null
      : cloneStreamingMessageDraft(failureAssistantMessage);
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
  resetRuntimeGuidancePendingCount(state, sessionId);

  if (state.currentSessionId === sessionId) {
    state.isLoading = false;
    state.isStreaming = false;
    state.streamingMessageId = null;
    state.error = readableError;
  }
};

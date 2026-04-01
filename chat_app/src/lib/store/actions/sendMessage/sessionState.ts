import type { Message } from '../../../../types';
import type {
  ChatStoreDraft,
  SessionChatState,
} from '../../types';
import {
  createEmptySessionRuntimeGuidanceState,
  resetRuntimeGuidancePendingCount,
} from './runtimeGuidanceState';
import { cloneStreamingMessageDraft } from './streamText';

export const createDefaultSessionChatState = (): SessionChatState => ({
  isLoading: false,
  isStreaming: false,
  isStopping: false,
  streamingMessageId: null,
  activeTurnId: null,
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
  }

  if (state.currentSession?.id === sessionId) {
    state.currentSession.metadata = runtimeMetadata;
  }
};

export const beginUserTurnInState = (
  state: ChatStoreDraft,
  {
    sessionId,
    userMessage,
    turnProcessKey,
    conversationTurnId,
  }: {
    sessionId: string;
    userMessage: Message;
    turnProcessKey: string;
    conversationTurnId: string;
  },
) => {
  state.messages.push(userMessage);

  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }
  state.sessionTurnProcessState[sessionId][turnProcessKey] = {
    expanded: false,
    loaded: false,
    loading: false,
  };

  const prev = resolveSessionChatState(state, sessionId);
  state.sessionChatState[sessionId] = {
    ...prev,
    isLoading: true,
    isStreaming: true,
    isStopping: false,
    activeTurnId: conversationTurnId,
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
    streamingMessageId: assistantMessage.id,
    activeTurnId: conversationTurnId,
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
  let backgroundFinalizedDraft: Message | null = null;

  if (currentDraft) {
    const finalizedDraft = cloneStreamingMessageDraft(currentDraft);
    finalizedDraft.status = sawDone ? 'completed' : 'error';

    const existingIndex = state.messages.findIndex((message) => message.id === assistantMessageId);
    const shouldWriteToCurrentMessages = (
      existingIndex !== -1 || state.currentSessionId === sessionId
    );

    if (existingIndex !== -1) {
      state.messages[existingIndex] = {
        ...state.messages[existingIndex],
        ...finalizedDraft,
      };
    } else if (shouldWriteToCurrentMessages) {
      state.messages.push(finalizedDraft);
    } else {
      backgroundFinalizedDraft = finalizedDraft;
    }
  }

  if (state.sessionStreamingMessageDrafts) {
    state.sessionStreamingMessageDrafts[sessionId] = backgroundFinalizedDraft;
  }

  const prev = resolveSessionChatState(state, sessionId);
  state.sessionChatState[sessionId] = {
    ...prev,
    isLoading: false,
    isStreaming: false,
    isStopping: false,
    streamingMessageId: null,
    activeTurnId: null,
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
    streamingMessageId: null,
    activeTurnId: null,
  };
  resetRuntimeGuidancePendingCount(state, sessionId);

  if (state.currentSessionId === sessionId) {
    state.isLoading = false;
    state.isStreaming = false;
    state.streamingMessageId = null;
    state.error = readableError;
  }
};

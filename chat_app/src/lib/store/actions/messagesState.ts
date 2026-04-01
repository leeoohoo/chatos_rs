import type { Message } from '../../../types';
import type { ChatStoreDraft } from '../types';

export type TurnProcessMapValue = {
  expanded: boolean;
  loaded: boolean;
  loading: boolean;
};

export const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

export const countLoadedBaseMessages = (messages: Message[]): number => (
  messages.filter((message) => !message?.metadata?.historyProcessUserMessageId).length
);

export const readTurnProcessState = (
  sessionState: Record<string, TurnProcessMapValue> | undefined,
  processKey: string,
  userMessageId: string,
): TurnProcessMapValue | undefined => {
  if (!sessionState) {
    return undefined;
  }
  if (processKey && sessionState[processKey]) {
    return sessionState[processKey];
  }
  if (userMessageId && sessionState[userMessageId]) {
    return sessionState[userMessageId];
  }
  return undefined;
};

export const writeTurnProcessState = (
  sessionState: Record<string, TurnProcessMapValue>,
  processKey: string,
  userMessageId: string,
  value: TurnProcessMapValue,
) => {
  const key = processKey || userMessageId;
  sessionState[key] = value;
  if (userMessageId && key !== userMessageId && userMessageId in sessionState) {
    delete sessionState[userMessageId];
  }
};

export const writeTurnProcessCache = (
  sessionCache: Record<string, Message[]>,
  processKey: string,
  userMessageId: string,
  value: Message[],
) => {
  const key = processKey || userMessageId;
  sessionCache[key] = value;
  if (userMessageId && key !== userMessageId && userMessageId in sessionCache) {
    delete sessionCache[userMessageId];
  }
};

export const ensureSessionTurnMaps = (state: ChatStoreDraft, sessionId: string) => {
  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }

  if (!state.sessionTurnProcessCache) {
    state.sessionTurnProcessCache = {};
  }
  if (!state.sessionTurnProcessCache[sessionId]) {
    state.sessionTurnProcessCache[sessionId] = {};
  }
};

export const mergeMessagesWithStreamingDraft = (
  state: ChatStoreDraft,
  sessionId: string,
  messages: Message[],
): Message[] => {
  const chatState = state.sessionChatState?.[sessionId];
  const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
  let nextMessages = messages;

  if (chatState?.isStreaming && chatState.streamingMessageId) {
    const hasStreamingMessage = nextMessages.some((message) => (
      message.id === chatState.streamingMessageId
    ));
    if (!hasStreamingMessage && draftMessage && typeof draftMessage === 'object') {
      nextMessages = [...nextMessages, cloneStreamingMessageDraft(draftMessage)];
    }
  }

  if (draftMessage && typeof draftMessage === 'object') {
    const draftClone = cloneStreamingMessageDraft(draftMessage);
    const draftId = typeof draftClone?.id === 'string' ? draftClone.id : '';
    const draftIndex = draftId
      ? nextMessages.findIndex((message) => message.id === draftId)
      : -1;

    if (draftIndex === -1) {
      nextMessages = [...nextMessages, draftClone];
    } else {
      const existing = nextMessages[draftIndex];
      const existingTime = new Date(
        existing?.updatedAt || existing?.createdAt || 0,
      ).getTime();
      const draftTime = new Date(
        draftClone?.updatedAt || draftClone?.createdAt || 0,
      ).getTime();
      const existingContentLength = typeof existing?.content === 'string'
        ? existing.content.length
        : 0;
      const draftContentLength = typeof draftClone?.content === 'string'
        ? draftClone.content.length
        : 0;
      const shouldReplaceWithDraft = Boolean(
        chatState?.isStreaming
        || draftTime > existingTime
        || draftContentLength > existingContentLength
        || existing?.status !== draftClone?.status
      );
      if (shouldReplaceWithDraft) {
        nextMessages[draftIndex] = {
          ...existing,
          ...draftClone,
        };
      }
    }

    if (!chatState?.isStreaming && state.sessionStreamingMessageDrafts) {
      state.sessionStreamingMessageDrafts[sessionId] = null;
    }
  }

  return nextMessages;
};

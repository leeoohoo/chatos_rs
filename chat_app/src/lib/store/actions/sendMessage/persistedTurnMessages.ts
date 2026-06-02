import {
  normalizeRawMessages,
  normalizeTurnId,
} from '../../../domain/messages';
import type { SessionMessageResponse } from '../../../api/client/types';
import type { ChatStoreDraft } from '../../types';
import type { StreamingMessage } from './types';
import { cloneStreamingMessageDraft } from './streamText';

const readTrimmedId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const isFinalAssistantMessage = (
  message: StreamingMessage | null | undefined,
): message is StreamingMessage => Boolean(
  message
  && message.role === 'assistant'
  && !message.metadata?.historyProcessUserMessageId
  && !message.metadata?.historyProcessTurnId,
);

const readMessageTurnId = (
  message: StreamingMessage | null | undefined,
): string => normalizeTurnId(
  message?.metadata?.historyFinalForTurnId
  || message?.metadata?.conversation_turn_id
  || message?.metadata?.conversationTurnId
  || message?.metadata?.historyProcessTurnId
  || message?.metadata?.historyProcess?.turnId
  || '',
);

const readLinkedUserId = (
  message: StreamingMessage | null | undefined,
): string => readTrimmedId(message?.metadata?.historyFinalForUserMessageId);

const readDraftUserId = (
  message: StreamingMessage | null | undefined,
): string => readTrimmedId(message?.metadata?.historyDraftUserMessage?.id);

const hasMeaningfulAssistantPayload = (
  message: StreamingMessage | null | undefined,
): boolean => {
  if (!message || message.role !== 'assistant') {
    return false;
  }

  if (typeof message.content === 'string' && message.content.trim().length > 0) {
    return true;
  }

  if (typeof message.metadata?.requestError === 'string' && message.metadata.requestError.trim().length > 0) {
    return true;
  }

  const contentSegments = Array.isArray(message.metadata?.contentSegments)
    ? message.metadata.contentSegments
    : [];
  const hasMeaningfulSegments = contentSegments.some((segment) => {
    if (!segment || typeof segment !== 'object') {
      return false;
    }
    if (segment.type === 'tool_call') {
      return typeof segment.toolCallId === 'string' && segment.toolCallId.trim().length > 0;
    }
    return typeof segment.content === 'string' && segment.content.trim().length > 0;
  });
  if (hasMeaningfulSegments) {
    return true;
  }

  const toolCalls = Array.isArray(message.metadata?.toolCalls)
    ? message.metadata.toolCalls
    : [];
  return toolCalls.some((toolCall) => {
    if (!toolCall || typeof toolCall !== 'object') {
      return false;
    }
    if (typeof toolCall.name === 'string' && toolCall.name.trim().length > 0) {
      return true;
    }
    const maybeCompleted = (
      toolCall
      && typeof toolCall === 'object'
      && 'completed' in toolCall
      && (toolCall as { completed?: unknown }).completed === true
    );
    if (maybeCompleted) {
      return true;
    }
    if (typeof toolCall.error === 'string' && toolCall.error.trim().length > 0) {
      return true;
    }
    if (toolCall.result !== undefined && toolCall.result !== null) {
      if (typeof toolCall.result === 'string') {
        return toolCall.result.trim().length > 0;
      }
      return true;
    }
    return false;
  });
};

const isTerminalAssistantStatus = (
  message: StreamingMessage | null | undefined,
): boolean => (
  message?.status === 'completed' || message?.status === 'error'
);

const countLocalFinalAssistantsForTurn = (
  messages: StreamingMessage[],
  tempAssistantMessageId: string,
  tempUserId: string | null,
  expectedTurnId?: string | null,
): number => {
  const normalizedExpectedTurnId = normalizeTurnId(expectedTurnId || '');
  const normalizedTempUserId = readTrimmedId(tempUserId);

  return messages.filter((message) => {
    if (!isFinalAssistantMessage(message)) {
      return false;
    }

    const matchesTempAssistant = readTrimmedId(tempAssistantMessageId) === message.id;
    const linkedUserId = readLinkedUserId(message);
    const draftUserId = readDraftUserId(message);
    const assistantTurnId = readMessageTurnId(message);
    const matchesTurn = normalizedExpectedTurnId.length > 0 && assistantTurnId === normalizedExpectedTurnId;
    const matchesUser = normalizedTempUserId.length > 0 && (
      linkedUserId === normalizedTempUserId || draftUserId === normalizedTempUserId
    );

    return matchesTempAssistant || matchesTurn || matchesUser;
  }).length;
};

const readMessageTime = (message: StreamingMessage): number => {
  const value = message.updatedAt || message.createdAt;
  const time = value instanceof Date ? value.getTime() : new Date(value || 0).getTime();
  return Number.isFinite(time) ? time : 0;
};

const patchUserHistoryProcessFinalAssistantId = (
  state: ChatStoreDraft,
  {
    assistantMessageId,
    persistedUserMessageId,
    tempUserId,
  }: {
    assistantMessageId: string;
    persistedUserMessageId?: string | null;
    tempUserId?: string | null;
  },
) => {
  const candidateIds = [persistedUserMessageId, tempUserId]
    .map((value) => readTrimmedId(value))
    .filter((value, index, arr) => value.length > 0 && arr.indexOf(value) === index);
  if (candidateIds.length === 0) {
    return;
  }

  const userIndex = state.messages.findIndex((message) => (
    message.role === 'user' && candidateIds.includes(message.id)
  ));
  if (userIndex < 0) {
    return;
  }

  const existingUser = state.messages[userIndex];
  const existingMeta = existingUser?.metadata || {};
  const existingHistoryProcess = existingMeta.historyProcess;
  if (!existingHistoryProcess || typeof existingHistoryProcess !== 'object') {
    return;
  }

  state.messages[userIndex] = {
    ...existingUser,
    metadata: {
      ...existingMeta,
      historyProcess: {
        ...existingHistoryProcess,
        userMessageId: existingUser.id,
        finalAssistantMessageId: assistantMessageId,
      },
    },
  };
};

export const findLocalTurnAssistantCandidate = (
  messages: StreamingMessage[],
  tempAssistantMessageId: string,
  tempUserId: string | null,
  expectedTurnId?: string | null,
): StreamingMessage | null => {
  const normalizedTempAssistantId = readTrimmedId(tempAssistantMessageId);
  const normalizedTempUserId = readTrimmedId(tempUserId);
  const normalizedExpectedTurnId = normalizeTurnId(expectedTurnId || '');
  let bestMatch: StreamingMessage | null = null;
  let bestScore = -1;
  let bestTime = -1;
  let bestMeaningfulMatch: StreamingMessage | null = null;
  let bestMeaningfulScore = -1;
  let bestMeaningfulTime = -1;

  messages.forEach((message) => {
    if (!isFinalAssistantMessage(message)) {
      return;
    }

    const linkedUserId = readLinkedUserId(message);
    const draftUserId = readDraftUserId(message);
    const assistantTurnId = readMessageTurnId(message);
    let score = 0;

    if (normalizedTempAssistantId && message.id === normalizedTempAssistantId) {
      score += 32;
    }
    if (normalizedTempUserId && linkedUserId === normalizedTempUserId) {
      score += 16;
    }
    if (normalizedTempUserId && draftUserId === normalizedTempUserId) {
      score += 12;
    }
    if (normalizedExpectedTurnId && assistantTurnId === normalizedExpectedTurnId) {
      score += 8;
    }
    if (isTerminalAssistantStatus(message)) {
      score += 2;
    }
    if (hasMeaningfulAssistantPayload(message)) {
      score += 1;
    }

    if (score <= 0) {
      return;
    }

    const meaningful = hasMeaningfulAssistantPayload(message);
    const time = readMessageTime(message);
    if (meaningful && (score > bestMeaningfulScore || (score === bestMeaningfulScore && time >= bestMeaningfulTime))) {
      bestMeaningfulMatch = message;
      bestMeaningfulScore = score;
      bestMeaningfulTime = time;
    }
    if (score > bestScore || (score === bestScore && time >= bestTime)) {
      bestMatch = message;
      bestScore = score;
      bestTime = time;
    }
  });

  return bestMeaningfulMatch || bestMatch;
};

export const canUseLocalTerminalAssistant = (
  assistant: StreamingMessage | null | undefined,
  options?: {
    expectedTurnId?: string | null;
    tempUserId?: string | null;
    requireTerminalStatus?: boolean;
  },
): boolean => {
  if (!isFinalAssistantMessage(assistant)) {
    return false;
  }

  if (options?.requireTerminalStatus && !isTerminalAssistantStatus(assistant)) {
    return false;
  }

  if (!hasMeaningfulAssistantPayload(assistant)) {
    return false;
  }

  const normalizedExpectedTurnId = normalizeTurnId(options?.expectedTurnId || '');
  const normalizedTempUserId = readTrimmedId(options?.tempUserId);
  const assistantTurnId = readMessageTurnId(assistant);
  const linkedUserId = readLinkedUserId(assistant);
  const draftUserId = readDraftUserId(assistant);
  const matchesTurn = normalizedExpectedTurnId.length > 0 && assistantTurnId === normalizedExpectedTurnId;
  const matchesUser = normalizedTempUserId.length > 0 && (
    linkedUserId === normalizedTempUserId
    || draftUserId === normalizedTempUserId
  );

  if (normalizedExpectedTurnId.length > 0 || normalizedTempUserId.length > 0) {
    return matchesTurn || matchesUser;
  }

  return true;
};

export const normalizePersistedMessage = (
  rawMessage: unknown,
  sessionId: string,
): StreamingMessage | null => {
  if (!rawMessage || typeof rawMessage !== 'object' || Array.isArray(rawMessage)) {
    return null;
  }
  const normalized = normalizeRawMessages([rawMessage as SessionMessageResponse], sessionId);
  if (normalized.length === 0) {
    return null;
  }
  return normalized[0] as StreamingMessage;
};

export const reconcilePersistedTurnMessages = (
  state: ChatStoreDraft,
  sessionId: string,
  tempAssistantMessageId: string,
  tempUserId: string | null,
  persistedUserMessage: StreamingMessage | null,
  persistedAssistantMessage: StreamingMessage | null,
): {
  applied: boolean;
  resolvedAssistantMessageId: string;
} => {
  let applied = false;
  let resolvedAssistantMessageId = tempAssistantMessageId;

  if (persistedUserMessage && tempUserId) {
    const userIndex = state.messages.findIndex((message) => message.id === tempUserId);
    if (userIndex >= 0) {
      const existingUser = state.messages[userIndex];
      const existingMeta = existingUser?.metadata || {};
      const persistedMeta = persistedUserMessage.metadata || {};
      state.messages[userIndex] = {
        ...existingUser,
        ...persistedUserMessage,
        metadata: {
          ...existingMeta,
          ...persistedMeta,
          historyProcess: existingMeta.historyProcess || persistedMeta.historyProcess,
        },
      };
      applied = true;
    }
  }

  if (persistedAssistantMessage) {
    const assistantIndex = state.messages.findIndex((message) => message.id === tempAssistantMessageId);
    const persistedAssistantIndex = state.messages.findIndex((message) => (
      message.id === persistedAssistantMessage.id
    ));
    const targetIndex = persistedAssistantIndex >= 0
      ? persistedAssistantIndex
      : assistantIndex;
    const existingAssistant = targetIndex >= 0
      ? state.messages[targetIndex]
      : null;
    const existingMeta = existingAssistant?.metadata || {};
    const persistedMeta = persistedAssistantMessage.metadata || {};
    const mergedAssistantMessage: StreamingMessage = {
      ...(existingAssistant || {}),
      ...persistedAssistantMessage,
      metadata: {
        ...existingMeta,
        ...persistedMeta,
        historyFinalForUserMessageId: persistedUserMessage?.id
          || persistedMeta.historyFinalForUserMessageId
          || existingMeta.historyFinalForUserMessageId,
        historyDraftUserMessage: existingMeta.historyDraftUserMessage,
      },
    };

    if (targetIndex >= 0) {
      state.messages[targetIndex] = mergedAssistantMessage;
      if (
        tempAssistantMessageId
        && tempAssistantMessageId !== mergedAssistantMessage.id
      ) {
        state.messages = state.messages.filter((message, index) => (
          index === targetIndex || message.id !== tempAssistantMessageId
        ));
      }
      applied = true;
    } else if (state.currentSessionId === sessionId) {
      state.messages.push(mergedAssistantMessage);
      applied = true;
    }

    if (state.sessionStreamingMessageDrafts) {
      state.sessionStreamingMessageDrafts[sessionId] = cloneStreamingMessageDraft(mergedAssistantMessage);
    }

    patchUserHistoryProcessFinalAssistantId(state, {
      assistantMessageId: mergedAssistantMessage.id,
      persistedUserMessageId: persistedUserMessage?.id || null,
      tempUserId,
    });
    resolvedAssistantMessageId = mergedAssistantMessage.id;
  }

  if (persistedUserMessage && !persistedAssistantMessage) {
    const assistantIndex = state.messages.findIndex((message) => message.id === tempAssistantMessageId);
    if (assistantIndex >= 0) {
      const existingAssistant = state.messages[assistantIndex];
      const existingMeta = existingAssistant?.metadata || {};
      state.messages[assistantIndex] = {
        ...existingAssistant,
        metadata: {
          ...existingMeta,
          historyFinalForUserMessageId: persistedUserMessage.id,
          historyDraftUserMessage: existingMeta.historyDraftUserMessage,
        },
      };
      applied = true;
    }
  }

  return {
    applied,
    resolvedAssistantMessageId,
  };
};

export const shouldReloadMessagesAfterCompletion = (
  state: Pick<ChatStoreDraft, 'messages'>,
  tempAssistantMessageId: string,
  tempUserId: string | null,
): boolean => {
  const messages = Array.isArray(state.messages) ? state.messages : [];
  const hasTempAssistant = messages.some((message) => message.id === tempAssistantMessageId);
  const hasTempUser = tempUserId
    ? messages.some((message) => message.id === tempUserId)
    : false;
  return hasTempAssistant || hasTempUser;
};

export const shouldReloadMessagesAfterTerminalState = (
  state: Pick<ChatStoreDraft, 'messages'>,
  tempAssistantMessageId: string,
  tempUserId: string | null,
  options?: {
    allowLocalTerminalAssistant?: boolean;
  },
): boolean => {
  const messages = Array.isArray(state.messages) ? state.messages : [];
  const tempAssistant = messages.find((message) => message.id === tempAssistantMessageId) || null;
  const tempUser = tempUserId
    ? messages.find((message) => message.id === tempUserId && message.role === 'user') || null
    : null;
  const hasTempAssistant = Boolean(tempAssistant);
  const hasTempUser = Boolean(tempUser);

  if (!hasTempAssistant && !hasTempUser) {
    return false;
  }

  if (!options?.allowLocalTerminalAssistant) {
    return hasTempAssistant || hasTempUser;
  }

  const expectedTurnId = readMessageTurnId(tempAssistant) || readMessageTurnId(tempUser);
  const localAssistant = findLocalTurnAssistantCandidate(
    messages as StreamingMessage[],
    tempAssistantMessageId,
    tempUserId,
    expectedTurnId,
  );
  const localFinalAssistantCount = countLocalFinalAssistantsForTurn(
    messages as StreamingMessage[],
    tempAssistantMessageId,
    tempUserId,
    expectedTurnId,
  );
  if (localFinalAssistantCount > 1) {
    return true;
  }
  if (
    localAssistant
    && canUseLocalTerminalAssistant(localAssistant, {
      expectedTurnId,
      tempUserId,
      requireTerminalStatus: true,
    })
  ) {
    return false;
  }

  return hasTempAssistant || hasTempUser;
};

export const shouldRecoverStreamingSessionAfterDisconnect = (
  state: Pick<ChatStoreDraft, 'messages' | 'sessionChatState' | 'sessionStreamingMessageDrafts'>,
  sessionId: string,
): boolean => {
  const chatState = state.sessionChatState?.[sessionId];
  if (!chatState?.isStreaming || chatState.streamingTransport !== 'realtime') {
    return false;
  }

  const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
  if (!draftMessage || typeof draftMessage !== 'object') {
    return false;
  }

  const draftId = typeof draftMessage.id === 'string' ? draftMessage.id : '';
  if (!draftId) {
    return true;
  }

  return shouldReloadMessagesAfterTerminalState(
    { messages: Array.isArray(state.messages) ? state.messages : [] },
    draftId,
    typeof draftMessage.metadata?.historyFinalForUserMessageId === 'string'
      ? draftMessage.metadata.historyFinalForUserMessageId
      : (
        typeof draftMessage.metadata?.historyDraftUserMessage?.id === 'string'
          ? draftMessage.metadata.historyDraftUserMessage.id
          : null
      ),
    {
      allowLocalTerminalAssistant: true,
    },
  );
};

import type { RealtimeChatStreamPayloadWrapper } from '../../lib/realtime/types';
import { normalizePersistedMessage } from '../../lib/store/actions/sendMessage/persistedTurnMessages';
import type { StreamingMessage } from '../../lib/store/actions/sendMessage/types';
import type { ChatStoreDraft } from '../../lib/store/types';
import type { StreamEventPayload } from '../../lib/store/actions/sendMessage/types';

export interface ActiveStreamContext {
  sessionId: string;
  conversationTurnId: string;
  tempAssistantMessageId: string;
  tempUserId: string | null;
  streamedTextRef: { value: string };
}

const readMessageTurnId = (
  message: Partial<StreamingMessage> | null | undefined,
): string => readTrimmedString(
  message?.metadata?.historyFinalForTurnId
  || message?.metadata?.conversation_turn_id
  || message?.metadata?.conversationTurnId
  || message?.metadata?.historyProcessTurnId
  || message?.metadata?.historyProcess?.turnId,
) || '';

const readLinkedUserMessageId = (
  message: Partial<StreamingMessage> | null | undefined,
): string => readTrimmedString(message?.metadata?.historyFinalForUserMessageId) || '';

const readDraftUserMessageId = (
  message: Partial<StreamingMessage> | null | undefined,
): string => readTrimmedString(message?.metadata?.historyDraftUserMessage?.id) || '';

export const isStreamingMessage = (value: unknown): value is StreamingMessage => (
  Boolean(value && typeof value === 'object' && typeof (value as { id?: unknown }).id === 'string')
);

export const asRealtimeParsedEvent = (
  payload: RealtimeChatStreamPayloadWrapper,
): StreamEventPayload => {
  const raw = payload.raw && typeof payload.raw === 'object'
    ? payload.raw
    : {};
  return {
    ...raw,
    type: typeof raw.type === 'string' ? raw.type : payload.stream_type,
  };
};

export const readTrimmedString = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const resolvePayloadConversationTurnId = (
  payload: RealtimeChatStreamPayloadWrapper,
): string | null => {
  const raw = payload.raw && typeof payload.raw === 'object'
    ? payload.raw as Record<string, unknown>
    : null;
  return readTrimmedString(payload.conversation_turn_id)
    || readTrimmedString(raw?.conversation_turn_id)
    || readTrimmedString(raw?.turn_id);
};

export const resolveActiveStreamContext = (
  state: ChatStoreDraft,
  sessionId: string,
): ActiveStreamContext | null => {
  const chatState = state.sessionChatState?.[sessionId];
  if (!chatState?.isStreaming || !chatState.streamingMessageId) {
    return null;
  }
  const draft = state.sessionStreamingMessageDrafts?.[sessionId];
  if (!isStreamingMessage(draft)) {
    return null;
  }
  const conversationTurnId = String(
    draft.metadata?.conversation_turn_id
    || chatState.activeTurnId
    || '',
  ).trim();
  if (!conversationTurnId) {
    return null;
  }
  const tempUserId = typeof draft.metadata?.historyFinalForUserMessageId === 'string'
    ? draft.metadata.historyFinalForUserMessageId
    : (
      typeof draft.metadata?.historyDraftUserMessage?.id === 'string'
        ? draft.metadata.historyDraftUserMessage.id
        : null
    );
  return {
    sessionId,
    conversationTurnId,
    tempAssistantMessageId: draft.id,
    tempUserId,
    streamedTextRef: {
      value: typeof draft.content === 'string' ? draft.content : '',
    },
  };
};

export const resolvePersistedRealtimeStreamContext = (
  state: ChatStoreDraft,
  sessionId: string,
  {
    payloadTurnId,
    payloadUserMessageId,
    persistedUserMessage,
    persistedAssistantMessage,
  }: {
    payloadTurnId?: string | null;
    payloadUserMessageId?: string | null;
    persistedUserMessage: StreamingMessage | null;
    persistedAssistantMessage: StreamingMessage | null;
  },
): ActiveStreamContext | null => {
  const normalizedPayloadTurnId = readTrimmedString(payloadTurnId);
  const normalizedPayloadUserMessageId = readTrimmedString(payloadUserMessageId);
  const draft = state.sessionStreamingMessageDrafts?.[sessionId];
  const draftMessage = isStreamingMessage(draft) ? draft : null;
  const conversationTurnId = normalizedPayloadTurnId
    || readMessageTurnId(draftMessage)
    || readMessageTurnId(persistedAssistantMessage)
    || readMessageTurnId(persistedUserMessage)
    || readTrimmedString(state.sessionChatState?.[sessionId]?.activeTurnId)
    || '';

  const sessionMessages = Array.isArray(state.messages)
    ? state.messages.filter((message) => message?.sessionId === sessionId)
    : [];
  const reversedSessionMessages = [...sessionMessages].reverse();

  const localUserMessage = reversedSessionMessages.find((message) => (
    message?.role === 'user' && (
      (normalizedPayloadUserMessageId && message.id === normalizedPayloadUserMessageId)
      || (conversationTurnId && readMessageTurnId(message as StreamingMessage) === conversationTurnId)
    )
  )) as StreamingMessage | undefined;

  const tempUserId = localUserMessage?.id
    || normalizedPayloadUserMessageId
    || readLinkedUserMessageId(persistedAssistantMessage)
    || readDraftUserMessageId(persistedAssistantMessage)
    || readTrimmedString(persistedUserMessage?.id)
    || null;

  const matchesTurnOrUser = (message: StreamingMessage | null | undefined): boolean => {
    if (!message || message.role !== 'assistant') {
      return false;
    }
    const assistantTurnId = readMessageTurnId(message);
    if (conversationTurnId && assistantTurnId === conversationTurnId) {
      return true;
    }
    if (!tempUserId) {
      return false;
    }
    return (
      readLinkedUserMessageId(message) === tempUserId
      || readDraftUserMessageId(message) === tempUserId
    );
  };

  const persistedAssistantId = readTrimmedString(persistedAssistantMessage?.id);
  const localAssistantMessage = (
    (draftMessage && matchesTurnOrUser(draftMessage)) ? draftMessage : null
  ) || reversedSessionMessages.find((message) => (
    message?.role === 'assistant'
    && message.status === 'streaming'
    && matchesTurnOrUser(message as StreamingMessage)
  )) as StreamingMessage | undefined
    || reversedSessionMessages.find((message) => (
      message?.role === 'assistant'
      && persistedAssistantId
      && message.id === persistedAssistantId
    )) as StreamingMessage | undefined
    || reversedSessionMessages.find((message) => (
      message?.role === 'assistant'
      && matchesTurnOrUser(message as StreamingMessage)
    )) as StreamingMessage | undefined
    || null;

  const tempAssistantMessageId = localAssistantMessage?.id || persistedAssistantId;
  if (!tempAssistantMessageId) {
    return null;
  }

  const streamedText = typeof localAssistantMessage?.content === 'string'
    ? localAssistantMessage.content
    : (
      typeof persistedAssistantMessage?.content === 'string'
        ? persistedAssistantMessage.content
        : ''
    );

  return {
    sessionId,
    conversationTurnId,
    tempAssistantMessageId,
    tempUserId,
    streamedTextRef: {
      value: streamedText,
    },
  };
};

export const buildRealtimeCompletionKey = (
  sessionId: string,
  assistantMessageId: string,
  kind: 'cancelled' | 'done' | 'error',
  streamType: string | null,
  timestamp: unknown,
): string => {
  const normalizedTimestamp = readTrimmedString(timestamp) || '';
  if (kind === 'done') {
    return `${sessionId}:${assistantMessageId}:${streamType || ''}:${normalizedTimestamp}`;
  }
  return `${sessionId}:${assistantMessageId}:${kind}:${normalizedTimestamp}`;
};

export const shouldAttemptDisconnectRecovery = (
  state: Pick<ChatStoreDraft, 'sessionChatState' | 'sessionStreamingMessageDrafts'>,
  sessionId: string,
  realtimeConnectionState: string,
): boolean => {
  const chatState = state.sessionChatState?.[sessionId];
  if (
    !chatState?.isStreaming
    || chatState.streamingTransport !== 'realtime'
    || realtimeConnectionState === 'connected'
  ) {
    return false;
  }
  return Boolean(resolveActiveStreamContext(state as ChatStoreDraft, sessionId));
};

export const shouldRecoverMessagesForActiveSession = (
  state: Pick<ChatStoreDraft, 'currentSessionId'>,
  sessionId: string,
): boolean => state.currentSessionId === sessionId;

export const resolveLatestStreamedText = (
  state: Pick<ChatStoreDraft, 'sessionStreamingMessageDrafts'>,
  sessionId: string,
  fallback: string,
): string => {
  const content = state.sessionStreamingMessageDrafts?.[sessionId]?.content;
  if (typeof content === 'string' && content.length > 0) {
    return content;
  }
  return fallback;
};

export const collectActiveStreamingSessionIds = (
  sessionChatState: ChatStoreDraft['sessionChatState'] | null | undefined,
): string[] => (
  Object.entries(sessionChatState || {})
    .filter(([sessionId, chatState]) => (
      sessionId.trim().length > 0
      && chatState?.isStreaming === true
      && typeof chatState.streamingMessageId === 'string'
      && chatState.streamingMessageId.trim().length > 0
    ))
    .map(([sessionId]) => sessionId)
    .sort()
);

export const resolvePersistedTurnMessages = (
  raw: RealtimeChatStreamPayloadWrapper['raw'],
  sessionId: string,
): {
  persistedUserMessage: StreamingMessage | null;
  persistedAssistantMessage: StreamingMessage | null;
} => ({
  persistedUserMessage: normalizePersistedMessage(
    raw?.result?.persisted_user_message,
    sessionId,
  ),
  persistedAssistantMessage: normalizePersistedMessage(
    raw?.result?.persisted_assistant_message,
    sessionId,
  ),
});

export const shouldFinalizeRealtimeTerminalEvent = (
  processedKeys: Set<string>,
  completionKey: string,
): boolean => {
  if (processedKeys.has(completionKey)) {
    return false;
  }
  processedKeys.add(completionKey);
  return true;
};

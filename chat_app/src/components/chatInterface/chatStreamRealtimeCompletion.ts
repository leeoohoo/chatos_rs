import { buildSendMessageFailure } from '../../lib/store/actions/sendMessage/streamControlEvents';
import { handleStreamEvent } from '../../lib/store/actions/sendMessage/streamEventHandler';
import { createStreamingMessageStateHelpers } from '../../lib/store/actions/sendMessage/streamingState';
import type { ChatStoreDraft, ChatStoreSet } from '../../lib/store/types';
import {
  asRealtimeParsedEvent,
  buildRealtimeCompletionKey,
  resolveActiveStreamContext,
  resolveLatestStreamedText,
  resolvePersistedRealtimeStreamContext,
  resolvePersistedTurnMessages,
  resolvePayloadConversationTurnId,
  shouldFinalizeRealtimeTerminalEvent,
} from './chatStreamRealtimeBridgeState';
import {
  applyRealtimeTerminalMessages,
  settleRealtimeTerminalEvent,
} from './chatStreamRealtimeTerminalState';

export const handleChatStreamRealtimeCompletion = async ({
  payload,
  storeGetState,
  chatStoreSet,
  apiClient,
  processedCompletionKeysRef,
}: {
  payload: Parameters<typeof resolvePayloadConversationTurnId>[0];
  storeGetState: () => ChatStoreDraft;
  chatStoreSet: ChatStoreSet;
  apiClient: Parameters<typeof settleRealtimeTerminalEvent>[0];
  processedCompletionKeysRef: { current: Set<string> };
}) => {
  const payloadSessionId = String(payload.conversation_id || '').trim();
  if (!payloadSessionId) {
    return;
  }
  const parsed = asRealtimeParsedEvent(payload);
  const currentState = storeGetState();
  const payloadTurnId = resolvePayloadConversationTurnId(payload);
  const persisted = resolvePersistedTurnMessages(payload.raw, payloadSessionId);
  const active = resolveActiveStreamContext(currentState, payloadSessionId)
    || (
      (persisted.persistedUserMessage || persisted.persistedAssistantMessage)
        ? resolvePersistedRealtimeStreamContext(currentState, payloadSessionId, {
          payloadTurnId,
          payloadUserMessageId: payload.user_message_id,
          persistedUserMessage: persisted.persistedUserMessage,
          persistedAssistantMessage: persisted.persistedAssistantMessage,
        })
        : null
    );
  if (!active) {
    return;
  }
  if (payloadTurnId && active.conversationTurnId && payloadTurnId !== active.conversationTurnId) {
    return;
  }

  const terminalKind = parsed.type === 'cancelled'
    ? 'cancelled'
    : (
      parsed.type === 'error'
        ? 'error'
        : (
          parsed.type === 'done' || parsed.type === 'complete'
            ? 'done'
            : null
        )
    );

  const finalizeWithoutDraft = async () => {
    if (!terminalKind) {
      return;
    }
    const completionKey = buildRealtimeCompletionKey(
      active.sessionId,
      active.tempAssistantMessageId,
      terminalKind === 'done' ? 'done' : terminalKind,
      terminalKind === 'done' ? payload.stream_type : null,
      payload.raw.timestamp,
    );
    if (!shouldFinalizeRealtimeTerminalEvent(processedCompletionKeysRef.current, completionKey)) {
      return;
    }

    if (!active.conversationTurnId) {
      if (persisted.persistedUserMessage || persisted.persistedAssistantMessage) {
        applyRealtimeTerminalMessages(chatStoreSet, {
          sessionId: active.sessionId,
          turnId: '',
          tempAssistantMessageId: active.tempAssistantMessageId,
          tempUserId: active.tempUserId,
        }, persisted);
      }
      const latest = storeGetState();
      if (typeof latest.syncSessionMessagesInBackground === 'function') {
        await latest.syncSessionMessagesInBackground(active.sessionId);
      }
      return;
    }

    const fallbackAssistantMessage = (
      persisted.persistedAssistantMessage
      || currentState.sessionStreamingMessageDrafts?.[active.sessionId]
      || currentState.messages.find((message) => message.id === active.tempAssistantMessageId)
      || {
        id: active.tempAssistantMessageId,
        sessionId: active.sessionId,
        role: 'assistant' as const,
        content: '',
        status: 'error' as const,
        createdAt: new Date(),
        metadata: {
          ...(active.tempUserId ? { historyFinalForUserMessageId: active.tempUserId } : {}),
          ...(active.conversationTurnId ? { conversation_turn_id: active.conversationTurnId } : {}),
        },
      }
    );

    await settleRealtimeTerminalEvent(
      apiClient,
      chatStoreSet,
      () => storeGetState() as ChatStoreDraft,
      {
        sessionId: active.sessionId,
        turnId: active.conversationTurnId,
        tempAssistantMessageId: active.tempAssistantMessageId,
        tempUserId: active.tempUserId,
      },
      persisted,
      terminalKind === 'error' || terminalKind === 'cancelled'
        ? {
          kind: 'failure',
          tempAssistantMessage: fallbackAssistantMessage,
          failureContent: typeof fallbackAssistantMessage.content === 'string' && fallbackAssistantMessage.content.trim().length > 0
            ? fallbackAssistantMessage.content
            : 'Request failed',
          readableError: typeof payload.raw.message === 'string' && payload.raw.message.trim().length > 0
            ? payload.raw.message
            : 'Request failed',
        }
        : { kind: 'success' },
    );
  };

  active.streamedTextRef.value = resolveLatestStreamedText(
    currentState as Pick<ChatStoreDraft, 'sessionStreamingMessageDrafts'>,
    active.sessionId,
    active.streamedTextRef.value,
  );

  const tempAssistantMessage = currentState.sessionStreamingMessageDrafts?.[active.sessionId];
  if (!tempAssistantMessage || typeof tempAssistantMessage !== 'object' || typeof tempAssistantMessage.id !== 'string') {
    await finalizeWithoutDraft();
    return;
  }
  const terminalContext = {
    sessionId: active.sessionId,
    turnId: active.conversationTurnId,
    tempAssistantMessageId: active.tempAssistantMessageId,
    tempUserId: active.tempUserId,
  };

  const helpers = createStreamingMessageStateHelpers({
    set: chatStoreSet,
    currentSessionId: active.sessionId,
    tempAssistantMessage,
    tempUserId: active.tempUserId,
    conversationTurnId: active.conversationTurnId,
    streamedTextRef: active.streamedTextRef,
  });

  try {
    const result = handleStreamEvent({
      parsed,
      set: chatStoreSet,
      currentSessionId: active.sessionId,
      conversationTurnId: active.conversationTurnId,
      tempAssistantMessageId: active.tempAssistantMessageId,
      tempUserId: active.tempUserId,
      streamedTextRef: active.streamedTextRef,
      helpers,
    });

    if (result.sawCancelled) {
      const cancellationKey = buildRealtimeCompletionKey(
        active.sessionId,
        active.tempAssistantMessageId,
        'cancelled',
        null,
        payload.raw.timestamp,
      );
      if (shouldFinalizeRealtimeTerminalEvent(processedCompletionKeysRef.current, cancellationKey)) {
        void settleRealtimeTerminalEvent(
          apiClient,
          chatStoreSet,
          () => storeGetState() as ChatStoreDraft,
          terminalContext,
          persisted,
          { kind: 'success' },
        ).catch((error) => {
          console.error('Failed to recover messages after realtime cancellation:', error);
        });
      }
    }

    if (result.sawDone) {
      const completionKey = buildRealtimeCompletionKey(
        active.sessionId,
        active.tempAssistantMessageId,
        'done',
        payload.stream_type,
        payload.raw.timestamp,
      );
      if (shouldFinalizeRealtimeTerminalEvent(processedCompletionKeysRef.current, completionKey)) {
        void settleRealtimeTerminalEvent(
          apiClient,
          chatStoreSet,
          () => storeGetState() as ChatStoreDraft,
          terminalContext,
          persisted,
          { kind: 'success' },
        ).catch((error) => {
          console.error('Failed to recover messages after realtime completion:', error);
        });
      }
    }
  } catch (error) {
    const completionKey = buildRealtimeCompletionKey(
      active.sessionId,
      active.tempAssistantMessageId,
      'error',
      null,
      payload.raw.timestamp,
    );
    if (shouldFinalizeRealtimeTerminalEvent(processedCompletionKeysRef.current, completionKey)) {
      const { failureContent, readableError } = buildSendMessageFailure(
        error,
        active.streamedTextRef.value,
      );
      void settleRealtimeTerminalEvent(
        apiClient,
        chatStoreSet,
        () => storeGetState() as ChatStoreDraft,
        terminalContext,
        persisted,
        {
          kind: 'failure',
          tempAssistantMessage,
          failureContent,
          readableError,
        },
      ).catch((loadError) => {
        console.error('Failed to recover messages after realtime error:', loadError);
      });
    }
  }
};

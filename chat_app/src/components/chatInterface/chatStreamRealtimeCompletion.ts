import { buildSendMessageFailure } from '../../lib/store/actions/sendMessage/streamControlEvents';
import { handleStreamEvent } from '../../lib/store/actions/sendMessage/streamEventHandler';
import { createStreamingMessageStateHelpers } from '../../lib/store/actions/sendMessage/streamingState';
import type { ChatStoreDraft, ChatStoreSet } from '../../lib/store/types';
import {
  asRealtimeParsedEvent,
  buildRealtimeCompletionKey,
  resolveActiveStreamContext,
  resolveLatestStreamedText,
  resolvePersistedTurnMessages,
  resolvePayloadConversationTurnId,
  shouldFinalizeRealtimeTerminalEvent,
} from './chatStreamRealtimeBridgeState';
import { settleRealtimeTerminalEvent } from './chatStreamRealtimeTerminalState';

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
  const active = resolveActiveStreamContext(currentState, payloadSessionId);
  if (!active) {
    return;
  }
  const payloadTurnId = resolvePayloadConversationTurnId(payload);
  if (payloadTurnId && payloadTurnId !== active.conversationTurnId) {
    return;
  }
  active.streamedTextRef.value = resolveLatestStreamedText(
    currentState as Pick<ChatStoreDraft, 'sessionStreamingMessageDrafts'>,
    active.sessionId,
    active.streamedTextRef.value,
  );

  const tempAssistantMessage = currentState.sessionStreamingMessageDrafts?.[active.sessionId];
  if (!tempAssistantMessage || typeof tempAssistantMessage !== 'object' || typeof tempAssistantMessage.id !== 'string') {
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
        const persisted = resolvePersistedTurnMessages(payload.raw, active.sessionId);
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
        const persisted = resolvePersistedTurnMessages(payload.raw, active.sessionId);
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
      const persisted = resolvePersistedTurnMessages(payload.raw, active.sessionId);
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

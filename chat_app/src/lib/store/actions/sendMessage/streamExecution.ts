import type ApiClient from '../../../api/client';
import type { ChatStoreSet } from '../../types';
import { consumeChatStream } from './streamReader';
import { buildSendMessageFailure } from './streamControlEvents';
import { handleStreamEvent } from './streamEventHandler';
import {
  normalizePersistedMessage,
  reconcilePersistedTurnMessages,
  shouldReloadMessagesAfterTerminalState,
} from './persistedTurnMessages';
import {
  failSendMessageState,
  finalizeStreamingSessionState,
} from './sessionState';
import { createStreamingMessageStateHelpers } from './streamingState';
import { recoverStreamingTurnBySnapshot } from './turnRecovery';
import type { StreamingMessage } from './types';

interface RunStreamingAssistantTurnParams {
  apiClient: Pick<
    ApiClient,
    | 'getConversationLatestTurnRuntimeContext'
    | 'getConversationTurnRuntimeContextByTurn'
    | 'getConversationTurnMessagesByTurn'
    | 'getConversationTurnMessages'
  >;
  set: ChatStoreSet;
  getCurrentState: () => {
    currentSessionId: string | null;
    messages: StreamingMessage[];
    loadMessages: (sessionId: string) => Promise<void>;
  };
  currentSessionId: string;
  tempAssistantMessage: StreamingMessage;
  tempUserId: string | null;
  conversationTurnId: string;
  streamedTextRef: { value: string };
  response: ReadableStream<Uint8Array>;
}

interface RollbackFailedSendMessageParams {
  set: ChatStoreSet;
  currentSessionId: string;
  tempAssistantId: string | null;
  tempAssistantMessage: StreamingMessage;
  error: unknown;
  streamedTextRef: { value: string };
}

export const runStreamingAssistantTurn = async ({
  apiClient,
  set,
  getCurrentState,
  currentSessionId,
  tempAssistantMessage,
  tempUserId,
  conversationTurnId,
  streamedTextRef,
  response,
}: RunStreamingAssistantTurnParams): Promise<void> => {
  const reader = response.getReader();
  let sawDone = false;
  let persistedUserMessage: StreamingMessage | null = null;
  let persistedAssistantMessage: StreamingMessage | null = null;
  const {
    ensureStreamingMessage,
    persistStreamingMessageDraft,
    updateTurnHistoryProcess,
    appendTextToStreamingMessage,
    flushPendingTextToStreamingMessage,
    appendThinkingToStreamingMessage,
    applyCompleteContent,
  } = createStreamingMessageStateHelpers({
    set,
    currentSessionId,
    tempAssistantMessage,
    tempUserId,
    conversationTurnId,
    streamedTextRef,
  });

  try {
    const streamResult = await consumeChatStream({
      reader,
      streamedTextRef,
      flushPendingTextToStreamingMessage,
      handleParsedEvent: (parsed) => {
        if (
          parsed.type === 'complete'
          || parsed.type === 'cancelled'
          || parsed.type === 'error'
        ) {
          persistedUserMessage = normalizePersistedMessage(
            parsed.result?.persisted_user_message,
            currentSessionId,
          );
          persistedAssistantMessage = normalizePersistedMessage(
            parsed.result?.persisted_assistant_message,
            currentSessionId,
          );
        }

        return handleStreamEvent({
          parsed,
          set,
          currentSessionId,
          conversationTurnId,
          tempAssistantMessageId: tempAssistantMessage.id,
          streamedTextRef,
          helpers: {
            ensureStreamingMessage,
            persistStreamingMessageDraft,
            updateTurnHistoryProcess,
            appendTextToStreamingMessage,
            flushPendingTextToStreamingMessage,
            appendThinkingToStreamingMessage,
            applyCompleteContent,
          },
        });
      },
    });
    sawDone = streamResult.sawDone;

    const bufferedText = typeof streamedTextRef.value === 'string'
      ? streamedTextRef.value
      : '';
    if (bufferedText.trim().length > 0) {
      applyCompleteContent(bufferedText);
    }
  } finally {
    set((state) => {
      reconcilePersistedTurnMessages(
        state,
        tempAssistantMessage.id,
        tempUserId,
        persistedUserMessage,
        persistedAssistantMessage,
      );
      finalizeStreamingSessionState(state, {
        sessionId: currentSessionId,
        assistantMessageId: tempAssistantMessage.id,
        sawDone,
      });
    });

    const latestState = getCurrentState();
    if (
      latestState.currentSessionId === currentSessionId
      && shouldReloadMessagesAfterTerminalState(
        latestState,
        tempAssistantMessage.id,
        tempUserId,
        {
          allowLocalTerminalAssistant: true,
        },
      )
    ) {
      const recovered = await recoverStreamingTurnBySnapshot({
        apiClient,
        set,
        sessionId: currentSessionId,
        turnId: conversationTurnId,
        tempAssistantMessageId: tempAssistantMessage.id,
        tempUserId,
        preferredUserMessageId: tempUserId,
      });
      if (!recovered.recovered) {
        await latestState.loadMessages(currentSessionId);
      }
    }
  }
};

export const rollbackFailedSendMessage = ({
  set,
  currentSessionId,
  tempAssistantId,
  tempAssistantMessage,
  error,
  streamedTextRef,
}: RollbackFailedSendMessageParams): string => {
  const { failureContent, readableError } = buildSendMessageFailure(
    error,
    streamedTextRef.value,
  );

  set((state) => {
    failSendMessageState(state, {
      sessionId: currentSessionId,
      tempAssistantId,
      tempAssistantMessage,
      failureContent,
      readableError,
    });
  });

  return readableError;
};

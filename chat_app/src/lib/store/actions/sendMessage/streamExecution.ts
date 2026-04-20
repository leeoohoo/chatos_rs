import type { ChatStoreSet } from '../../types';
import { consumeChatStream } from './streamReader';
import { buildSendMessageFailure, handleStreamEvent } from './streamEventHandler';
import {
  failSendMessageState,
  finalizeStreamingSessionState,
} from './sessionState';
import { createStreamingMessageStateHelpers } from './streamingState';
import type { StreamingMessage } from './types';

interface RunStreamingAssistantTurnParams {
  set: ChatStoreSet;
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
  set,
  currentSessionId,
  tempAssistantMessage,
  tempUserId,
  conversationTurnId,
  streamedTextRef,
  response,
}: RunStreamingAssistantTurnParams): Promise<void> => {
  const reader = response.getReader();
  let sawDone = false;
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
      handleParsedEvent: (parsed) => handleStreamEvent({
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
      }),
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
      finalizeStreamingSessionState(state, {
        sessionId: currentSessionId,
        assistantMessageId: tempAssistantMessage.id,
        sawDone,
      });
    });
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

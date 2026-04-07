import type { ChatStoreSet } from '../../types';
import { buildSendMessageFailure } from './streamEventHandler';
import { failSendMessageState } from './sessionState';
import type { StreamingMessage } from './types';

interface RollbackFailedSendMessageParams {
  set: ChatStoreSet;
  currentSessionId: string;
  tempAssistantId: string | null;
  tempAssistantMessage: StreamingMessage;
  error: unknown;
  streamedTextRef: { value: string };
}

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

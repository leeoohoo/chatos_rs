// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildSendMessageFailure } from './streamControlEvents';
import { failSendMessageState } from './sessionState';
import type { ChatStoreSet } from '../../types';
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

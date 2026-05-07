import type { ChatStoreSet } from '../../types';
import {
  formatAssistantFailureContent,
  resolveReadableErrorMessage,
  resolveStreamErrorPayload,
} from './errorParsing';
import {
  applyAppliedRuntimeGuidanceEvent,
  applyQueuedRuntimeGuidanceEvent,
} from './runtimeGuidanceState';
import type { StreamEventPayload } from './types';

interface StreamControlContext {
  set: ChatStoreSet;
  currentSessionId: string;
}

const formatStreamErrorMessage = ({
  message,
  code,
}: {
  message: string;
  code?: string;
}): string => (
  typeof code === 'string' && code.trim().length > 0
    ? `[${code}] ${message}`
    : message
);

export const handleQueuedRuntimeGuidanceEvent = (
  parsed: StreamEventPayload,
  context: StreamControlContext,
): void => {
  context.set((state) => {
    applyQueuedRuntimeGuidanceEvent(state, context.currentSessionId, parsed);
  });
};

export const handleAppliedRuntimeGuidanceEvent = (
  parsed: StreamEventPayload,
  context: StreamControlContext,
): void => {
  context.set((state) => {
    applyAppliedRuntimeGuidanceEvent(state, context.currentSessionId, parsed);
  });
};

export const buildStreamEventError = (
  parsed: StreamEventPayload,
): Error => {
  const streamError = resolveStreamErrorPayload(parsed);
  return new Error(formatStreamErrorMessage(streamError));
};

export const throwStreamEventError = (
  parsed: StreamEventPayload,
): never => {
  throw buildStreamEventError(parsed);
};

export const buildSendMessageFailure = (
  error: unknown,
  streamedText: string,
) => {
  const readableError = resolveReadableErrorMessage(error);
  return {
    failureContent: formatAssistantFailureContent(readableError, streamedText),
    readableError,
  };
};

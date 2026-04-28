import type { ChatStoreSet } from '../../types';
import {
  ensureStreamingMetadata,
  ensureStreamingToolCalls,
  touchStreamingMessage,
  type StreamEventPayload,
} from './types';
import type { StreamingMessageStateHelpers } from './streamingState';
import { joinStreamingText, normalizeStreamedText } from './streamText';

interface StreamPreviewContext {
  set: ChatStoreSet;
  currentSessionId: string;
  streamedTextRef: { value: string };
}

interface StreamThinkingContext {
  helpers: StreamingMessageStateHelpers;
}

interface StreamCancelContext {
  set: ChatStoreSet;
  helpers: StreamingMessageStateHelpers;
}

interface StreamCompleteContext {
  helpers: StreamingMessageStateHelpers;
  streamedTextRef: { value: string };
}

const asTextContent = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

export const syncStreamingPreviewText = (
  set: ChatStoreSet,
  sessionId: string,
  previewText: string,
) => {
  set((state) => {
    const sessionState = state.sessionChatState?.[sessionId];
    if (!sessionState) {
      return;
    }
    if (sessionState.streamingPreviewText === previewText) {
      return;
    }
    sessionState.streamingPreviewText = previewText;
  });
};

export const handleChunkOrContentEvent = (
  parsed: StreamEventPayload,
  context: StreamPreviewContext,
): boolean => {
  const contentStr = asTextContent(parsed.content);
  if (!contentStr) {
    return false;
  }

  context.streamedTextRef.value = normalizeStreamedText(
    joinStreamingText(context.streamedTextRef.value, contentStr),
  );
  syncStreamingPreviewText(context.set, context.currentSessionId, context.streamedTextRef.value);

  return contentStr.trim().length > 0;
};

export const handleThinkingEvent = (
  parsed: StreamEventPayload,
  context: StreamThinkingContext,
): boolean => {
  const contentStr = asTextContent(parsed.content);
  context.helpers.appendThinkingToStreamingMessage(contentStr);
  return contentStr.trim().length > 0;
};

export const handleCancelledEvent = (
  context: StreamCancelContext,
): boolean => {
  let applied = false;

  context.set((state) => {
    const message = context.helpers.ensureStreamingMessage(state);
    if (!message) {
      return;
    }

    const toolCalls = ensureStreamingToolCalls(ensureStreamingMetadata(message));
    toolCalls.forEach((toolCall) => {
      if (!toolCall.error) {
        const hasResult = toolCall.result !== undefined
          && toolCall.result !== null
          && String(toolCall.result).trim() !== '';
        if (!hasResult) {
          toolCall.result = toolCall.result || '';
        }
        toolCall.error = '已取消';
      }
      toolCall.completed = true;
    });

    touchStreamingMessage(message);
    context.helpers.persistStreamingMessageDraft(state, message);
    applied = true;
  });

  return applied;
};

export const handleDoneEvent = (
  context: StreamThinkingContext,
): void => {
  context.helpers.flushPendingTextToStreamingMessage();
};

export const handleCompleteEvent = (
  parsed: StreamEventPayload,
  context: StreamCompleteContext,
): boolean => {
  context.helpers.flushPendingTextToStreamingMessage();
  const hasStreamedText = typeof context.streamedTextRef.value === 'string'
    && context.streamedTextRef.value.trim().length > 0;
  const finalContent = parsed?.result?.content;
  if (!hasStreamedText && typeof finalContent === 'string' && finalContent.length > 0) {
    context.helpers.applyCompleteContent(finalContent);
    return true;
  }
  return false;
};

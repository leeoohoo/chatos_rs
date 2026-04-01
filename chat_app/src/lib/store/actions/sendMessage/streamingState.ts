import type { ChatStoreDraft, ChatStoreSet } from '../../types';
import {
  cloneStreamingMessageDraft,
  joinStreamingText,
  normalizeStreamedText,
} from './streamText';
import {
  createDefaultHistoryProcessState,
  ensureContentSegments,
  ensureStreamingMetadata,
  touchStreamingMessage,
  type MessageHistoryProcessState,
  type StreamingContentSegment,
  type StreamingMessage,
} from './types';

interface StreamingStateParams {
  set: ChatStoreSet;
  currentSessionId: string;
  tempAssistantMessage: StreamingMessage;
  tempUserId: string | null;
  conversationTurnId: string;
  streamedTextRef: { value: string };
}

type HistoryProcessUpdater = (
  current: MessageHistoryProcessState,
) => Partial<MessageHistoryProcessState>;

const isTextSegment = (
  segment: StreamingContentSegment | undefined,
): segment is StreamingContentSegment => (
  Boolean(segment && segment.type === 'text')
);

const collectVisibleText = (segments: StreamingContentSegment[]): string => segments
  .filter((segment) => segment.type === 'text')
  .map((segment) => (typeof segment.content === 'string' ? segment.content : ''))
  .join('');

export const createStreamingMessageStateHelpers = ({
  set,
  currentSessionId,
  tempAssistantMessage,
  tempUserId,
  conversationTurnId,
  streamedTextRef,
}: StreamingStateParams) => {
  const ensureStreamingMessage = (
    state: ChatStoreDraft,
  ): StreamingMessage | undefined => {
    let message = state.messages.find(
      (item): item is StreamingMessage => item.id === tempAssistantMessage.id,
    );
    if (!message) {
      const savedDraft = state.sessionStreamingMessageDrafts?.[currentSessionId];
      const fallbackMessage = savedDraft
        ? cloneStreamingMessageDraft(savedDraft)
        : cloneStreamingMessageDraft({
            ...tempAssistantMessage,
            role: 'assistant' as const,
            status: 'streaming' as const,
            content: streamedTextRef.value,
          });

      const metadata = ensureStreamingMetadata(fallbackMessage);
      if (!Array.isArray(metadata.toolCalls)) {
        metadata.toolCalls = [];
      }
      if (!Array.isArray(metadata.contentSegments) || metadata.contentSegments.length === 0) {
        metadata.contentSegments = [{
          content: streamedTextRef.value,
          type: 'text' as const,
        }];
      }
      if (!Number.isInteger(metadata.currentSegmentIndex)) {
        metadata.currentSegmentIndex = 0;
      }

      if (state.currentSessionId === currentSessionId) {
        state.messages.push(fallbackMessage);
      }
      message = fallbackMessage;
    }
    return message;
  };

  const persistStreamingMessageDraft = (
    state: ChatStoreDraft,
    message: StreamingMessage | undefined,
  ) => {
    if (!message) {
      return;
    }
    if (!state.sessionStreamingMessageDrafts) {
      state.sessionStreamingMessageDrafts = {};
    }
    state.sessionStreamingMessageDrafts[currentSessionId] = cloneStreamingMessageDraft(message);
  };

  const updateTurnHistoryProcess = (
    state: ChatStoreDraft,
    updater: HistoryProcessUpdater,
  ) => {
    if (!tempUserId) {
      return;
    }

    const userMessage = state.messages.find(
      (message): message is StreamingMessage => (
        message.id === tempUserId && message.role === 'user'
      ),
    );
    if (!userMessage) {
      return;
    }

    const metadata = ensureStreamingMetadata(userMessage);
    const current = metadata.historyProcess || createDefaultHistoryProcessState({
      userMessageId: tempUserId,
      turnId: conversationTurnId,
      finalAssistantMessageId: tempAssistantMessage.id,
    });

    const patch = updater(current) || {};
    const next: MessageHistoryProcessState = {
      ...current,
      ...patch,
      userMessageId: tempUserId,
      turnId: conversationTurnId,
      finalAssistantMessageId: tempAssistantMessage.id,
    };

    const toolCallCount = Number(next.toolCallCount || 0);
    const thinkingCount = Number(next.thinkingCount || 0);
    const processMessageCount = Number(next.processMessageCount || 0);
    next.hasProcess = Boolean(
      next.hasProcess || toolCallCount > 0 || thinkingCount > 0 || processMessageCount > 0,
    );

    metadata.historyProcess = next;

    const assistantMessage = state.messages.find(
      (message): message is StreamingMessage => (
        message.id === tempAssistantMessage.id && message.role === 'assistant'
      ),
    );
    if (assistantMessage) {
      ensureStreamingMetadata(assistantMessage).historyProcessExpanded = next.expanded === true;
    }
  };

  const applyTextDeltaToMessage = (contentStr: string) => {
    if (!contentStr) {
      return;
    }

    set((state) => {
      const message = ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      const metadata = ensureStreamingMetadata(message);
      const segments = ensureContentSegments(metadata);
      const currentIndex = Number.isInteger(metadata.currentSegmentIndex)
        ? Number(metadata.currentSegmentIndex)
        : -1;
      let textIndex = -1;

      if (currentIndex >= 0 && isTextSegment(segments[currentIndex])) {
        textIndex = currentIndex;
      } else {
        for (let i = segments.length - 1; i >= 0; i -= 1) {
          if (isTextSegment(segments[i])) {
            textIndex = i;
            break;
          }
        }
      }

      if (textIndex >= 0) {
        const currentSegment = segments[textIndex];
        const currentText = typeof currentSegment?.content === 'string'
          ? currentSegment.content
          : '';
        segments[textIndex].content = normalizeStreamedText(
          joinStreamingText(currentText, contentStr),
        );
      } else {
        segments.push({
          content: normalizeStreamedText(contentStr),
          type: 'text' as const,
        });
        textIndex = segments.length - 1;
      }

      metadata.currentSegmentIndex = textIndex;
      metadata.contentSegments = segments;
      message.content = collectVisibleText(segments);
      streamedTextRef.value = message.content;
      touchStreamingMessage(message);
      persistStreamingMessageDraft(state, message);
    });
  };

  const STREAM_TEXT_FLUSH_INTERVAL_MS = 40;
  let pendingTextDelta = '';
  let flushScheduledAt = 0;
  let flushTimeoutId: ReturnType<typeof setTimeout> | null = null;
  let flushRafId: number | null = null;

  const clearTextFlushHandles = () => {
    if (flushTimeoutId !== null) {
      clearTimeout(flushTimeoutId);
      flushTimeoutId = null;
    }
    if (flushRafId !== null && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(flushRafId);
      flushRafId = null;
    }
  };

  const flushPendingTextToStreamingMessage = () => {
    if (!pendingTextDelta) {
      clearTextFlushHandles();
      return;
    }
    const nextDelta = pendingTextDelta;
    pendingTextDelta = '';
    clearTextFlushHandles();
    flushScheduledAt = Date.now();
    applyTextDeltaToMessage(nextDelta);
  };

  const schedulePendingTextFlush = () => {
    if (flushTimeoutId !== null || flushRafId !== null) {
      return;
    }

    const elapsed = Date.now() - flushScheduledAt;
    const wait = Math.max(0, STREAM_TEXT_FLUSH_INTERVAL_MS - elapsed);

    flushTimeoutId = setTimeout(() => {
      flushTimeoutId = null;
      if (typeof requestAnimationFrame === 'function') {
        flushRafId = requestAnimationFrame(() => {
          flushRafId = null;
          flushPendingTextToStreamingMessage();
        });
      } else {
        flushPendingTextToStreamingMessage();
      }
    }, wait);
  };

  const appendTextToStreamingMessage = (contentStr: string) => {
    if (!contentStr) {
      return;
    }
    pendingTextDelta = pendingTextDelta
      ? normalizeStreamedText(joinStreamingText(pendingTextDelta, contentStr))
      : normalizeStreamedText(contentStr);
    schedulePendingTextFlush();
  };

  const appendThinkingToStreamingMessage = (contentStr: string) => {
    if (!contentStr) {
      return;
    }
    flushPendingTextToStreamingMessage();

    set((state) => {
      const message = ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      const metadata = ensureStreamingMetadata(message);
      const segments = ensureContentSegments(metadata);
      const lastIdx = segments.length - 1;
      let createdThinkingSegment = false;

      if (lastIdx >= 0 && segments[lastIdx].type === 'thinking') {
        const currentContent = typeof segments[lastIdx].content === 'string'
          ? segments[lastIdx].content
          : '';
        segments[lastIdx].content = `${currentContent}${contentStr}`;
      } else {
        segments.push({ content: contentStr, type: 'thinking' as const });
        createdThinkingSegment = true;
      }

      message.content = collectVisibleText(segments);

      updateTurnHistoryProcess(state, (current) => ({
        hasProcess: true,
        thinkingCount: Number(current.thinkingCount || 0) + (createdThinkingSegment ? 1 : 0),
        processMessageCount: Number(current.processMessageCount || 0)
          + (createdThinkingSegment ? 1 : 0),
      }));

      touchStreamingMessage(message);
      persistStreamingMessageDraft(state, message);
    });
  };

  const applyCompleteContent = (finalContent: string) => {
    if (!finalContent) {
      return;
    }
    pendingTextDelta = '';
    clearTextFlushHandles();
    const safeFinalContent = normalizeStreamedText(finalContent);
    streamedTextRef.value = safeFinalContent;

    set((state) => {
      const message = ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      const metadata = ensureStreamingMetadata(message);
      const segments = ensureContentSegments(metadata);
      let textIndex = -1;
      for (let i = segments.length - 1; i >= 0; i -= 1) {
        if (segments[i].type === 'text') {
          textIndex = i;
          break;
        }
      }

      if (textIndex === -1) {
        segments.push({ content: safeFinalContent, type: 'text' as const });
        textIndex = segments.length - 1;
      } else {
        segments[textIndex].content = safeFinalContent;
        for (let i = 0; i < segments.length; i += 1) {
          if (i !== textIndex && segments[i].type === 'text') {
            segments[i].content = '';
          }
        }
      }

      metadata.contentSegments = segments;
      metadata.currentSegmentIndex = textIndex;
      message.content = safeFinalContent;
      touchStreamingMessage(message);
      persistStreamingMessageDraft(state, message);
    });
  };

  return {
    ensureStreamingMessage,
    persistStreamingMessageDraft,
    updateTurnHistoryProcess,
    appendTextToStreamingMessage,
    flushPendingTextToStreamingMessage,
    appendThinkingToStreamingMessage,
    applyCompleteContent,
  };
};

export type StreamingMessageStateHelpers = ReturnType<typeof createStreamingMessageStateHelpers>;

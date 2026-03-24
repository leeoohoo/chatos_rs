import {
  cloneStreamingMessageDraft,
  joinStreamingText,
  normalizeStreamedText,
} from './streamText';

interface StreamingStateParams {
  set: (fn: (state: any) => void) => void;
  currentSessionId: string;
  tempAssistantMessage: any;
  tempUserId: string | null;
  conversationTurnId: string;
  streamedTextRef: { value: string };
}

export const createStreamingMessageStateHelpers = ({
  set,
  currentSessionId,
  tempAssistantMessage,
  tempUserId,
  conversationTurnId,
  streamedTextRef,
}: StreamingStateParams) => {
  const ensureStreamingMessage = (state: any) => {
    let message = state.messages.find((m: any) => m.id === tempAssistantMessage.id);
    if (!message) {
      const savedDraft = state.sessionStreamingMessageDrafts?.[currentSessionId];
      const fallbackMessage = savedDraft
        ? cloneStreamingMessageDraft(savedDraft)
        : {
            ...tempAssistantMessage,
            role: 'assistant' as const,
            status: 'streaming' as const,
            content: streamedTextRef.value,
            metadata: {
              ...(tempAssistantMessage.metadata || {}),
              toolCalls: [],
              contentSegments: [{ content: streamedTextRef.value, type: 'text' as const }],
              currentSegmentIndex: 0,
            },
          };

      if (state.currentSessionId === currentSessionId) {
        state.messages.push(fallbackMessage);
      }
      message = fallbackMessage;
    }
    return message;
  };

  const persistStreamingMessageDraft = (state: any, message: any) => {
    if (!message) {
      return;
    }
    if (!state.sessionStreamingMessageDrafts) {
      state.sessionStreamingMessageDrafts = {};
    }
    state.sessionStreamingMessageDrafts[currentSessionId] = cloneStreamingMessageDraft(message);
  };

  const updateTurnHistoryProcess = (state: any, updater: (current: any) => Partial<any>) => {
    if (!tempUserId) {
      return;
    }

    const userMessage = state.messages.find((m: any) => m.id === tempUserId && m.role === 'user');
    if (!userMessage) {
      return;
    }

    if (!userMessage.metadata) {
      userMessage.metadata = {} as any;
    }

    const current = userMessage.metadata.historyProcess || {
      hasProcess: false,
      toolCallCount: 0,
      thinkingCount: 0,
      processMessageCount: 0,
      userMessageId: tempUserId,
      turnId: conversationTurnId,
      finalAssistantMessageId: tempAssistantMessage.id,
      expanded: false,
      loaded: false,
      loading: false,
    };

    const patch = updater(current) || {};
    const next = {
      ...current,
      ...patch,
      userMessageId: tempUserId,
      turnId: conversationTurnId,
      finalAssistantMessageId: tempAssistantMessage.id,
    };

    const toolCallCount = Number(next.toolCallCount || 0);
    const thinkingCount = Number(next.thinkingCount || 0);
    const processMessageCount = Number(next.processMessageCount || 0);
    next.hasProcess = Boolean(next.hasProcess || toolCallCount > 0 || thinkingCount > 0 || processMessageCount > 0);

    userMessage.metadata.historyProcess = next;

    const assistantMessage = state.messages.find((m: any) => m.id === tempAssistantMessage.id && m.role === 'assistant');
    if (assistantMessage?.metadata) {
      assistantMessage.metadata.historyProcessExpanded = next.expanded === true;
    }
  };

  const applyTextDeltaToMessage = (contentStr: string) => {
    if (!contentStr) return;

    set((state: any) => {
      const message = ensureStreamingMessage(state);
      if (message && message.metadata) {
        const currentIndex = Number.isInteger(message.metadata.currentSegmentIndex)
          ? Number(message.metadata.currentSegmentIndex)
          : -1;
        const segments = message.metadata.contentSegments || [];
        let textIndex = -1;

        if (currentIndex >= 0 && segments[currentIndex] && segments[currentIndex].type === 'text') {
          textIndex = currentIndex;
        } else {
          for (let i = segments.length - 1; i >= 0; i -= 1) {
            if (segments[i]?.type === 'text') {
              textIndex = i;
              break;
            }
          }
        }

        if (textIndex >= 0) {
          const currentText = typeof segments[textIndex].content === 'string'
            ? segments[textIndex].content
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

        message.metadata.currentSegmentIndex = textIndex;
        message.metadata.contentSegments = segments;
        message.content = segments
          .filter((s: any) => s.type === 'text')
          .map((s: any) => s.content)
          .join('');
        streamedTextRef.value = message.content;
        (message as any).updatedAt = new Date();
      }
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
    if (!contentStr) return;
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

    set((state: any) => {
      const message = ensureStreamingMessage(state);
      if (!message || !message.metadata) {
        return;
      }

      const segments = message.metadata.contentSegments || [];
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

      message.content = segments
        .filter((s: any) => s.type === 'text')
        .map((s: any) => s.content)
        .join('');

      updateTurnHistoryProcess(state, (current: any) => ({
        hasProcess: true,
        thinkingCount: Number(current?.thinkingCount || 0) + (createdThinkingSegment ? 1 : 0),
        processMessageCount: Number(current?.processMessageCount || 0) + (createdThinkingSegment ? 1 : 0),
      }));

      (message as any).updatedAt = new Date();
      persistStreamingMessageDraft(state, message);
    });
  };

  const applyCompleteContent = (finalContent: string) => {
    if (!finalContent) return;
    pendingTextDelta = '';
    clearTextFlushHandles();
    const normalizedFinalContent = normalizeStreamedText(finalContent);
    const normalizedCurrentContent = normalizeStreamedText(streamedTextRef.value || '');
    // complete 事件在后端已经做过流式文本合并，这里不要再做一次拼接，避免正文重复。
    // 若 complete 内容意外更短，保留已接收的更长内容兜底。
    const safeFinalContent = normalizedFinalContent.length >= normalizedCurrentContent.length
      ? normalizedFinalContent
      : normalizedCurrentContent;
    streamedTextRef.value = safeFinalContent;

    set((state: any) => {
      const message = ensureStreamingMessage(state);
      if (!message || !message.metadata) return;

      const segments = message.metadata.contentSegments || [];
      let textIndex = -1;
      for (let i = segments.length - 1; i >= 0; i--) {
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
        for (let i = 0; i < segments.length; i++) {
          if (i !== textIndex && segments[i].type === 'text') {
            segments[i].content = '';
          }
        }
      }

      message.metadata.contentSegments = segments;
      message.metadata.currentSegmentIndex = textIndex;
      message.content = safeFinalContent;
      (message as any).updatedAt = new Date();
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

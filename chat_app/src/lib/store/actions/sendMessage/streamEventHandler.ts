import { debugLog } from '@/lib/utils';
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
import type { StreamingMessageStateHelpers } from './streamingState';
import {
  extractTaskReviewPanelFromToolStream,
  extractUiPromptPanelFromToolStream,
} from './toolPanels';
import {
  markToolCallAsWaitingForPanel,
  upsertTaskReviewPanelState,
  upsertUiPromptPanelState,
} from './toolPanelState';
import {
  applyToolEndResultsToMessage,
  applyToolStartToMessage,
  applyToolStreamDataToMessage,
  extractToolCallsFromStartPayload,
  extractToolResultsFromEndPayload,
} from './toolEvents';
import {
  ensureStreamingMetadata,
  ensureStreamingToolCalls,
  touchStreamingMessage,
  type RawToolResultPayload,
  type StreamEventPayload,
} from './types';
import { joinStreamingText, normalizeStreamedText } from './streamText';

export interface HandleStreamEventResult {
  sawCancelled: boolean;
  sawDone: boolean;
  sawMeaningfulStreamData: boolean;
}

interface HandleStreamEventParams {
  parsed: StreamEventPayload;
  set: ChatStoreSet;
  currentSessionId: string;
  conversationTurnId: string;
  tempAssistantMessageId: string;
  streamedTextRef: { value: string };
  helpers: StreamingMessageStateHelpers;
}

const EMPTY_RESULT: HandleStreamEventResult = {
  sawCancelled: false,
  sawDone: false,
  sawMeaningfulStreamData: false,
};

const asTextContent = (value: unknown): string => (
  typeof value === 'string' ? value : ''
);

const markPendingToolCallsCancelled = ({
  set,
  helpers,
}: Pick<HandleStreamEventParams, 'set' | 'helpers'>) => {
  set((state) => {
    const message = helpers.ensureStreamingMessage(state);
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
    helpers.persistStreamingMessageDraft(state, message);
  });
};

const syncStreamingPreviewText = (
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

export const handleStreamEvent = ({
  parsed,
  set,
  currentSessionId,
  conversationTurnId,
  tempAssistantMessageId,
  streamedTextRef,
  helpers,
}: HandleStreamEventParams): HandleStreamEventResult => {
  const isTextDeltaEvent = parsed.type === 'chunk' || parsed.type === 'content';
  if (!isTextDeltaEvent) {
    helpers.flushPendingTextToStreamingMessage();
  }

  if (parsed.type === 'chunk') {
    const contentStr = asTextContent(parsed.content);
    if (contentStr) {
      streamedTextRef.value = normalizeStreamedText(
        joinStreamingText(streamedTextRef.value, contentStr),
      );
      syncStreamingPreviewText(set, currentSessionId, streamedTextRef.value);
    }
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: contentStr.trim().length > 0,
    };
  }

  if (parsed.type === 'thinking') {
    const contentStr = asTextContent(parsed.content);
    helpers.appendThinkingToStreamingMessage(contentStr);
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: contentStr.trim().length > 0,
    };
  }

  if (parsed.type === 'content') {
    const contentStr = asTextContent(parsed.content);
    if (contentStr) {
      streamedTextRef.value = normalizeStreamedText(
        joinStreamingText(streamedTextRef.value, contentStr),
      );
      syncStreamingPreviewText(set, currentSessionId, streamedTextRef.value);
    }
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: contentStr.trim().length > 0,
    };
  }

  if (parsed.type === 'tools_start') {
    debugLog('🔧 收到工具调用:', parsed.data);
    const toolCallsArray = extractToolCallsFromStartPayload(parsed.data);

    set((state) => {
      const message = helpers.ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      const addedCount = applyToolStartToMessage(
        message,
        toolCallsArray,
        tempAssistantMessageId,
      );

      helpers.updateTurnHistoryProcess(state, (current) => ({
        hasProcess: true,
        toolCallCount: Number(current.toolCallCount || 0) + addedCount,
        processMessageCount: Number(current.processMessageCount || 0) + addedCount,
      }));

      touchStreamingMessage(message);
      helpers.persistStreamingMessageDraft(state, message);
    });

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'tools_end') {
    debugLog('🔧 收到工具结果:', parsed.data);
    const resultsArray = extractToolResultsFromEndPayload(parsed.data);

    set((state) => {
      const message = helpers.ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      applyToolEndResultsToMessage(message, resultsArray);
      touchStreamingMessage(message);
      helpers.persistStreamingMessageDraft(state, message);
    });

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'tools_stream') {
    debugLog('🔧 收到工具流式数据:', parsed.data);
    const data = parsed.data as RawToolResultPayload;
    const reviewPanel = extractTaskReviewPanelFromToolStream(
      data,
      currentSessionId,
      conversationTurnId,
    );
    if (reviewPanel) {
      debugLog('📝 收到任务确认事件，打开任务编辑面板:', reviewPanel);
      set((state) => {
        upsertTaskReviewPanelState(state, reviewPanel);

        const message = helpers.ensureStreamingMessage(state);
        if (!message) {
          return;
        }
        markToolCallAsWaitingForPanel(message, data, 'Waiting for task confirmation...');
        touchStreamingMessage(message);
        helpers.persistStreamingMessageDraft(state, message);
      });
      return EMPTY_RESULT;
    }

    const uiPromptPanel = extractUiPromptPanelFromToolStream(
      data,
      currentSessionId,
      conversationTurnId,
    );
    if (uiPromptPanel) {
      debugLog('🧩 收到 UI Prompt 事件，打开交互面板:', uiPromptPanel);
      set((state) => {
        upsertUiPromptPanelState(state, uiPromptPanel);

        const message = helpers.ensureStreamingMessage(state);
        if (!message) {
          return;
        }
        markToolCallAsWaitingForPanel(message, data, 'Waiting for UI prompt response...');
        touchStreamingMessage(message);
        helpers.persistStreamingMessageDraft(state, message);
      });
      return EMPTY_RESULT;
    }

    set((state) => {
      const message = helpers.ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      const updated = applyToolStreamDataToMessage(message, data);
      if (!updated) {
        return;
      }

      touchStreamingMessage(message);
      helpers.persistStreamingMessageDraft(state, message);
    });

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'runtime_guidance_queued') {
    set((state) => {
      applyQueuedRuntimeGuidanceEvent(state, currentSessionId, parsed);
    });
    return EMPTY_RESULT;
  }

  if (parsed.type === 'runtime_guidance_applied') {
    set((state) => {
      applyAppliedRuntimeGuidanceEvent(state, currentSessionId, parsed);
    });
    return EMPTY_RESULT;
  }

  if (parsed.type === 'error') {
    const streamError = resolveStreamErrorPayload(parsed);
    throw new Error(
      typeof streamError.code === 'string' && streamError.code.trim().length > 0
        ? `[${streamError.code}] ${streamError.message}`
        : streamError.message,
    );
  }

  if (parsed.type === 'cancelled') {
    markPendingToolCallsCancelled({ set, helpers });
    debugLog('⚠️ 收到取消事件，等待后端完成信号...');
    return {
      ...EMPTY_RESULT,
      sawCancelled: true,
    };
  }

  if (parsed.type === 'done') {
    helpers.flushPendingTextToStreamingMessage();
    debugLog('✅ 收到完成信号');
    return {
      ...EMPTY_RESULT,
      sawDone: true,
    };
  }

  if (parsed.type === 'complete') {
    helpers.flushPendingTextToStreamingMessage();
    const hasStreamedText = typeof streamedTextRef.value === 'string'
      && streamedTextRef.value.trim().length > 0;
    const finalContent = parsed?.result?.content;
    if (!hasStreamedText && typeof finalContent === 'string' && finalContent.length > 0) {
      helpers.applyCompleteContent(finalContent);
    }
    return {
      ...EMPTY_RESULT,
      sawDone: true,
    };
  }

  return EMPTY_RESULT;
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

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
  ensureUnavailableTools,
  ensureStreamingToolCalls,
  touchStreamingMessage,
  type UnavailableToolEntry,
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

interface RawUnavailableToolPayload {
  server_name?: string;
  serverName?: string;
  tool_name?: string;
  toolName?: string;
  reason?: string;
}

const normalizeUnavailableEntry = (
  value: RawUnavailableToolPayload,
  index: number,
): UnavailableToolEntry => {
  const serverName = (
    typeof value.server_name === 'string' && value.server_name.trim().length > 0
      ? value.server_name.trim()
      : (typeof value.serverName === 'string' && value.serverName.trim().length > 0
        ? value.serverName.trim()
        : 'unknown_server')
  );
  const toolName = (
    typeof value.tool_name === 'string' && value.tool_name.trim().length > 0
      ? value.tool_name.trim()
      : (typeof value.toolName === 'string' && value.toolName.trim().length > 0
        ? value.toolName.trim()
        : 'unknown_tool')
  );
  const reason = (
    typeof value.reason === 'string' && value.reason.trim().length > 0
      ? value.reason.trim()
      : '工具当前不可用'
  );
  const createdAt = new Date().toISOString();
  return {
    id: `unavailable_tool_${Date.now()}_${index}`,
    serverName,
    toolName,
    reason,
    createdAt,
  };
};

const unavailableEntryKey = (entry: UnavailableToolEntry): string => (
  `${entry.serverName}::${entry.toolName}::${entry.reason}`
);

const extractUnavailableToolsFromPayload = (
  data: unknown,
): RawUnavailableToolPayload[] => {
  const rawUnavailableTools = (
    data && typeof data === 'object' && 'unavailable_tools' in data
      ? (data as { unavailable_tools?: unknown }).unavailable_tools
      : data
  );
  if (Array.isArray(rawUnavailableTools)) {
    return rawUnavailableTools as RawUnavailableToolPayload[];
  }
  return rawUnavailableTools ? [rawUnavailableTools as RawUnavailableToolPayload] : [];
};

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

  if (parsed.type === 'tools_unavailable') {
    const unavailableTools = extractUnavailableToolsFromPayload(parsed.data);
    if (unavailableTools.length === 0) {
      return EMPTY_RESULT;
    }

    set((state) => {
      const message = helpers.ensureStreamingMessage(state);
      if (!message) {
        return;
      }

      const metadata = ensureStreamingMetadata(message);
      const items = ensureUnavailableTools(metadata);
      const existingKeys = new Set(items.map(unavailableEntryKey));
      let addedCount = 0;
      unavailableTools.forEach((tool, index) => {
        const normalized = normalizeUnavailableEntry(tool, index);
        const key = unavailableEntryKey(normalized);
        if (existingKeys.has(key)) {
          return;
        }
        items.push(normalized);
        existingKeys.add(key);
        addedCount += 1;
      });
      metadata.unavailableTools = items;

      helpers.updateTurnHistoryProcess(state, (current) => ({
        hasProcess: true,
        unavailableToolCount: Number(current.unavailableToolCount || 0) + addedCount,
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

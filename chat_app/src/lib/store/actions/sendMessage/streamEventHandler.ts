import { debugLog } from '@/lib/utils';
import type { ChatStoreSet } from '../../types';
import {
  handleCancelledEvent,
  handleChunkOrContentEvent,
  handleCompleteEvent,
  handleDoneEvent,
  handleThinkingEvent,
  handleTurnPhaseEvent,
} from './streamLifecycleEvents';
import { setTaskRunnerAsyncUserMessageStatus } from './sessionState';
import {
  handleAppliedRuntimeGuidanceEvent,
  handleQueuedRuntimeGuidanceEvent,
  throwStreamEventError,
} from './streamControlEvents';
import type { StreamingMessageStateHelpers } from './streamingState';
import {
  handleToolsEndEvent,
  handleToolsStartEvent,
  handleToolsStreamEvent,
  handleToolsUnavailableEvent,
} from './toolStreamEvents';
import {
  type StreamEventPayload,
} from './types';

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
  tempUserId?: string | null;
  streamedTextRef: { value: string };
  helpers: StreamingMessageStateHelpers;
}

const EMPTY_RESULT: HandleStreamEventResult = {
  sawCancelled: false,
  sawDone: false,
  sawMeaningfulStreamData: false,
};

export const handleStreamEvent = ({
  parsed,
  set,
  currentSessionId,
  conversationTurnId,
  tempAssistantMessageId,
  tempUserId,
  streamedTextRef,
  helpers,
}: HandleStreamEventParams): HandleStreamEventResult => {
  const isTextDeltaEvent = parsed.type === 'chunk' || parsed.type === 'content';
  if (!isTextDeltaEvent) {
    helpers.flushPendingTextToStreamingMessage();
  }

  if (parsed.type === 'chunk') {
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: handleChunkOrContentEvent(parsed, {
        set,
        currentSessionId,
        streamedTextRef,
      }),
    };
  }

  if (parsed.type === 'thinking') {
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: handleThinkingEvent(parsed, { helpers }),
    };
  }

  if (parsed.type === 'start') {
    if (tempUserId) {
      set((state) => {
        setTaskRunnerAsyncUserMessageStatus(state, tempUserId, 'processing');
      });
    }
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'turn_phase') {
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: handleTurnPhaseEvent(parsed, {
        set,
        currentSessionId,
      }),
    };
  }

  if (parsed.type === 'content') {
    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: handleChunkOrContentEvent(parsed, {
        set,
        currentSessionId,
        streamedTextRef,
      }),
    };
  }

  if (parsed.type === 'tools_start') {
    debugLog('🔧 收到工具调用:', parsed.data);
    handleToolsStartEvent(parsed, {
      set,
      helpers,
      currentSessionId,
      conversationTurnId,
      tempAssistantMessageId,
    });

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'tools_unavailable') {
    handleToolsUnavailableEvent(parsed, {
      set,
      helpers,
      currentSessionId,
      conversationTurnId,
      tempAssistantMessageId,
    });

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'tools_end') {
    debugLog('🔧 收到工具结果:', parsed.data);
    handleToolsEndEvent(parsed, {
      set,
      helpers,
      currentSessionId,
      conversationTurnId,
      tempAssistantMessageId,
    });

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'tools_stream') {
    debugLog('🔧 收到工具流式数据:', parsed.data);
    const { openedPanel } = handleToolsStreamEvent(parsed, {
      set,
      helpers,
      currentSessionId,
      conversationTurnId,
      tempAssistantMessageId,
    });
    if (openedPanel) {
      return EMPTY_RESULT;
    }

    return {
      ...EMPTY_RESULT,
      sawMeaningfulStreamData: true,
    };
  }

  if (parsed.type === 'runtime_guidance_queued') {
    handleQueuedRuntimeGuidanceEvent(parsed, { set, currentSessionId });
    return EMPTY_RESULT;
  }

  if (parsed.type === 'runtime_guidance_applied') {
    handleAppliedRuntimeGuidanceEvent(parsed, { set, currentSessionId });
    return EMPTY_RESULT;
  }

  if (parsed.type === 'error') {
    throwStreamEventError(parsed);
  }

  if (parsed.type === 'cancelled') {
    handleCancelledEvent({ set, helpers });
    debugLog('⚠️ 收到取消事件，等待后端完成信号...');
    return {
      ...EMPTY_RESULT,
      sawCancelled: true,
    };
  }

  if (parsed.type === 'done') {
    handleDoneEvent({
      helpers,
      streamedTextRef,
    });
    debugLog('✅ 收到完成信号');
    return {
      ...EMPTY_RESULT,
      sawDone: true,
    };
  }

  if (parsed.type === 'complete') {
    handleCompleteEvent(parsed, {
      set,
      currentSessionId,
      helpers,
      streamedTextRef,
    });
    return {
      ...EMPTY_RESULT,
      sawDone: true,
    };
  }

  return EMPTY_RESULT;
};

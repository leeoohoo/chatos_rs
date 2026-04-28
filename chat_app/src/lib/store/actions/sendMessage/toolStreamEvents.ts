import type { ChatStoreSet } from '../../types';
import { mergeUnavailableToolEntries } from '../../../domain/toolAvailability';
import {
  extractTaskBoardUpdatedEvent,
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
import type { StreamingMessageStateHelpers } from './streamingState';
import {
  ensureStreamingMetadata,
  ensureUnavailableTools,
  touchStreamingMessage,
  type RawToolResultPayload,
  type StreamEventPayload,
  type UnavailableToolEntry,
} from './types';

export interface ToolStreamContext {
  set: ChatStoreSet;
  helpers: StreamingMessageStateHelpers;
  currentSessionId: string;
  conversationTurnId: string;
  tempAssistantMessageId: string;
}

export const handleToolsStartEvent = (
  parsed: StreamEventPayload,
  context: ToolStreamContext,
): boolean => {
  const toolCallsArray = extractToolCallsFromStartPayload(parsed.data);

  context.set((state) => {
    const message = context.helpers.ensureStreamingMessage(state);
    if (!message) {
      return;
    }

    const addedCount = applyToolStartToMessage(
      message,
      toolCallsArray,
      context.tempAssistantMessageId,
    );

    context.helpers.updateTurnHistoryProcess(state, (current) => ({
      hasProcess: true,
      toolCallCount: Number(current.toolCallCount || 0) + addedCount,
      processMessageCount: Number(current.processMessageCount || 0) + addedCount,
    }));

    touchStreamingMessage(message);
    context.helpers.persistStreamingMessageDraft(state, message);
  });

  return true;
};

export const handleToolsUnavailableEvent = (
  parsed: StreamEventPayload,
  context: ToolStreamContext,
): boolean => {
  let applied = false;

  context.set((state) => {
    const message = context.helpers.ensureStreamingMessage(state);
    if (!message) {
      return;
    }

    const metadata = ensureStreamingMetadata(message);
    const currentItems = ensureUnavailableTools(metadata) as UnavailableToolEntry[];
    const { items, addedCount } = mergeUnavailableToolEntries(currentItems, parsed.data);
    if (addedCount === 0) {
      return;
    }

    metadata.unavailableTools = items;
    context.helpers.updateTurnHistoryProcess(state, (current) => ({
      hasProcess: true,
      unavailableToolCount: Number(current.unavailableToolCount || 0) + addedCount,
      processMessageCount: Number(current.processMessageCount || 0) + addedCount,
    }));

    touchStreamingMessage(message);
    context.helpers.persistStreamingMessageDraft(state, message);
    applied = true;
  });

  return applied;
};

export const handleToolsEndEvent = (
  parsed: StreamEventPayload,
  context: ToolStreamContext,
): boolean => {
  const resultsArray = extractToolResultsFromEndPayload(parsed.data);

  context.set((state) => {
    const message = context.helpers.ensureStreamingMessage(state);
    if (!message) {
      return;
    }

    applyToolEndResultsToMessage(message, resultsArray);
    touchStreamingMessage(message);
    context.helpers.persistStreamingMessageDraft(state, message);
  });

  return true;
};

export const handleToolsStreamEvent = (
  parsed: StreamEventPayload,
  context: ToolStreamContext,
): {
  handled: boolean;
  openedPanel: boolean;
} => {
  const data = parsed.data as RawToolResultPayload;
  const taskBoardUpdated = extractTaskBoardUpdatedEvent(data);
  if (taskBoardUpdated) {
    context.set((state) => {
      const prev = state.sessionChatState?.[taskBoardUpdated.sessionId];
      if (!prev) {
        return;
      }
      state.sessionChatState[taskBoardUpdated.sessionId] = {
        ...prev,
        runtimeContextRefreshNonce: (prev.runtimeContextRefreshNonce || 0) + 1,
      };
    });
    return { handled: true, openedPanel: false };
  }

  const reviewPanel = extractTaskReviewPanelFromToolStream(
    data,
    context.currentSessionId,
    context.conversationTurnId,
  );
  if (reviewPanel) {
    context.set((state) => {
      upsertTaskReviewPanelState(state, reviewPanel);

      const message = context.helpers.ensureStreamingMessage(state);
      if (!message) {
        return;
      }
      markToolCallAsWaitingForPanel(message, data, 'Waiting for task confirmation...');
      touchStreamingMessage(message);
      context.helpers.persistStreamingMessageDraft(state, message);
    });
    return { handled: true, openedPanel: true };
  }

  const uiPromptPanel = extractUiPromptPanelFromToolStream(
    data,
    context.currentSessionId,
    context.conversationTurnId,
  );
  if (uiPromptPanel) {
    context.set((state) => {
      upsertUiPromptPanelState(state, uiPromptPanel);

      const message = context.helpers.ensureStreamingMessage(state);
      if (!message) {
        return;
      }
      markToolCallAsWaitingForPanel(message, data, 'Waiting for UI prompt response...');
      touchStreamingMessage(message);
      context.helpers.persistStreamingMessageDraft(state, message);
    });
    return { handled: true, openedPanel: true };
  }

  let applied = false;
  context.set((state) => {
    const message = context.helpers.ensureStreamingMessage(state);
    if (!message) {
      return;
    }

    const updated = applyToolStreamDataToMessage(message, data);
    if (!updated) {
      return;
    }

    touchStreamingMessage(message);
    context.helpers.persistStreamingMessageDraft(state, message);
    applied = true;
  });

  return { handled: applied, openedPanel: false };
};

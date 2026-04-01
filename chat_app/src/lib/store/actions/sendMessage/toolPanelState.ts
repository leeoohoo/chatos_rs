import type { ChatStoreDraft, TaskReviewPanelState, UiPromptPanelState } from '../../types';
import { extractToolCallIdFromStreamData } from './toolEvents';
import {
  ensureStreamingMetadata,
  ensureStreamingToolCalls,
  type StreamingMessage,
} from './types';

export const upsertTaskReviewPanelState = (
  state: ChatStoreDraft,
  reviewPanel: TaskReviewPanelState,
) => {
  const sessionId = reviewPanel.sessionId;
  const panels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
    ? state.taskReviewPanelsBySession[sessionId]
    : [];
  const index = panels.findIndex((item) => item.reviewId === reviewPanel.reviewId);
  if (index >= 0) {
    panels[index] = reviewPanel;
  } else {
    panels.push(reviewPanel);
  }

  state.taskReviewPanelsBySession[sessionId] = panels;
  if (state.currentSessionId === sessionId) {
    state.taskReviewPanel = panels[0] || reviewPanel;
  }
};

export const upsertUiPromptPanelState = (
  state: ChatStoreDraft,
  panel: UiPromptPanelState,
) => {
  const sessionId = panel.sessionId;
  const panels = Array.isArray(state.uiPromptPanelsBySession?.[sessionId])
    ? state.uiPromptPanelsBySession[sessionId]
    : [];
  const index = panels.findIndex((item) => item.promptId === panel.promptId);
  if (index >= 0) {
    panels[index] = panel;
  } else {
    panels.push(panel);
  }

  state.uiPromptPanelsBySession[sessionId] = panels;
  if (state.currentSessionId === sessionId) {
    state.uiPromptPanel = panels[0] || panel;
  }
};

export const markToolCallAsWaitingForPanel = (
  message: StreamingMessage,
  streamData: unknown,
  waitingResult: string,
) => {
  const toolCalls = ensureStreamingToolCalls(ensureStreamingMetadata(message));

  const toolCallId = extractToolCallIdFromStreamData(streamData);
  if (!toolCallId) {
    return;
  }

  const toolCall = toolCalls.find((tc) => tc.id === toolCallId);
  if (!toolCall) {
    return;
  }

  toolCall.result = waitingResult;
  toolCall.completed = false;
};

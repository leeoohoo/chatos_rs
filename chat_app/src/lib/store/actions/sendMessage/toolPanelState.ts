import { extractToolCallIdFromStreamData } from './toolEvents';

export const upsertTaskReviewPanelState = (state: any, reviewPanel: any) => {
  const sessionId = reviewPanel.sessionId;
  const panels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
    ? state.taskReviewPanelsBySession[sessionId]
    : [];
  const index = panels.findIndex((item: any) => item.reviewId === reviewPanel.reviewId);
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

export const upsertUiPromptPanelState = (state: any, panel: any) => {
  const sessionId = panel.sessionId;
  const panels = Array.isArray(state.uiPromptPanelsBySession?.[sessionId])
    ? state.uiPromptPanelsBySession[sessionId]
    : [];
  const index = panels.findIndex((item: any) => item.promptId === panel.promptId);
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
  message: any,
  streamData: any,
  waitingResult: string,
) => {
  if (!message?.metadata?.toolCalls) {
    return;
  }

  const toolCallId = extractToolCallIdFromStreamData(streamData);
  if (!toolCallId) {
    return;
  }

  const toolCall = message.metadata.toolCalls.find((tc: any) => tc.id === toolCallId);
  if (!toolCall) {
    return;
  }

  toolCall.result = waitingResult;
  toolCall.completed = false;
};

import type {
  ChatState,
  ChatStoreDraft,
  ChatStoreSet,
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../types';

interface RuntimePanelActionsDeps {
  set: ChatStoreSet;
}

export function createRuntimePanelActions({ set }: RuntimePanelActionsDeps) {
  return {
    openTurnProcessViewer: (
      sessionId: string,
      userMessageId: string,
      options?: { turnId?: string | null },
    ) => {
      const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
      const normalizedUserMessageId = typeof userMessageId === 'string' ? userMessageId.trim() : '';
      const normalizedTurnId = typeof options?.turnId === 'string'
        ? options.turnId.trim()
        : '';
      if (!normalizedSessionId || !normalizedUserMessageId) {
        return;
      }
      set((state: ChatStoreDraft) => {
        state.turnProcessViewer = {
          open: true,
          sessionId: normalizedSessionId,
          userMessageId: normalizedUserMessageId,
          turnId: normalizedTurnId || null,
        };
      });
    },
    closeTurnProcessViewer: () => {
      set((state: ChatStoreDraft) => {
        state.turnProcessViewer = {
          open: false,
          sessionId: null,
          userMessageId: null,
          turnId: null,
        };
      });
    },
    setTaskReviewPanel: (panel: ChatState['taskReviewPanel']) => {
      set((state: ChatStoreDraft) => {
        state.taskReviewPanel = panel;
      });
    },
    upsertTaskReviewPanel: (panel: TaskReviewPanelState) => {
      if (!panel || !panel.reviewId || !panel.sessionId) {
        return;
      }
      set((state: ChatStoreDraft) => {
        const sessionId = panel.sessionId;
        const panels = Array.isArray(state.taskReviewPanelsBySession?.[sessionId])
          ? state.taskReviewPanelsBySession[sessionId]
          : [];
        const index = panels.findIndex((item) => item.reviewId === panel.reviewId);
        if (index >= 0) {
          panels[index] = panel;
        } else {
          panels.push(panel);
        }
        state.taskReviewPanelsBySession[sessionId] = panels;
        if (state.currentSessionId === sessionId) {
          state.taskReviewPanel = panels[0] || panel;
        }
      });
    },
    removeTaskReviewPanel: (reviewId: string, sessionId?: string) => {
      if (!reviewId) {
        return;
      }
      set((state: ChatStoreDraft) => {
        const candidates = sessionId
          ? [sessionId]
          : Object.keys(state.taskReviewPanelsBySession || {});
        for (const sid of candidates) {
          const panels = state.taskReviewPanelsBySession?.[sid];
          if (!Array.isArray(panels) || panels.length === 0) {
            continue;
          }
          const nextPanels = panels.filter((item) => item.reviewId !== reviewId);
          if (nextPanels.length > 0) {
            state.taskReviewPanelsBySession[sid] = nextPanels;
          } else {
            delete state.taskReviewPanelsBySession[sid];
          }
          if (state.currentSessionId === sid) {
            state.taskReviewPanel = nextPanels[0] || null;
          }
          break;
        }
      });
    },
    setUiPromptPanel: (panel: ChatState['uiPromptPanel']) => {
      set((state: ChatStoreDraft) => {
        state.uiPromptPanel = panel;
      });
    },
    upsertUiPromptPanel: (panel: UiPromptPanelState) => {
      if (!panel || !panel.promptId || !panel.sessionId) {
        return;
      }
      set((state: ChatStoreDraft) => {
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
      });
    },
    removeUiPromptPanel: (promptId: string, sessionId?: string) => {
      if (!promptId) {
        return;
      }
      set((state: ChatStoreDraft) => {
        const candidates = sessionId
          ? [sessionId]
          : Object.keys(state.uiPromptPanelsBySession || {});
        for (const sid of candidates) {
          const panels = state.uiPromptPanelsBySession?.[sid];
          if (!Array.isArray(panels) || panels.length === 0) {
            continue;
          }
          const nextPanels = panels.filter((item) => item.promptId !== promptId);
          if (nextPanels.length > 0) {
            state.uiPromptPanelsBySession[sid] = nextPanels;
          } else {
            delete state.uiPromptPanelsBySession[sid];
          }
          if (state.currentSessionId === sid) {
            state.uiPromptPanel = nextPanels[0] || null;
          }
          break;
        }
      });
    },
  };
}

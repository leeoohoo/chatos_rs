import type {
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';

export const syncUiPromptPanelsSnapshot = ({
  sessionId,
  panels,
  existingPanels,
  upsertUiPromptPanel,
  removeUiPromptPanel,
}: {
  sessionId: string;
  panels: UiPromptPanelState[];
  existingPanels?: UiPromptPanelState[] | null;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}): void => {
  if (!sessionId) {
    return;
  }
  const nextPromptIds = new Set(
    (panels || [])
      .map((panel) => String(panel?.promptId || '').trim())
      .filter((promptId) => promptId.length > 0),
  );

  (Array.isArray(existingPanels) ? existingPanels : []).forEach((panel) => {
    const promptId = String(panel?.promptId || '').trim();
    if (!promptId || nextPromptIds.has(promptId)) {
      return;
    }
    removeUiPromptPanel(promptId, sessionId);
  });

  (panels || []).forEach((panel) => {
    upsertUiPromptPanel(panel);
  });
};

export const syncTaskReviewPanelsSnapshot = ({
  sessionId,
  panels,
  existingPanels,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
}: {
  sessionId: string;
  panels: TaskReviewPanelState[];
  existingPanels?: TaskReviewPanelState[] | null;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
}): void => {
  if (!sessionId) {
    return;
  }
  const nextReviewIds = new Set(
    (panels || [])
      .map((panel) => String(panel?.reviewId || '').trim())
      .filter((reviewId) => reviewId.length > 0),
  );

  (Array.isArray(existingPanels) ? existingPanels : []).forEach((panel) => {
    const reviewId = String(panel?.reviewId || '').trim();
    if (!reviewId || nextReviewIds.has(reviewId)) {
      return;
    }
    removeTaskReviewPanel(reviewId, sessionId);
  });

  (panels || []).forEach((panel) => {
    upsertTaskReviewPanel(panel);
  });
};

export const pickFirstSessionPanel = <T,>(
  panelsBySession: Record<string, T[] | undefined> | undefined,
  sessionId: string | null | undefined,
): T | null => {
  const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
  if (!normalizedSessionId) {
    return null;
  }
  const panels = panelsBySession?.[normalizedSessionId];
  if (!Array.isArray(panels) || panels.length === 0) {
    return null;
  }
  return panels[0] || null;
};

export const pickSessionScopedState = <T,>(
  stateBySession: Record<string, T | undefined> | undefined,
  sessionId: string | null | undefined,
): T | undefined => {
  const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
  if (!normalizedSessionId) {
    return undefined;
  }
  return stateBySession?.[normalizedSessionId];
};

import { useEffect, useRef } from 'react';

import type {
  TaskReviewPanelState,
  UiPromptPanelState,
} from '../../lib/store/types';
import {
  loadPendingTaskReviewPanels,
  peekPendingTaskReviewCacheEntry,
} from './pendingTaskReviewCache';
import {
  loadPendingUiPromptPanels,
  peekPendingUiPromptCacheEntry,
} from './pendingUiPromptCache';
import {
  syncTaskReviewPanelsSnapshot,
  syncUiPromptPanelsSnapshot,
} from './panelStateSync';
import { syncPendingPanelsFromCacheOrLoad } from './pendingPanelSync';
import {
  beginSessionLoadRequest,
  isLoadRequestCurrent,
} from './sessionLoadGuard';
import type { SessionWorkbarApiClient } from './useSessionWorkbarPanels.types';

interface UsePendingWorkbarPanelsArgs {
  apiClient: SessionWorkbarApiClient;
  enabled: boolean;
  sessionId: string | null;
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[] | undefined>;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[] | undefined>;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
}

export const usePendingWorkbarPanels = ({
  apiClient,
  enabled,
  sessionId,
  taskReviewPanelsBySession,
  uiPromptPanelsBySession,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
}: UsePendingWorkbarPanelsArgs) => {
  const pendingTaskReviewLoadSeqRef = useRef(0);
  const pendingUiPromptLoadSeqRef = useRef(0);

  const applyTaskReviewPanels = (targetSessionId: string, panels: TaskReviewPanelState[]) => {
    syncTaskReviewPanelsSnapshot({
      sessionId: targetSessionId,
      panels,
      existingPanels: taskReviewPanelsBySession?.[targetSessionId],
      upsertTaskReviewPanel,
      removeTaskReviewPanel,
    });
  };

  const applyUiPromptPanels = (targetSessionId: string, panels: UiPromptPanelState[]) => {
    syncUiPromptPanelsSnapshot({
      sessionId: targetSessionId,
      panels,
      existingPanels: uiPromptPanelsBySession?.[targetSessionId],
      upsertUiPromptPanel,
      removeUiPromptPanel,
    });
  };

  useEffect(() => {
    if (!enabled || !sessionId) {
      return;
    }

    const cachedEntry = peekPendingTaskReviewCacheEntry(apiClient, sessionId);
    const requestSeq = beginSessionLoadRequest(pendingTaskReviewLoadSeqRef);
    return syncPendingPanelsFromCacheOrLoad({
      cachedEntry,
      loadPanels: () => loadPendingTaskReviewPanels(apiClient, sessionId, { limit: 50 }),
      applyPanels: (panels) => {
        applyTaskReviewPanels(sessionId, panels);
      },
      shouldApply: () => isLoadRequestCurrent(pendingTaskReviewLoadSeqRef, requestSeq),
    });
  }, [
    apiClient,
    applyTaskReviewPanels,
    enabled,
    removeTaskReviewPanel,
    sessionId,
    taskReviewPanelsBySession,
    upsertTaskReviewPanel,
  ]);

  useEffect(() => {
    if (!enabled || !sessionId) {
      return;
    }

    const cachedEntry = peekPendingUiPromptCacheEntry(apiClient, sessionId);
    const requestSeq = beginSessionLoadRequest(pendingUiPromptLoadSeqRef);
    return syncPendingPanelsFromCacheOrLoad({
      cachedEntry,
      loadPanels: () => loadPendingUiPromptPanels(apiClient, sessionId, { limit: 50 }),
      applyPanels: (panels) => {
        applyUiPromptPanels(sessionId, panels);
      },
      shouldApply: () => isLoadRequestCurrent(pendingUiPromptLoadSeqRef, requestSeq),
    });
  }, [
    apiClient,
    applyUiPromptPanels,
    enabled,
    removeUiPromptPanel,
    sessionId,
    uiPromptPanelsBySession,
    upsertUiPromptPanel,
  ]);
};

import { useEffect, useMemo, useRef } from 'react';

import type ApiClient from '../../lib/api/client';
import type { UiPromptPanelState } from '../../lib/store/types';
import type { Session } from '../../types';
import { toUiPromptPanelFromRecord } from './helpers';

interface UseChatSessionEffectsParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  currentSessionIdForUiPrompts: string | null;
  uiPromptHistoryOpen: boolean;
  summaryPaneSessionId: string | null;
  loadProjects: () => Promise<unknown>;
  loadAiModelConfigs: () => Promise<void>;
  loadAgents: () => Promise<void>;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<unknown>;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateUiPromptHistoryFromCache: (sessionId: string) => void;
  resetUiPromptHistoryState: () => void;
  cancelPendingUiPromptHistoryLoad: () => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  resetAllWorkbarState: () => void;
  resetHistoryWorkbarState: () => void;
  setUiPromptHistoryOpen: (value: boolean) => void;
}

export const useChatSessionEffects = ({
  apiClient,
  activePanel,
  currentSession,
  currentSessionIdForUiPrompts,
  uiPromptHistoryOpen,
  summaryPaneSessionId,
  loadProjects,
  loadAiModelConfigs,
  loadAgents,
  loadContactMemoryContext,
  resetMemoryState,
  cancelPendingMemoryLoad,
  loadUiPromptHistory,
  hydrateUiPromptHistoryFromCache,
  resetUiPromptHistoryState,
  cancelPendingUiPromptHistoryLoad,
  upsertUiPromptPanel,
  resetAllWorkbarState,
  resetHistoryWorkbarState,
  setUiPromptHistoryOpen,
}: UseChatSessionEffectsParams) => {
  const didInitRef = useRef(false);
  const lastHydratedChatSessionRef = useRef<string | null>(null);

  const sessionSummaryPaneVisible = useMemo(
    () => Boolean(activePanel === 'chat' && currentSession && summaryPaneSessionId === currentSession.id),
    [activePanel, currentSession, summaryPaneSessionId],
  );

  useEffect(() => {
    if (!currentSessionIdForUiPrompts || activePanel !== 'chat') {
      return;
    }

    let cancelled = false;
    void apiClient
      .getPendingUiPrompts(currentSessionIdForUiPrompts, { limit: 50 })
      .then((records) => {
        if (cancelled || !Array.isArray(records)) {
          return;
        }
        records.forEach((record) => {
          const panel = toUiPromptPanelFromRecord(record);
          if (panel) {
            upsertUiPromptPanel(panel);
          }
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [activePanel, apiClient, currentSessionIdForUiPrompts, upsertUiPromptPanel]);

  useEffect(() => {
    if (didInitRef.current) {
      return;
    }
    didInitRef.current = true;

    void loadProjects();
    void loadAiModelConfigs();
    void loadAgents();
  }, [loadProjects, loadAiModelConfigs, loadAgents]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      lastHydratedChatSessionRef.current = null;
      resetAllWorkbarState();
      resetMemoryState();
      resetUiPromptHistoryState();
      setUiPromptHistoryOpen(false);
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentSession.id;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentSession.id;
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      resetHistoryWorkbarState();
      resetMemoryState();
      hydrateUiPromptHistoryFromCache(currentSession.id);
    }

    if (sessionSummaryPaneVisible) {
      void loadContactMemoryContext(currentSession.id);
    }
    if (uiPromptHistoryOpen) {
      void loadUiPromptHistory(currentSession.id);
    }
  }, [
    activePanel,
    cancelPendingMemoryLoad,
    cancelPendingUiPromptHistoryLoad,
    currentSession,
    hydrateUiPromptHistoryFromCache,
    loadContactMemoryContext,
    loadUiPromptHistory,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    resetMemoryState,
    resetUiPromptHistoryState,
    sessionSummaryPaneVisible,
    setUiPromptHistoryOpen,
    uiPromptHistoryOpen,
  ]);

  return {
    sessionSummaryPaneVisible,
  };
};

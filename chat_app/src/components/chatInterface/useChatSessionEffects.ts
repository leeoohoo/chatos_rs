import { useEffect, useMemo, useRef } from 'react';

import type { Session } from '../../types';

interface UseChatSessionEffectsParams {
  activePanel: string;
  currentSession: Session | null;
  uiPromptHistoryOpen: boolean;
  summaryPaneSessionId: string | null;
  setTaskHistoryOpen?: (value: boolean) => void;
  loadProjects: () => Promise<unknown>;
  loadAiModelConfigs: () => Promise<void>;
  loadAgents: () => Promise<void>;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateContactMemoryContextFromCache: (sessionId: string) => void;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateUiPromptHistoryFromCache: (sessionId: string) => void;
  resetUiPromptHistoryState: () => void;
  cancelPendingUiPromptHistoryLoad: () => void;
  resetAllWorkbarState: () => void;
  resetHistoryWorkbarState: () => void;
  setUiPromptHistoryOpen: (value: boolean) => void;
}

export const useChatSessionEffects = ({
  activePanel,
  currentSession,
  uiPromptHistoryOpen,
  summaryPaneSessionId,
  setTaskHistoryOpen,
  loadProjects,
  loadAiModelConfigs,
  loadAgents,
  loadContactMemoryContext,
  hydrateContactMemoryContextFromCache,
  resetMemoryState,
  cancelPendingMemoryLoad,
  loadUiPromptHistory,
  hydrateUiPromptHistoryFromCache,
  resetUiPromptHistoryState,
  cancelPendingUiPromptHistoryLoad,
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
      setTaskHistoryOpen?.(false);
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentSession.id;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentSession.id;
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      setTaskHistoryOpen?.(false);
      resetHistoryWorkbarState();
      resetMemoryState();
      hydrateUiPromptHistoryFromCache(currentSession.id);
    }

    if (sessionSummaryPaneVisible) {
      hydrateContactMemoryContextFromCache(currentSession.id);
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
    hydrateContactMemoryContextFromCache,
    hydrateUiPromptHistoryFromCache,
    loadContactMemoryContext,
    loadUiPromptHistory,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    resetMemoryState,
    resetUiPromptHistoryState,
    setTaskHistoryOpen,
    sessionSummaryPaneVisible,
    setUiPromptHistoryOpen,
    uiPromptHistoryOpen,
  ]);

  return {
    sessionSummaryPaneVisible,
  };
};

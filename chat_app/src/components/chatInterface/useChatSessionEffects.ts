// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useRef } from 'react';

import type { Session } from '../../types';

interface UseChatSessionEffectsParams {
  activePanel: string;
  currentSession: Session | null;
  summaryPaneSessionId: string | null;
  loadProjects: () => Promise<unknown>;
  loadAiModelConfigs: () => Promise<void>;
  loadAgents: () => Promise<void>;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateContactMemoryContextFromCache: (sessionId: string) => void;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
}

export const useChatSessionEffects = ({
  activePanel,
  currentSession,
  summaryPaneSessionId,
  loadProjects,
  loadAiModelConfigs,
  loadAgents,
  loadContactMemoryContext,
  hydrateContactMemoryContextFromCache,
  resetMemoryState,
  cancelPendingMemoryLoad,
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
      lastHydratedChatSessionRef.current = null;
      resetMemoryState();
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentSession.id;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentSession.id;
      cancelPendingMemoryLoad();
      resetMemoryState();
    }

    if (sessionSummaryPaneVisible) {
      hydrateContactMemoryContextFromCache(currentSession.id);
      void loadContactMemoryContext(currentSession.id);
    }
  }, [
    activePanel,
    cancelPendingMemoryLoad,
    currentSession,
    hydrateContactMemoryContextFromCache,
    loadContactMemoryContext,
    resetMemoryState,
    sessionSummaryPaneVisible,
  ]);

  return {
    sessionSummaryPaneVisible,
  };
};

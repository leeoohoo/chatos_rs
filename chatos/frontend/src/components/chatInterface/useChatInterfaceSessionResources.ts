// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { SessionSummariesListResponse } from '../../lib/api/client/types';
import type { Project, Session } from '../../types';
import { useContactMemoryContext } from './useContactMemoryContext';
import { useContactProjectScope } from './useContactProjectScope';

interface SessionResourcesApiClient {
  getContactProjects: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getConversationSummaries: (
    sessionId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<SessionSummariesListResponse>;
  getContactAgentRecalls: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
}

interface UseChatInterfaceSessionResourcesParams {
  apiClient: SessionResourcesApiClient;
  currentSession: Session | null;
  currentContactId: string;
  currentProject: Project | null;
  projects: Project[];
}

export const useChatInterfaceSessionResources = ({
  apiClient,
  currentSession,
  currentContactId,
  currentProject,
  projects,
}: UseChatInterfaceSessionResourcesParams) => {
  const {
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
  } = useContactProjectScope({
    apiClient,
    currentSession,
    currentContactId,
    projects,
  });

  const {
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    loadSessionMemorySummaries,
    applyRealtimeSessionMemorySummaries,
    markContactMemoryContextStale,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
  } = useContactMemoryContext({
    apiClient,
    currentSessionId: currentSession?.id || null,
    currentContactId,
    currentProjectIdForMemory,
  });

  return {
    currentProject,
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    loadSessionMemorySummaries,
    applyRealtimeSessionMemorySummaries,
    markContactMemoryContextStale,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
  };
};

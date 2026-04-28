import { useCallback, useEffect, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { TurnRuntimeSnapshotLookupResponse } from '../../lib/api/client/types';
import type { Session } from '../../types';

interface UseRuntimeContextStateParams {
  apiClient: ApiClient;
  currentSession: Session | null;
  runtimeContextRefreshNonce: number;
}

export const useRuntimeContextState = ({
  apiClient,
  currentSession,
  runtimeContextRefreshNonce,
}: UseRuntimeContextStateParams) => {
  const [runtimeContextOpen, setRuntimeContextOpen] = useState(false);
  const [runtimeContextSessionId, setRuntimeContextSessionId] = useState<string | null>(null);
  const [runtimeContextData, setRuntimeContextData] =
    useState<TurnRuntimeSnapshotLookupResponse | null>(null);
  const [runtimeContextLoading, setRuntimeContextLoading] = useState(false);
  const [runtimeContextError, setRuntimeContextError] = useState<string | null>(null);

  const loadLatestRuntimeContext = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextLoading(true);
    setRuntimeContextError(null);
    try {
      const payload = await apiClient.getConversationLatestTurnRuntimeContext(sessionId);
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context:', error);
      setRuntimeContextError(error instanceof Error ? error.message : '加载上下文失败');
    } finally {
      setRuntimeContextLoading(false);
    }
  }, [apiClient]);

  const handleOpenRuntimeContext = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextOpen(true);
    setRuntimeContextSessionId(sessionId);
    setRuntimeContextData(null);
    void loadLatestRuntimeContext(sessionId);
  }, [loadLatestRuntimeContext]);

  const handleRefreshRuntimeContext = useCallback(() => {
    if (!runtimeContextSessionId) {
      return;
    }
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [loadLatestRuntimeContext, runtimeContextSessionId]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      return;
    }
    if (currentSession?.id !== runtimeContextSessionId) {
      return;
    }
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [
    currentSession?.id,
    loadLatestRuntimeContext,
    runtimeContextOpen,
    runtimeContextRefreshNonce,
    runtimeContextSessionId,
  ]);

  return {
    runtimeContextOpen,
    setRuntimeContextOpen,
    runtimeContextSessionId,
    runtimeContextData,
    runtimeContextLoading,
    runtimeContextError,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  };
};

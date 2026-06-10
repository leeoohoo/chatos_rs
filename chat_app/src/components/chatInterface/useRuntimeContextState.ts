import { useCallback, useEffect, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { TurnRuntimeSnapshotLookupResponse } from '../../lib/api/client/types';
import {
  getCachedRuntimeContextData,
  loadRuntimeContextSnapshot,
  markRuntimeContextStale,
} from '../../lib/runtimeContext/cache';
import type { Session } from '../../types';
import { useI18n } from '../../i18n/I18nProvider';

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
  const { t } = useI18n();
  const [runtimeContextOpen, setRuntimeContextOpen] = useState(false);
  const [runtimeContextSessionId, setRuntimeContextSessionId] = useState<string | null>(null);
  const [runtimeContextData, setRuntimeContextData] =
    useState<TurnRuntimeSnapshotLookupResponse | null>(null);
  const [runtimeContextLoading, setRuntimeContextLoading] = useState(false);
  const [runtimeContextError, setRuntimeContextError] = useState<string | null>(null);
  const latestSessionIdRef = useRef<string | null>(null);
  const refreshNonceRef = useRef(runtimeContextRefreshNonce);
  const lastRefreshSignatureRef = useRef<string | null>(null);

  refreshNonceRef.current = runtimeContextRefreshNonce;

  const loadLatestRuntimeContext = useCallback(async (
    sessionId: string,
    options?: { force?: boolean; silent?: boolean },
  ) => {
    if (!sessionId) {
      return;
    }
    latestSessionIdRef.current = sessionId;
    if (!options?.silent) {
      setRuntimeContextLoading(true);
    }
    setRuntimeContextError(null);
    try {
      const payload = await loadRuntimeContextSnapshot(apiClient, sessionId, options);
      if (latestSessionIdRef.current !== sessionId) {
        return;
      }
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context:', error);
      if (latestSessionIdRef.current === sessionId) {
        setRuntimeContextError(error instanceof Error ? error.message : t('runtimeContext.loadFailed'));
      }
    } finally {
      if (latestSessionIdRef.current === sessionId && !options?.silent) {
        setRuntimeContextLoading(false);
      }
    }
  }, [apiClient, t]);

  const handleOpenRuntimeContext = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextOpen(true);
    setRuntimeContextSessionId(sessionId);
    setRuntimeContextData(getCachedRuntimeContextData(apiClient, sessionId));
  }, [apiClient]);

  const handleRefreshRuntimeContext = useCallback(() => {
    if (!runtimeContextSessionId) {
      return;
    }
    markRuntimeContextStale(apiClient, runtimeContextSessionId);
    void loadLatestRuntimeContext(runtimeContextSessionId, { force: true });
  }, [apiClient, loadLatestRuntimeContext, runtimeContextSessionId]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      lastRefreshSignatureRef.current = null;
      return;
    }
    if (currentSession?.id !== runtimeContextSessionId) {
      return;
    }
    setRuntimeContextData(getCachedRuntimeContextData(apiClient, runtimeContextSessionId));
    lastRefreshSignatureRef.current = `${runtimeContextSessionId}:${refreshNonceRef.current}`;
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [
    apiClient,
    currentSession?.id,
    loadLatestRuntimeContext,
    runtimeContextOpen,
    runtimeContextSessionId,
  ]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      return;
    }
    if (currentSession?.id !== runtimeContextSessionId) {
      return;
    }
    const signature = `${runtimeContextSessionId}:${runtimeContextRefreshNonce}`;
    if (lastRefreshSignatureRef.current === signature) {
      return;
    }
    lastRefreshSignatureRef.current = signature;
    markRuntimeContextStale(apiClient, runtimeContextSessionId);
    setRuntimeContextData(getCachedRuntimeContextData(apiClient, runtimeContextSessionId));
    void loadLatestRuntimeContext(runtimeContextSessionId, { silent: true });
  }, [
    apiClient,
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

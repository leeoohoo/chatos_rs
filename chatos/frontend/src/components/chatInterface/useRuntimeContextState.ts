// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  const [runtimeContextTurnId, setRuntimeContextTurnId] = useState<string | null>(null);
  const [runtimeContextData, setRuntimeContextData] =
    useState<TurnRuntimeSnapshotLookupResponse | null>(null);
  const [runtimeContextLoading, setRuntimeContextLoading] = useState(false);
  const [runtimeContextError, setRuntimeContextError] = useState<string | null>(null);
  const latestRequestKeyRef = useRef<string | null>(null);
  const refreshNonceRef = useRef(runtimeContextRefreshNonce);
  const lastRefreshSignatureRef = useRef<string | null>(null);

  refreshNonceRef.current = runtimeContextRefreshNonce;

  const loadSelectedRuntimeContext = useCallback(async (
    sessionId: string,
    turnId: string | null,
    options?: { force?: boolean; silent?: boolean },
  ) => {
    if (!sessionId) {
      return;
    }
    const requestKey = `${sessionId}\u0000${turnId || ''}`;
    latestRequestKeyRef.current = requestKey;
    if (!options?.silent) {
      setRuntimeContextLoading(true);
    }
    setRuntimeContextError(null);
    try {
      const payload = turnId
        ? await apiClient.getConversationTurnRuntimeContextByTurn(sessionId, turnId)
        : await loadRuntimeContextSnapshot(apiClient, sessionId, options);
      if (latestRequestKeyRef.current !== requestKey) {
        return;
      }
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context:', error);
      if (latestRequestKeyRef.current === requestKey) {
        setRuntimeContextError(error instanceof Error ? error.message : t('runtimeContext.loadFailed'));
      }
    } finally {
      if (latestRequestKeyRef.current === requestKey && !options?.silent) {
        setRuntimeContextLoading(false);
      }
    }
  }, [apiClient, t]);

  const handleOpenRuntimeContext = useCallback((sessionId: string, turnId?: string | null) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextOpen(true);
    setRuntimeContextSessionId(sessionId);
    const normalizedTurnId = typeof turnId === 'string' ? turnId.trim() : '';
    setRuntimeContextTurnId(normalizedTurnId || null);
    setRuntimeContextData(normalizedTurnId
      ? null
      : getCachedRuntimeContextData(apiClient, sessionId));
  }, [apiClient]);

  const handleRefreshRuntimeContext = useCallback(() => {
    if (!runtimeContextSessionId) {
      return;
    }
    if (!runtimeContextTurnId) {
      markRuntimeContextStale(apiClient, runtimeContextSessionId);
    }
    void loadSelectedRuntimeContext(
      runtimeContextSessionId,
      runtimeContextTurnId,
      { force: true },
    );
  }, [
    apiClient,
    loadSelectedRuntimeContext,
    runtimeContextSessionId,
    runtimeContextTurnId,
  ]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      lastRefreshSignatureRef.current = null;
      return;
    }
    if (currentSession?.id !== runtimeContextSessionId) {
      return;
    }
    setRuntimeContextData(runtimeContextTurnId
      ? null
      : getCachedRuntimeContextData(apiClient, runtimeContextSessionId));
    lastRefreshSignatureRef.current = `${runtimeContextSessionId}:${runtimeContextTurnId || ''}:${refreshNonceRef.current}`;
    void loadSelectedRuntimeContext(runtimeContextSessionId, runtimeContextTurnId);
  }, [
    apiClient,
    currentSession?.id,
    loadSelectedRuntimeContext,
    runtimeContextOpen,
    runtimeContextSessionId,
    runtimeContextTurnId,
  ]);

  useEffect(() => {
    if (!runtimeContextOpen || !runtimeContextSessionId) {
      return;
    }
    if (currentSession?.id !== runtimeContextSessionId) {
      return;
    }
    const signature = `${runtimeContextSessionId}:${runtimeContextTurnId || ''}:${runtimeContextRefreshNonce}`;
    if (lastRefreshSignatureRef.current === signature) {
      return;
    }
    lastRefreshSignatureRef.current = signature;
    if (!runtimeContextTurnId) {
      markRuntimeContextStale(apiClient, runtimeContextSessionId);
      setRuntimeContextData(getCachedRuntimeContextData(apiClient, runtimeContextSessionId));
    }
    void loadSelectedRuntimeContext(
      runtimeContextSessionId,
      runtimeContextTurnId,
      { silent: true },
    );
  }, [
    apiClient,
    currentSession?.id,
    loadSelectedRuntimeContext,
    runtimeContextOpen,
    runtimeContextRefreshNonce,
    runtimeContextSessionId,
    runtimeContextTurnId,
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

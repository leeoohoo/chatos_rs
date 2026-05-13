import { useCallback, useEffect, useRef, useState } from 'react';
import { normalizeUiPromptHistoryItem } from './panelTransforms';
import {
  beginSessionLoadRequest,
  isSessionLoadCurrent,
  isSessionLoadRequestCurrent,
  runGuardedSessionLoad,
} from './sessionLoadGuard';
import type { UiPromptHistoryItem } from './types';
import {
  getUiPromptHistoryInflight,
  markUiPromptHistoryCacheStale,
  peekUiPromptHistoryCacheEntry,
  setUiPromptHistoryCacheEntry,
  setUiPromptHistoryInflight,
} from './uiPromptHistoryCache';

interface UiPromptHistoryApiClient {
  getUiPromptHistory: (
    sessionId: string,
    params?: { limit?: number },
  ) => Promise<unknown[]>;
}

interface UseUiPromptHistoryOptions {
  apiClient: UiPromptHistoryApiClient;
  currentSessionId: string | null;
}

interface UseUiPromptHistoryResult {
  uiPromptHistoryItems: UiPromptHistoryItem[];
  uiPromptHistoryLoading: boolean;
  uiPromptHistoryError: string | null;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<void>;
  markUiPromptHistoryStale: (sessionId: string) => void;
  resetUiPromptHistoryState: () => void;
  hydrateUiPromptHistoryFromCache: (sessionId: string) => void;
  cancelPendingUiPromptHistoryLoad: () => void;
}

export const useUiPromptHistory = ({
  apiClient,
  currentSessionId,
}: UseUiPromptHistoryOptions): UseUiPromptHistoryResult => {
  const [uiPromptHistoryItems, setUiPromptHistoryItems] = useState<UiPromptHistoryItem[]>([]);
  const [uiPromptHistoryLoading, setUiPromptHistoryLoading] = useState(false);
  const [uiPromptHistoryError, setUiPromptHistoryError] = useState<string | null>(null);
  const [uiPromptHistoryLoadedSessionId, setUiPromptHistoryLoadedSessionId] = useState<string | null>(null);
  const uiPromptHistoryLoadSeqRef = useRef(0);
  const uiPromptHistoryStaleSessionsRef = useRef<Set<string>>(new Set());
  const currentSessionIdRef = useRef<string | null>(currentSessionId);

  useEffect(() => {
    currentSessionIdRef.current = currentSessionId;
  }, [currentSessionId]);

  const resetUiPromptHistoryState = useCallback(() => {
    setUiPromptHistoryItems([]);
    setUiPromptHistoryError(null);
    setUiPromptHistoryLoading(false);
    setUiPromptHistoryLoadedSessionId(null);
  }, []);

  const hydrateUiPromptHistoryFromCache = useCallback((sessionId: string) => {
    if (!sessionId) {
      resetUiPromptHistoryState();
      return;
    }
    const cached = peekUiPromptHistoryCacheEntry(apiClient, sessionId);
    setUiPromptHistoryItems(cached ? [...cached.items] : []);
    setUiPromptHistoryError(null);
    setUiPromptHistoryLoadedSessionId(cached ? sessionId : null);
    setUiPromptHistoryLoading(false);
  }, [apiClient, resetUiPromptHistoryState]);

  const cancelPendingUiPromptHistoryLoad = useCallback(() => {
    uiPromptHistoryLoadSeqRef.current += 1;
  }, []);

  const markUiPromptHistoryStale = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    uiPromptHistoryStaleSessionsRef.current.add(sessionId);
    markUiPromptHistoryCacheStale(apiClient, sessionId);
  }, [apiClient]);

  const loadUiPromptHistory = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      resetUiPromptHistoryState();
      return;
    }

    const cachedEntry = peekUiPromptHistoryCacheEntry(apiClient, sessionId);
    const cached = cachedEntry?.items || null;
    const isStale = uiPromptHistoryStaleSessionsRef.current.has(sessionId);
    if (
      !force
      && !isStale
      && uiPromptHistoryLoadedSessionId === sessionId
      && uiPromptHistoryItems.length > 0
    ) {
      return;
    }
    if (!force && !isStale && cached) {
      setUiPromptHistoryItems(cached);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(sessionId);
      setUiPromptHistoryLoading(false);
      return;
    }

    const existingInflight = !force
      ? getUiPromptHistoryInflight(apiClient, sessionId)
      : null;
    if (existingInflight) {
      const shouldShowLoading = force || !cached;
      await runGuardedSessionLoad({
        applyResult: (normalized) => {
          uiPromptHistoryStaleSessionsRef.current.delete(sessionId);
          setUiPromptHistoryItems(normalized);
          setUiPromptHistoryLoadedSessionId(sessionId);
        },
        errorMessage: '交互确认记录加载失败',
        load: () => existingInflight,
        setError: setUiPromptHistoryError,
        setLoading: setUiPromptHistoryLoading,
        shouldApply: () => isSessionLoadCurrent({
          currentSessionRef: currentSessionIdRef,
          sessionId,
        }),
        showLoading: shouldShowLoading,
      });
      return;
    }

    const requestSeq = beginSessionLoadRequest(uiPromptHistoryLoadSeqRef);
    const shouldShowLoading = force || !cached;
    await runGuardedSessionLoad({
      applyResult: (normalized) => {
        uiPromptHistoryStaleSessionsRef.current.delete(sessionId);
        setUiPromptHistoryItems(normalized);
        setUiPromptHistoryLoadedSessionId(sessionId);
      },
      errorMessage: '交互确认记录加载失败',
      load: () => {
        const inflight = apiClient.getUiPromptHistory(sessionId, { limit: 200 })
          .then((records) => (
            Array.isArray(records)
              ? records
                  .map((item) => normalizeUiPromptHistoryItem(item))
                  .filter((item): item is UiPromptHistoryItem => item !== null)
              : []
          ))
          .then((normalized) => {
            setUiPromptHistoryCacheEntry(apiClient, sessionId, normalized);
            return normalized;
          })
          .finally(() => {
            setUiPromptHistoryInflight(apiClient, sessionId, null);
          });
        setUiPromptHistoryInflight(apiClient, sessionId, inflight);
        return inflight;
      },
      setError: setUiPromptHistoryError,
      setLoading: setUiPromptHistoryLoading,
      shouldApply: () => isSessionLoadRequestCurrent({
        currentSessionRef: currentSessionIdRef,
        requestSeq,
        requestSeqRef: uiPromptHistoryLoadSeqRef,
        sessionId,
      }),
      showLoading: shouldShowLoading,
    });
  }, [apiClient, resetUiPromptHistoryState, uiPromptHistoryItems.length, uiPromptHistoryLoadedSessionId]);

  return {
    uiPromptHistoryItems,
    uiPromptHistoryLoading,
    uiPromptHistoryError,
    loadUiPromptHistory,
    markUiPromptHistoryStale,
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
  };
};

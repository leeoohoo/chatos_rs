import { useCallback, useEffect, useRef, useState } from 'react';
import { normalizeUiPromptHistoryItem } from './helpers';
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
      if (shouldShowLoading) {
        setUiPromptHistoryLoading(true);
      }
      setUiPromptHistoryError(null);
      try {
        const normalized = await existingInflight;
        if (
          currentSessionIdRef.current !== sessionId
        ) {
          return;
        }
        uiPromptHistoryStaleSessionsRef.current.delete(sessionId);
        setUiPromptHistoryItems(normalized);
        setUiPromptHistoryLoadedSessionId(sessionId);
      } catch (error) {
        if (currentSessionIdRef.current !== sessionId) {
          return;
        }
        setUiPromptHistoryError(error instanceof Error ? error.message : '交互确认记录加载失败');
      } finally {
        if (currentSessionIdRef.current === sessionId) {
          setUiPromptHistoryLoading(false);
        }
      }
      return;
    }

    const requestSeq = uiPromptHistoryLoadSeqRef.current + 1;
    uiPromptHistoryLoadSeqRef.current = requestSeq;
    const shouldShowLoading = force || !cached;
    if (shouldShowLoading) {
      setUiPromptHistoryLoading(true);
    }
    setUiPromptHistoryError(null);
    try {
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
      const normalized = await inflight;
      if (
        uiPromptHistoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }
      uiPromptHistoryStaleSessionsRef.current.delete(sessionId);
      setUiPromptHistoryItems(normalized);
      setUiPromptHistoryLoadedSessionId(sessionId);
    } catch (error) {
      if (
        uiPromptHistoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }
      setUiPromptHistoryError(error instanceof Error ? error.message : '交互确认记录加载失败');
    } finally {
      if (
        uiPromptHistoryLoadSeqRef.current === requestSeq
        && currentSessionIdRef.current === sessionId
      ) {
        setUiPromptHistoryLoading(false);
      }
    }
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

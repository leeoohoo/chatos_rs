import { useCallback, useEffect, useRef, useState } from 'react';
import { normalizeUiPromptHistoryItem } from './helpers';
import type { UiPromptHistoryItem } from './types';

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
  const uiPromptHistoryCacheRef = useRef<Map<string, UiPromptHistoryItem[]>>(new Map());
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
    const cached = uiPromptHistoryCacheRef.current.get(sessionId);
    setUiPromptHistoryItems(cached ? [...cached] : []);
    setUiPromptHistoryError(null);
    setUiPromptHistoryLoadedSessionId(cached ? sessionId : null);
    setUiPromptHistoryLoading(false);
  }, [resetUiPromptHistoryState]);

  const cancelPendingUiPromptHistoryLoad = useCallback(() => {
    uiPromptHistoryLoadSeqRef.current += 1;
  }, []);

  const loadUiPromptHistory = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId) {
      resetUiPromptHistoryState();
      return;
    }

    const cached = uiPromptHistoryCacheRef.current.get(sessionId);
    if (!force && uiPromptHistoryLoadedSessionId === sessionId && uiPromptHistoryItems.length > 0) {
      return;
    }
    if (!force && cached) {
      setUiPromptHistoryItems(cached);
      setUiPromptHistoryError(null);
      setUiPromptHistoryLoadedSessionId(sessionId);
      setUiPromptHistoryLoading(false);
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
      const records = await apiClient.getUiPromptHistory(sessionId, { limit: 200 });
      const normalized = Array.isArray(records)
        ? records
            .map((item) => normalizeUiPromptHistoryItem(item))
            .filter((item): item is UiPromptHistoryItem => item !== null)
        : [];
      uiPromptHistoryCacheRef.current.set(sessionId, normalized);
      if (
        uiPromptHistoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }
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
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
  };
};

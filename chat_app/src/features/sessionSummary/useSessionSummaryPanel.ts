import { useCallback, useEffect, useRef, useState } from 'react';

import { useDialogService } from '../../components/ui/DialogProvider';
import type { SessionSummariesListResponse } from '../../lib/api/client/types';
import type { SessionSummaryItem } from '../../lib/domain/configs';
import {
  applyConversationSummaryItemsSnapshot,
  getCachedConversationSummaryItems,
  loadConversationSummaryItems,
  markConversationSummaryCacheStale,
} from '../../lib/sessionSummaries/cache';
export type { SessionSummaryItem } from '../../lib/domain/configs';

interface SessionSummaryApiClient {
  getConversationSummaries: (
    sessionId: string,
    options?: { limit?: number; offset?: number },
  ) => Promise<SessionSummariesListResponse>;
  deleteConversationSummary: (sessionId: string, summaryId: string) => Promise<{ success?: boolean }>;
  clearConversationSummaries: (sessionId: string) => Promise<{ success?: boolean }>;
}

interface UseSessionSummaryPanelResult {
  summaryPaneSessionId: string | null;
  summaryItems: SessionSummaryItem[];
  summaryLoading: boolean;
  summaryError: string | null;
  clearingSummaries: boolean;
  deletingSummaryId: string | null;
  setSummaryPaneSessionId: (sessionId: string | null) => void;
  setSummaryError: (message: string | null) => void;
  resetSummaryState: () => void;
  loadSessionSummaries: (
    sessionId: string,
    options?: { silent?: boolean; force?: boolean },
  ) => Promise<void>;
  markSessionSummariesStale: (sessionId: string) => void;
  hydrateSessionSummariesFromCache: (sessionId: string) => void;
  cancelPendingSessionSummariesLoad: () => void;
  applyRealtimeSessionSummaries: (
    sessionId: string,
    payload: SessionSummariesListResponse | unknown,
  ) => void;
  openSummaryForSession: (sessionId: string) => Promise<void>;
  deleteSummary: (sessionId: string, summaryId: string) => Promise<void>;
  clearSummaries: (
    sessionId: string,
    options?: { confirmMessage?: string; skipConfirm?: boolean },
  ) => Promise<void>;
}

export const useSessionSummaryPanel = (
  apiClient: SessionSummaryApiClient,
): UseSessionSummaryPanelResult => {
  const { confirm } = useDialogService();
  const [summaryPaneSessionId, setSummaryPaneSessionIdState] = useState<string | null>(null);
  const [summaryItems, setSummaryItems] = useState<SessionSummaryItem[]>([]);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [clearingSummaries, setClearingSummaries] = useState(false);
  const [deletingSummaryId, setDeletingSummaryId] = useState<string | null>(null);
  const [summaryLoadedSessionId, setSummaryLoadedSessionId] = useState<string | null>(null);
  const summaryLoadSeqRef = useRef(0);
  const summaryStaleSessionsRef = useRef<Set<string>>(new Set());
  const currentSummarySessionIdRef = useRef<string | null>(summaryPaneSessionId);

  useEffect(() => {
    currentSummarySessionIdRef.current = summaryPaneSessionId;
  }, [summaryPaneSessionId]);

  const setSummaryPaneSessionId = useCallback((
    value: string | null | ((prev: string | null) => string | null),
  ) => {
    setSummaryPaneSessionIdState((prev) => {
      const nextValue = typeof value === 'function'
        ? value(prev)
        : value;
      currentSummarySessionIdRef.current = nextValue;
      return nextValue;
    });
  }, []);

  const resetSummaryState = useCallback(() => {
    setSummaryItems([]);
    setSummaryError(null);
    setSummaryLoadedSessionId(null);
    setSummaryLoading(false);
  }, []);

  const hydrateSessionSummariesFromCache = useCallback((sessionId: string) => {
    if (!sessionId) {
      resetSummaryState();
      return;
    }
    if (currentSummarySessionIdRef.current !== sessionId) {
      return;
    }
    const cached = getCachedConversationSummaryItems(apiClient, sessionId);
    setSummaryItems(cached ? [...cached] : []);
    setSummaryError(null);
    setSummaryLoadedSessionId(cached ? sessionId : null);
    setSummaryLoading(false);
  }, [apiClient, resetSummaryState]);

  const cancelPendingSessionSummariesLoad = useCallback(() => {
    summaryLoadSeqRef.current += 1;
  }, []);

  const applyRealtimeSessionSummaries = useCallback((
    sessionId: string,
    payload: SessionSummariesListResponse | unknown,
  ) => {
    if (!sessionId) {
      return;
    }
    const normalized = applyConversationSummaryItemsSnapshot(apiClient, sessionId, payload, {
      loadedLimit: 100,
    });
    summaryStaleSessionsRef.current.delete(sessionId);
    if (currentSummarySessionIdRef.current !== sessionId) {
      return;
    }
    setSummaryItems(normalized);
    setSummaryLoadedSessionId(sessionId);
    setSummaryError(null);
    setSummaryLoading(false);
  }, [apiClient]);

  const markSessionSummariesStale = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    summaryStaleSessionsRef.current.add(sessionId);
    markConversationSummaryCacheStale(apiClient, sessionId);
  }, [apiClient]);

  const loadSessionSummaries = useCallback(async (
    sessionId: string,
    options?: { silent?: boolean; force?: boolean },
  ) => {
    if (!sessionId) {
      setSummaryItems([]);
      setSummaryError(null);
      setSummaryLoading(false);
      setSummaryLoadedSessionId(null);
      return;
    }

    const force = options?.force === true;
    const cached = getCachedConversationSummaryItems(apiClient, sessionId);
    const isStale = summaryStaleSessionsRef.current.has(sessionId);
    if (
      !force
      && !isStale
      && summaryLoadedSessionId === sessionId
      && summaryItems.length > 0
    ) {
      return;
    }
    if (!force && !isStale && cached) {
      if (currentSummarySessionIdRef.current === sessionId) {
        setSummaryItems(cached);
        setSummaryError(null);
        setSummaryLoadedSessionId(sessionId);
        setSummaryLoading(false);
      }
      return;
    }

    const requestSeq = summaryLoadSeqRef.current + 1;
    summaryLoadSeqRef.current = requestSeq;
    const shouldShowLoading = !options?.silent && (!cached || force);
    if (shouldShowLoading) {
      setSummaryLoading(true);
    }
    setSummaryError(null);
    try {
      const normalized = await loadConversationSummaryItems(apiClient, sessionId, {
        force,
        limit: 100,
      });
      if (
        summaryLoadSeqRef.current !== requestSeq
        || currentSummarySessionIdRef.current !== sessionId
      ) {
        return;
      }
      summaryStaleSessionsRef.current.delete(sessionId);
      setSummaryItems(normalized);
      setSummaryLoadedSessionId(sessionId);
    } catch (error) {
      if (
        summaryLoadSeqRef.current !== requestSeq
        || currentSummarySessionIdRef.current !== sessionId
      ) {
        return;
      }
      setSummaryError(error instanceof Error ? error.message : '加载会话总结失败');
      setSummaryItems([]);
    } finally {
      if (
        summaryLoadSeqRef.current === requestSeq
        && currentSummarySessionIdRef.current === sessionId
      ) {
        setSummaryLoading(false);
      }
    }
  }, [apiClient, summaryItems.length, summaryLoadedSessionId]);

  const openSummaryForSession = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    if (summaryPaneSessionId === sessionId) {
      setSummaryPaneSessionId(null);
      return;
    }
    setSummaryPaneSessionId(sessionId);
    const cached = getCachedConversationSummaryItems(apiClient, sessionId);
    setSummaryItems(cached ? [...cached] : []);
    setSummaryError(null);
    setSummaryLoadedSessionId(cached ? sessionId : null);
    setSummaryLoading(!cached);
    await loadSessionSummaries(sessionId);
  }, [apiClient, loadSessionSummaries, summaryPaneSessionId, setSummaryPaneSessionId]);

  const deleteSummary = useCallback(async (sessionId: string, summaryId: string) => {
    if (!sessionId || !summaryId) {
      return;
    }
    setDeletingSummaryId(summaryId);
    setSummaryError(null);
    try {
      await apiClient.deleteConversationSummary(sessionId, summaryId);
      markSessionSummariesStale(sessionId);
      await loadSessionSummaries(sessionId, { silent: true, force: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '删除总结失败');
    } finally {
      setDeletingSummaryId((prev) => (prev === summaryId ? null : prev));
    }
  }, [apiClient, loadSessionSummaries, markSessionSummariesStale]);

  const clearSummaries = useCallback(async (
    sessionId: string,
    options?: { confirmMessage?: string; skipConfirm?: boolean },
  ) => {
    if (!sessionId) {
      return;
    }
    const confirmed = options?.skipConfirm === true
      || await confirm({
        title: '清空会话总结',
        message: options?.confirmMessage || '确定清空当前会话的所有总结吗？',
        confirmText: '清空',
        cancelText: '取消',
        type: 'danger',
      });
    if (!confirmed) {
      return;
    }
    setClearingSummaries(true);
    setSummaryError(null);
    try {
      await apiClient.clearConversationSummaries(sessionId);
      markSessionSummariesStale(sessionId);
      await loadSessionSummaries(sessionId, { silent: true, force: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '清空总结失败');
    } finally {
      setClearingSummaries(false);
    }
  }, [apiClient, confirm, loadSessionSummaries, markSessionSummariesStale]);

  return {
    summaryPaneSessionId,
    summaryItems,
    summaryLoading,
    summaryError,
    clearingSummaries,
    deletingSummaryId,
    setSummaryPaneSessionId,
    setSummaryError,
    resetSummaryState,
    loadSessionSummaries,
    markSessionSummariesStale,
    hydrateSessionSummariesFromCache,
    cancelPendingSessionSummariesLoad,
    applyRealtimeSessionSummaries,
    openSummaryForSession,
    deleteSummary,
    clearSummaries,
  };
};

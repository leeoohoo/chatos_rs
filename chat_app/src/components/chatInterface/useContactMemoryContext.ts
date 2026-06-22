import { useCallback, useEffect, useRef, useState } from 'react';

import type { SessionSummariesListResponse } from '../../lib/api/client/types';
import {
  applyConversationSummaryItemsSnapshot,
  loadConversationSummaryItems,
  markConversationSummaryCacheStale,
} from '../../lib/sessionSummaries/cache';
import {
  buildMemoryLoadKey,
  normalizeAgentRecalls,
  type MemoryCacheEntry,
} from './contactMemoryContext.helpers';
import {
  beginSessionLoadRequest,
  isSessionLoadRequestCurrent,
  runGuardedSessionLoad,
} from './sessionLoadGuard';
import { useI18n } from '../../i18n/I18nProvider';

export interface SessionMemorySummary {
  id: string;
  summaryText: string;
  status: string;
  level: number;
  createdAt: string;
  updatedAt: string;
}

export interface ContactAgentRecall {
  id: string;
  recallKey: string;
  recallText: string;
  level: number;
  confidence?: number | null;
  lastSeenAt?: string | null;
  updatedAt: string;
}

interface MemoryApiClient {
  getConversationSummaries: (
    sessionId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<SessionSummariesListResponse>;
  getContactAgentRecalls: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
}

interface UseContactMemoryContextOptions {
  apiClient: MemoryApiClient;
  currentSessionId: string | null;
  currentContactId: string;
  currentProjectIdForMemory: string;
}

interface UseContactMemoryContextResult {
  sessionMemorySummaries: SessionMemorySummary[];
  agentRecalls: ContactAgentRecall[];
  memoryLoading: boolean;
  memoryError: string | null;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<void>;
  loadSessionMemorySummaries: (sessionId: string, force?: boolean) => Promise<void>;
  applyRealtimeSessionMemorySummaries: (
    sessionId: string,
    payload: SessionSummariesListResponse | unknown,
  ) => void;
  markContactMemoryContextStale: (sessionId: string) => void;
  hydrateContactMemoryContextFromCache: (sessionId: string) => void;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
}

export const useContactMemoryContext = ({
  apiClient,
  currentSessionId,
  currentContactId,
  currentProjectIdForMemory,
}: UseContactMemoryContextOptions): UseContactMemoryContextResult => {
  const { t } = useI18n();
  const [sessionMemorySummaries, setSessionMemorySummaries] = useState<SessionMemorySummary[]>([]);
  const [agentRecalls, setAgentRecalls] = useState<ContactAgentRecall[]>([]);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [memoryError, setMemoryError] = useState<string | null>(null);
  const memoryLoadSeqRef = useRef(0);
  const memoryLoadedKeyRef = useRef<string | null>(null);
  const currentSessionIdRef = useRef<string | null>(currentSessionId);
  const memoryCacheRef = useRef<Map<string, MemoryCacheEntry>>(new Map());
  const staleMemorySessionsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    currentSessionIdRef.current = currentSessionId;
  }, [currentSessionId]);

  const resetMemoryState = useCallback(() => {
    setSessionMemorySummaries([]);
    setAgentRecalls([]);
    memoryLoadedKeyRef.current = null;
    setMemoryError(null);
    setMemoryLoading(false);
  }, []);

  const cancelPendingMemoryLoad = useCallback(() => {
    memoryLoadSeqRef.current += 1;
  }, []);

  const hydrateContactMemoryContextFromCache = useCallback((sessionId: string) => {
    if (!sessionId) {
      resetMemoryState();
      return;
    }
    const cached = memoryCacheRef.current.get(sessionId);
    setSessionMemorySummaries(cached ? [...cached.sessionMemorySummaries] : []);
    setAgentRecalls(cached ? [...cached.agentRecalls] : []);
    setMemoryError(null);
    setMemoryLoading(false);
  }, [resetMemoryState]);

  const applyMemoryCacheEntry = useCallback((sessionId: string, cached: MemoryCacheEntry) => {
    setSessionMemorySummaries(cached.sessionMemorySummaries);
    setAgentRecalls(cached.agentRecalls);
    setMemoryError(null);
    setMemoryLoading(false);
    memoryLoadedKeyRef.current = buildMemoryLoadKey(
      sessionId,
      currentContactId,
      currentProjectIdForMemory,
    );
  }, [currentContactId, currentProjectIdForMemory]);

  const applyRealtimeSessionMemorySummaries = useCallback((
    sessionId: string,
    payload: SessionSummariesListResponse | unknown,
  ) => {
    if (!sessionId) {
      return;
    }
    const normalized = applyConversationSummaryItemsSnapshot(apiClient, sessionId, payload, {
      loadedLimit: 300,
    });
    staleMemorySessionsRef.current.delete(sessionId);
    const cached = memoryCacheRef.current.get(sessionId);
    const nextEntry = {
      sessionMemorySummaries: normalized,
      agentRecalls: cached?.agentRecalls || [],
    };
    memoryCacheRef.current.set(sessionId, nextEntry);
    if (currentSessionIdRef.current !== sessionId) {
      return;
    }
    setSessionMemorySummaries(normalized);
    setAgentRecalls(nextEntry.agentRecalls);
    setMemoryError(null);
    setMemoryLoading(false);
  }, [apiClient]);

  const markContactMemoryContextStale = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    staleMemorySessionsRef.current.add(sessionId);
    markConversationSummaryCacheStale(apiClient, sessionId);
  }, [apiClient]);

  const loadSessionMemorySummaries = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId || !currentSessionId || currentSessionId !== sessionId) {
      resetMemoryState();
      return;
    }

    const loadKey = buildMemoryLoadKey(sessionId, currentContactId, currentProjectIdForMemory);
    const cached = memoryCacheRef.current.get(sessionId);
    const isStale = staleMemorySessionsRef.current.has(sessionId);
    if (!force && !isStale && memoryLoadedKeyRef.current === loadKey) {
      return;
    }
    if (!force && !isStale && cached) {
      applyMemoryCacheEntry(sessionId, cached);
      return;
    }

    const requestSeq = beginSessionLoadRequest(memoryLoadSeqRef);
    await runGuardedSessionLoad({
      applyResult: (selectedSessionSummaries) => {
        const preservedAgentRecalls = cached?.agentRecalls || [];
        setSessionMemorySummaries(selectedSessionSummaries);
        setAgentRecalls(preservedAgentRecalls);
        memoryCacheRef.current.set(sessionId, {
          sessionMemorySummaries: selectedSessionSummaries,
          agentRecalls: preservedAgentRecalls,
        });
        staleMemorySessionsRef.current.delete(sessionId);
        memoryLoadedKeyRef.current = loadKey;
      },
      errorMessage: t('memory.sessionSummaryLoadFailed'),
      load: () => loadConversationSummaryItems(apiClient, sessionId, {
        force,
        limit: 100,
      }),
      setError: setMemoryError,
      setLoading: setMemoryLoading,
      shouldApply: () => isSessionLoadRequestCurrent({
        currentSessionRef: currentSessionIdRef,
        requestSeq,
        requestSeqRef: memoryLoadSeqRef,
        sessionId,
      }),
    });
  }, [
    apiClient,
    applyMemoryCacheEntry,
    currentContactId,
    currentProjectIdForMemory,
    currentSessionId,
    resetMemoryState,
    t,
  ]);

  const loadContactMemoryContext = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId || !currentSessionId || currentSessionId !== sessionId) {
      resetMemoryState();
      return;
    }

    const normalizedContactId = currentContactId.trim();
    const loadKey = buildMemoryLoadKey(sessionId, currentContactId, currentProjectIdForMemory);
    const cached = memoryCacheRef.current.get(sessionId);
    const isStale = staleMemorySessionsRef.current.has(sessionId);
    if (!force && !isStale && memoryLoadedKeyRef.current === loadKey) {
      return;
    }
    if (!force && !isStale && cached) {
      applyMemoryCacheEntry(sessionId, cached);
      return;
    }

    if (!normalizedContactId) {
      setSessionMemorySummaries([]);
      setAgentRecalls([]);
      memoryLoadedKeyRef.current = loadKey;
      setMemoryError(t('memory.unboundContact'));
      setMemoryLoading(false);
      return;
    }

    const requestSeq = beginSessionLoadRequest(memoryLoadSeqRef);
    await runGuardedSessionLoad({
      applyResult: ({ recallRows, selectedSessionSummaries }) => {
        const selectedAgentRecalls = normalizeAgentRecalls(
          Array.isArray(recallRows) ? recallRows : [],
        );
        setSessionMemorySummaries(selectedSessionSummaries);
        setAgentRecalls(selectedAgentRecalls);
        memoryCacheRef.current.set(sessionId, {
          sessionMemorySummaries: selectedSessionSummaries,
          agentRecalls: selectedAgentRecalls,
        });
        staleMemorySessionsRef.current.delete(sessionId);
        memoryLoadedKeyRef.current = loadKey;
      },
      errorMessage: t('memory.loadFailed'),
      load: async () => {
        const [selectedSessionSummaries, recallRows] = await Promise.all([
          loadConversationSummaryItems(apiClient, sessionId, { force, limit: 100 }),
          apiClient.getContactAgentRecalls(normalizedContactId, { limit: 50, offset: 0 }),
        ]);
        return {
          selectedSessionSummaries,
          recallRows,
        };
      },
      setError: setMemoryError,
      setLoading: setMemoryLoading,
      shouldApply: () => isSessionLoadRequestCurrent({
        currentSessionRef: currentSessionIdRef,
        requestSeq,
        requestSeqRef: memoryLoadSeqRef,
        sessionId,
      }),
    });
  }, [
    apiClient,
    applyMemoryCacheEntry,
    currentContactId,
    currentProjectIdForMemory,
    currentSessionId,
    resetMemoryState,
    t,
  ]);

  return {
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

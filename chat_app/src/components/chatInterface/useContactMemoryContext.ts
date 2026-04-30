import { useCallback, useEffect, useRef, useState } from 'react';

import type { SessionSummariesListResponse } from '../../lib/api/client/types';
import { loadConversationSummaryItems, markConversationSummaryCacheStale } from '../../lib/sessionSummaries/cache';

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
  markContactMemoryContextStale: (sessionId: string) => void;
  hydrateContactMemoryContextFromCache: (sessionId: string) => void;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
}

const toTimestamp = (value: string | null | undefined): number => {
  const parsed = value ? new Date(value).getTime() : Number.NaN;
  return Number.isFinite(parsed) ? parsed : 0;
};

const asRecord = (value: unknown): Record<string, unknown> | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }
  return value as Record<string, unknown>;
};

const readString = (record: Record<string, unknown> | null, key: string): string => {
  if (!record) {
    return '';
  }
  const value = record[key];
  return typeof value === 'string' ? value : '';
};

const normalizeAgentRecalls = (rows: unknown[]): ContactAgentRecall[] => {
  const normalized = rows
    .map((item) => {
      const record = asRecord(item);
      return {
        id: String(record?.id || ''),
        recallKey: String(record?.recall_key || ''),
        recallText: String(record?.recall_text || ''),
        level: Number.isFinite(Number(record?.level)) ? Number(record?.level) : 0,
        confidence: typeof record?.confidence === 'number' ? record.confidence : null,
        lastSeenAt: readString(record, 'last_seen_at') || null,
        updatedAt: String(record?.updated_at || ''),
      };
    })
    .filter((item) => item.id && item.recallKey);

  return normalized
    .sort((left, right) => {
      if (right.level !== left.level) {
        return right.level - left.level;
      }
      return toTimestamp(right.updatedAt) - toTimestamp(left.updatedAt);
    })
    .slice(0, 1);
};

export const useContactMemoryContext = ({
  apiClient,
  currentSessionId,
  currentContactId,
  currentProjectIdForMemory,
}: UseContactMemoryContextOptions): UseContactMemoryContextResult => {
  const [sessionMemorySummaries, setSessionMemorySummaries] = useState<SessionMemorySummary[]>([]);
  const [agentRecalls, setAgentRecalls] = useState<ContactAgentRecall[]>([]);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [memoryError, setMemoryError] = useState<string | null>(null);
  const memoryLoadSeqRef = useRef(0);
  const memoryLoadedKeyRef = useRef<string | null>(null);
  const currentSessionIdRef = useRef<string | null>(currentSessionId);
  const memoryCacheRef = useRef<Map<string, {
    sessionMemorySummaries: SessionMemorySummary[];
    agentRecalls: ContactAgentRecall[];
  }>>(new Map());
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

    const normalizedContactId = currentContactId.trim();
    const normalizedProjectId = currentProjectIdForMemory.trim();
    const loadKey = `${sessionId}::${normalizedContactId || '-'}::${normalizedProjectId || '-'}`;
    const cached = memoryCacheRef.current.get(sessionId);
    const isStale = staleMemorySessionsRef.current.has(sessionId);
    if (!force && !isStale && memoryLoadedKeyRef.current === loadKey) {
      return;
    }
    if (!force && !isStale && cached) {
      setSessionMemorySummaries(cached.sessionMemorySummaries);
      setAgentRecalls(cached.agentRecalls);
      setMemoryError(null);
      setMemoryLoading(false);
      memoryLoadedKeyRef.current = loadKey;
      return;
    }

    const requestSeq = memoryLoadSeqRef.current + 1;
    memoryLoadSeqRef.current = requestSeq;
    setMemoryLoading(true);
    setMemoryError(null);
    try {
      const selectedSessionSummaries = await loadConversationSummaryItems(apiClient, sessionId, {
        force,
        limit: 300,
      });

      if (
        memoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }
      const preservedAgentRecalls = cached?.agentRecalls || [];

      setSessionMemorySummaries(selectedSessionSummaries);
      setAgentRecalls(preservedAgentRecalls);
      memoryCacheRef.current.set(sessionId, {
        sessionMemorySummaries: selectedSessionSummaries,
        agentRecalls: preservedAgentRecalls,
      });
      staleMemorySessionsRef.current.delete(sessionId);
      memoryLoadedKeyRef.current = loadKey;
    } catch (error) {
      if (
        memoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }
      setMemoryError(error instanceof Error ? error.message : '会话总结加载失败');
    } finally {
      if (
        memoryLoadSeqRef.current === requestSeq
        && currentSessionIdRef.current === sessionId
      ) {
        setMemoryLoading(false);
      }
    }
  }, [
    apiClient,
    currentContactId,
    currentProjectIdForMemory,
    currentSessionId,
    resetMemoryState,
  ]);

  const loadContactMemoryContext = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId || !currentSessionId || currentSessionId !== sessionId) {
      resetMemoryState();
      return;
    }

    const normalizedContactId = currentContactId.trim();
    const normalizedProjectId = currentProjectIdForMemory.trim();
    const loadKey = `${sessionId}::${normalizedContactId || '-'}::${normalizedProjectId || '-'}`;
    const cached = memoryCacheRef.current.get(sessionId);
    const isStale = staleMemorySessionsRef.current.has(sessionId);
    if (!force && !isStale && memoryLoadedKeyRef.current === loadKey) {
      return;
    }
    if (!force && !isStale && cached) {
      setSessionMemorySummaries(cached.sessionMemorySummaries);
      setAgentRecalls(cached.agentRecalls);
      setMemoryError(null);
      setMemoryLoading(false);
      memoryLoadedKeyRef.current = loadKey;
      return;
    }

    if (!normalizedContactId) {
      setSessionMemorySummaries([]);
      setAgentRecalls([]);
      memoryLoadedKeyRef.current = loadKey;
      setMemoryError('当前会话未绑定联系人，无法加载记忆。');
      setMemoryLoading(false);
      return;
    }

    const requestSeq = memoryLoadSeqRef.current + 1;
    memoryLoadSeqRef.current = requestSeq;
    setMemoryLoading(true);
    setMemoryError(null);
    try {
      const [selectedSessionSummaries, recallRows] = await Promise.all([
        loadConversationSummaryItems(apiClient, sessionId, { force, limit: 300 }),
        apiClient.getContactAgentRecalls(normalizedContactId, { limit: 200, offset: 0 }),
      ]);

      if (
        memoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }

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
    } catch (error) {
      if (
        memoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }
      setMemoryError(error instanceof Error ? error.message : '记忆加载失败');
    } finally {
      if (
        memoryLoadSeqRef.current === requestSeq
        && currentSessionIdRef.current === sessionId
      ) {
        setMemoryLoading(false);
      }
    }
  }, [
    apiClient,
    currentContactId,
    currentProjectIdForMemory,
    currentSessionId,
    resetMemoryState,
  ]);

  return {
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    loadSessionMemorySummaries,
    markContactMemoryContextStale,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
  };
};

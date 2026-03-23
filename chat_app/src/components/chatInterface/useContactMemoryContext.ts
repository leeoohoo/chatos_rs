import { useCallback, useEffect, useRef, useState } from 'react';

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
  getSessionSummaries: (
    sessionId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<{ items?: unknown[] }>;
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
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
}

const toTimestamp = (value: string | null | undefined): number => {
  const parsed = value ? new Date(value).getTime() : Number.NaN;
  return Number.isFinite(parsed) ? parsed : 0;
};

const compareByNewestTime = (
  left: { createdAt?: string; updatedAt?: string },
  right: { createdAt?: string; updatedAt?: string },
): number => {
  const leftTs = Math.max(toTimestamp(left.updatedAt), toTimestamp(left.createdAt));
  const rightTs = Math.max(toTimestamp(right.updatedAt), toTimestamp(right.createdAt));
  return rightTs - leftTs;
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

const normalizeSessionSummaries = (rows: unknown[]): SessionMemorySummary[] => {
  const normalized = rows
    .map((item) => {
      const record = asRecord(item);
      return {
        id: String(record?.id || ''),
        summaryText: String(record?.summary_text ?? record?.summaryText ?? ''),
        status: String(record?.status || ''),
        level: Number.isFinite(Number(record?.level)) ? Number(record?.level) : 0,
        createdAt: String(record?.created_at ?? record?.createdAt ?? ''),
        updatedAt: String(record?.updated_at ?? record?.updatedAt ?? ''),
      };
    })
    .filter((item) => item.id && item.summaryText.trim().length > 0);

  const retainedLevel0 = normalized
    .filter((item) => item.level === 0)
    .sort(compareByNewestTime);
  const topLevel = [...normalized]
    .sort((left, right) => {
      if (right.level !== left.level) {
        return right.level - left.level;
      }
      return compareByNewestTime(left, right);
    })
    .slice(0, 2);

  const selectedMap = new Map<string, SessionMemorySummary>();
  for (const item of [...retainedLevel0, ...topLevel]) {
    if (!selectedMap.has(item.id)) {
      selectedMap.set(item.id, item);
    }
  }
  return Array.from(selectedMap.values());
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
  const [memoryLoadedKey, setMemoryLoadedKey] = useState<string | null>(null);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [memoryError, setMemoryError] = useState<string | null>(null);
  const memoryLoadSeqRef = useRef(0);
  const currentSessionIdRef = useRef<string | null>(currentSessionId);

  useEffect(() => {
    currentSessionIdRef.current = currentSessionId;
  }, [currentSessionId]);

  const resetMemoryState = useCallback(() => {
    setSessionMemorySummaries([]);
    setAgentRecalls([]);
    setMemoryLoadedKey(null);
    setMemoryError(null);
    setMemoryLoading(false);
  }, []);

  const cancelPendingMemoryLoad = useCallback(() => {
    memoryLoadSeqRef.current += 1;
  }, []);

  const loadContactMemoryContext = useCallback(async (sessionId: string, force = false) => {
    if (!sessionId || !currentSessionId || currentSessionId !== sessionId) {
      resetMemoryState();
      return;
    }

    const normalizedContactId = currentContactId.trim();
    const normalizedProjectId = currentProjectIdForMemory.trim();
    const loadKey = `${sessionId}::${normalizedContactId || '-'}::${normalizedProjectId || '-'}`;
    if (!force && memoryLoadedKey === loadKey) {
      return;
    }

    if (!normalizedContactId) {
      setSessionMemorySummaries([]);
      setAgentRecalls([]);
      setMemoryLoadedKey(loadKey);
      setMemoryError('当前会话未绑定联系人，无法加载记忆。');
      setMemoryLoading(false);
      return;
    }

    const requestSeq = memoryLoadSeqRef.current + 1;
    memoryLoadSeqRef.current = requestSeq;
    setMemoryLoading(true);
    setMemoryError(null);
    try {
      const [summaryRows, recallRows] = await Promise.all([
        apiClient.getSessionSummaries(sessionId, { limit: 300, offset: 0 }),
        apiClient.getContactAgentRecalls(normalizedContactId, { limit: 200, offset: 0 }),
      ]);

      if (
        memoryLoadSeqRef.current !== requestSeq
        || currentSessionIdRef.current !== sessionId
      ) {
        return;
      }

      const selectedSessionSummaries = normalizeSessionSummaries(
        Array.isArray(summaryRows?.items) ? summaryRows.items : [],
      );
      const selectedAgentRecalls = normalizeAgentRecalls(
        Array.isArray(recallRows) ? recallRows : [],
      );

      setSessionMemorySummaries(selectedSessionSummaries);
      setAgentRecalls(selectedAgentRecalls);
      setMemoryLoadedKey(loadKey);
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
    memoryLoadedKey,
    resetMemoryState,
  ]);

  return {
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    resetMemoryState,
    cancelPendingMemoryLoad,
  };
};

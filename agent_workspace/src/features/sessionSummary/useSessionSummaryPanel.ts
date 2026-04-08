import { useCallback, useState } from 'react';

export interface SessionSummaryItem {
  id: string;
  summaryText: string;
  summaryModel: string;
  triggerType: string;
  sourceMessageCount: number;
  sourceEstimatedTokens: number;
  status: string;
  errorMessage: string | null;
  level: number;
  createdAt: string;
  updatedAt: string;
}

interface SessionSummaryApiClient {
  getSessionSummaries: (
    sessionId: string,
    options?: { limit?: number; offset?: number },
  ) => Promise<{ items?: any[] }>;
  deleteSessionSummary: (sessionId: string, summaryId: string) => Promise<any>;
  clearSessionSummaries: (sessionId: string) => Promise<any>;
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
  loadSessionSummaries: (sessionId: string, options?: { silent?: boolean }) => Promise<void>;
  openSummaryForSession: (sessionId: string) => Promise<void>;
  deleteSummary: (sessionId: string, summaryId: string) => Promise<void>;
  clearSummaries: (
    sessionId: string,
    options?: { confirmMessage?: string; skipConfirm?: boolean },
  ) => Promise<void>;
}

const normalizeSessionSummary = (item: any): SessionSummaryItem | null => {
  const id = typeof item?.id === 'string' ? item.id.trim() : '';
  if (!id) {
    return null;
  }
  const createdAt = typeof item?.created_at === 'string'
    ? item.created_at
    : (typeof item?.createdAt === 'string' ? item.createdAt : '');
  const updatedAt = typeof item?.updated_at === 'string'
    ? item.updated_at
    : (typeof item?.updatedAt === 'string' ? item.updatedAt : createdAt);

  return {
    id,
    summaryText: typeof item?.summary_text === 'string'
      ? item.summary_text
      : (typeof item?.summaryText === 'string' ? item.summaryText : ''),
    summaryModel: typeof item?.summary_model === 'string'
      ? item.summary_model
      : (typeof item?.summaryModel === 'string' ? item.summaryModel : ''),
    triggerType: typeof item?.trigger_type === 'string'
      ? item.trigger_type
      : (typeof item?.triggerType === 'string' ? item.triggerType : ''),
    sourceMessageCount: Number(item?.source_message_count ?? item?.sourceMessageCount ?? 0) || 0,
    sourceEstimatedTokens: Number(item?.source_estimated_tokens ?? item?.sourceEstimatedTokens ?? 0) || 0,
    status: typeof item?.status === 'string' ? item.status : '',
    errorMessage: typeof item?.error_message === 'string'
      ? item.error_message
      : (typeof item?.errorMessage === 'string' ? item.errorMessage : null),
    level: Number(item?.level ?? 0) || 0,
    createdAt,
    updatedAt,
  };
};

export const useSessionSummaryPanel = (
  apiClient: SessionSummaryApiClient,
): UseSessionSummaryPanelResult => {
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);
  const [summaryItems, setSummaryItems] = useState<SessionSummaryItem[]>([]);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [clearingSummaries, setClearingSummaries] = useState(false);
  const [deletingSummaryId, setDeletingSummaryId] = useState<string | null>(null);

  const resetSummaryState = useCallback(() => {
    setSummaryItems([]);
    setSummaryError(null);
  }, []);

  const loadSessionSummaries = useCallback(async (
    sessionId: string,
    options?: { silent?: boolean },
  ) => {
    if (!sessionId) {
      setSummaryItems([]);
      setSummaryError(null);
      setSummaryLoading(false);
      return;
    }
    if (!options?.silent) {
      setSummaryLoading(true);
    }
    setSummaryError(null);
    try {
      const result = await apiClient.getSessionSummaries(sessionId, { limit: 200, offset: 0 });
      const normalized = (Array.isArray(result?.items) ? result.items : [])
        .map((item: any) => normalizeSessionSummary(item))
        .filter((item: SessionSummaryItem | null): item is SessionSummaryItem => Boolean(item))
        .sort((left, right) => {
          const leftTs = new Date(left.createdAt || left.updatedAt).getTime();
          const rightTs = new Date(right.createdAt || right.updatedAt).getTime();
          return (Number.isFinite(rightTs) ? rightTs : 0) - (Number.isFinite(leftTs) ? leftTs : 0);
        });
      setSummaryItems(normalized);
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '加载聊天摘要失败');
      setSummaryItems([]);
    } finally {
      setSummaryLoading(false);
    }
  }, [apiClient]);

  const openSummaryForSession = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    if (summaryPaneSessionId === sessionId) {
      setSummaryPaneSessionId(null);
      return;
    }
    setSummaryPaneSessionId(sessionId);
    await loadSessionSummaries(sessionId);
  }, [loadSessionSummaries, summaryPaneSessionId]);

  const deleteSummary = useCallback(async (sessionId: string, summaryId: string) => {
    if (!sessionId || !summaryId) {
      return;
    }
    setDeletingSummaryId(summaryId);
    setSummaryError(null);
    try {
      await apiClient.deleteSessionSummary(sessionId, summaryId);
      await loadSessionSummaries(sessionId, { silent: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '删除总结失败');
    } finally {
      setDeletingSummaryId((prev) => (prev === summaryId ? null : prev));
    }
  }, [apiClient, loadSessionSummaries]);

  const clearSummaries = useCallback(async (
    sessionId: string,
    options?: { confirmMessage?: string; skipConfirm?: boolean },
  ) => {
    if (!sessionId) {
      return;
    }
    const confirmed = options?.skipConfirm === true
      || typeof window === 'undefined'
      || window.confirm(options?.confirmMessage || '确定清空当前聊天的所有摘要吗？');
    if (!confirmed) {
      return;
    }
    setClearingSummaries(true);
    setSummaryError(null);
    try {
      await apiClient.clearSessionSummaries(sessionId);
      await loadSessionSummaries(sessionId, { silent: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '清空总结失败');
    } finally {
      setClearingSummaries(false);
    }
  }, [apiClient, loadSessionSummaries]);

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
    openSummaryForSession,
    deleteSummary,
    clearSummaries,
  };
};

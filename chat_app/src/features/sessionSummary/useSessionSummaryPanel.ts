import { useCallback, useState } from 'react';

import { useDialogService } from '../../components/ui/DialogProvider';
import type {
  SessionSummariesListResponse,
} from '../../lib/api/client/types';
import {
  normalizeSessionSummary,
  type SessionSummaryItem,
} from '../../lib/domain/configs';
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
  loadSessionSummaries: (sessionId: string, options?: { silent?: boolean }) => Promise<void>;
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
      const result = await apiClient.getConversationSummaries(sessionId, { limit: 200, offset: 0 });
      const normalized = (Array.isArray(result?.items) ? result.items : [])
        .map((item) => normalizeSessionSummary(item))
        .filter((item: SessionSummaryItem | null): item is SessionSummaryItem => Boolean(item))
        .sort((left, right) => {
          const leftTs = new Date(left.createdAt || left.updatedAt).getTime();
          const rightTs = new Date(right.createdAt || right.updatedAt).getTime();
          return (Number.isFinite(rightTs) ? rightTs : 0) - (Number.isFinite(leftTs) ? leftTs : 0);
        });
      setSummaryItems(normalized);
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '加载会话总结失败');
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
      await apiClient.deleteConversationSummary(sessionId, summaryId);
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
      await loadSessionSummaries(sessionId, { silent: true });
    } catch (error) {
      setSummaryError(error instanceof Error ? error.message : '清空总结失败');
    } finally {
      setClearingSummaries(false);
    }
  }, [apiClient, confirm, loadSessionSummaries]);

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

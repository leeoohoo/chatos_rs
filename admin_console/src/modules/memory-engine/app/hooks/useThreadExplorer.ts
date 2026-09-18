// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Form } from 'antd';
import { useEffect, useRef, useState } from 'react';

import { api } from '../../api';
import type {
  EngineRecord,
  EngineSubjectMemory,
  EngineSummary,
  EngineThread,
  ThreadQuery,
  ThreadRecordsQuery,
  ThreadSummariesQuery,
} from '../../types';
import type { TabKey, ThreadFilterFormValues } from '../types';
import { textOrUndefined, threadMemorySubjectId, threadScopeKey } from '../utils';

export const DEFAULT_THREAD_FILTERS: ThreadFilterFormValues = {
  source_id: undefined,
  tenant_id: '',
  subject_id: '',
  external_thread_id: '',
  session_id: '',
  contact_id: '',
  project_id: '',
  agent_id: '',
  mapping_source: '',
  mapping_version: '',
  thread_label: '',
  status: 'active',
  limit: 100,
  offset: 0,
};
const DEFAULT_RECORD_PAGE_SIZE = 20;

export type ThreadListCursor = {
  updatedAt: string;
  createdAt: string;
  id: string;
};

type RecordListCursor = {
  createdAt: string;
  id: string;
};

type SummaryListCursor = {
  level: number;
  createdAt: string;
  id: string;
};

export function buildThreadFilters(values: ThreadFilterFormValues): ThreadQuery {
  return {
    source_id: textOrUndefined(values.source_id),
    tenant_id: textOrUndefined(values.tenant_id),
    subject_id: textOrUndefined(values.subject_id),
    external_thread_id: textOrUndefined(values.external_thread_id),
    session_id: textOrUndefined(values.session_id),
    contact_id: textOrUndefined(values.contact_id),
    project_id: textOrUndefined(values.project_id),
    agent_id: textOrUndefined(values.agent_id),
    mapping_source: textOrUndefined(values.mapping_source),
    mapping_version: textOrUndefined(values.mapping_version),
    thread_label: textOrUndefined(values.thread_label),
    status: textOrUndefined(values.status),
    limit: values.limit ?? 100,
    offset: values.offset ?? 0,
  };
}

type ThreadExplorerOptions = {
  onError?: (message: string) => void;
};

export function useThreadExplorer(tab: TabKey, options?: ThreadExplorerOptions) {
  const [threadsLoading, setThreadsLoading] = useState(false);
  const [threadDetailLoading, setThreadDetailLoading] = useState(false);
  const [threadRecordsLoading, setThreadRecordsLoading] = useState(false);
  const [threadFilters, setThreadFilters] =
    useState<ThreadQuery>(DEFAULT_THREAD_FILTERS);
  const [threads, setThreads] = useState<EngineThread[]>([]);
  const [threadPage, setThreadPage] = useState(1);
  const [threadPageSize, setThreadPageSize] = useState(DEFAULT_THREAD_FILTERS.limit ?? 100);
  const [threadPageCursors, setThreadPageCursors] =
    useState<Array<ThreadListCursor | null>>([null]);
  const [threadHasMore, setThreadHasMore] = useState(false);
  const [selectedThread, setSelectedThread] = useState<EngineThread | null>(null);
  const [threadRecords, setThreadRecords] = useState<EngineRecord[]>([]);
  const [threadRecordPage, setThreadRecordPage] = useState(1);
  const [threadRecordPageSize, setThreadRecordPageSize] = useState(DEFAULT_RECORD_PAGE_SIZE);
  const [threadRecordTotal, setThreadRecordTotal] = useState(0);
  const [threadRecordHasMore, setThreadRecordHasMore] = useState(false);
  const [threadRecordPageCursors, setThreadRecordPageCursors] =
    useState<Array<RecordListCursor | null>>([null]);
  const [threadSummaries, setThreadSummaries] = useState<EngineSummary[]>([]);
  const [threadSummaryPage, setThreadSummaryPage] = useState(1);
  const [threadSummaryPageSize, setThreadSummaryPageSize] = useState(10);
  const [threadSummaryHasMore, setThreadSummaryHasMore] = useState(false);
  const [threadSummaryPageCursors, setThreadSummaryPageCursors] =
    useState<Array<SummaryListCursor | null>>([null]);
  const [subjectMemories, setSubjectMemories] = useState<EngineSubjectMemory[]>([]);
  const [detailTab, setDetailTab] = useState<'records' | 'summaries' | 'memories'>('records');

  const [threadFilterForm] = Form.useForm<ThreadFilterFormValues>();
  const selectedThreadKey = threadScopeKey(selectedThread) ?? '';
  const threadsRequestIdRef = useRef(0);
  const threadRecordsRequestIdRef = useRef(0);
  const threadDetailRequestIdRef = useRef(0);
  const selectedThreadKeyRef = useRef(selectedThreadKey);
  const detailTabRef = useRef(detailTab);

  useEffect(() => {
    selectedThreadKeyRef.current = selectedThreadKey;
  }, [selectedThreadKey]);

  useEffect(() => {
    detailTabRef.current = detailTab;
  }, [detailTab]);

  const reportError = (message: string) => {
    options?.onError?.(message);
  };

  const fetchThreadRecordsPage = async (
    thread: EngineThread,
    requestedPage: number,
    requestedPageSize: number,
    cursor: RecordListCursor | null,
  ) => {
    const recordQueryBase: ThreadRecordsQuery = {
      tenant_id: thread.tenant_id,
      source_id: thread.source_id,
      order: 'asc',
    };
    const pageSize = Math.max(1, requestedPageSize);
    const pageResult = await api.listThreadRecords(thread.id, {
      ...recordQueryBase,
      limit: pageSize,
      offset: 0,
      ...(cursor
        ? { after_created_at: cursor.createdAt, after_id: cursor.id }
        : {}),
    });

    return {
      records: pageResult.items,
      total: pageResult.total,
      hasMore: Boolean(pageResult.has_more),
      page: Math.max(1, requestedPage),
      pageSize,
    };
  };

  const loadThreadRecordsPage = async (
    thread: EngineThread,
    requestedPage: number,
    requestedPageSize: number,
    cursor?: RecordListCursor | null,
  ) => {
    const requestId = threadRecordsRequestIdRef.current + 1;
    threadRecordsRequestIdRef.current = requestId;
    setThreadRecordsLoading(true);
    try {
      const result = await fetchThreadRecordsPage(
        thread,
        requestedPage,
        requestedPageSize,
        cursor === undefined
          ? threadRecordPageCursors[requestedPage - 1] ?? null
          : cursor,
      );
      if (threadRecordsRequestIdRef.current !== requestId) {
        return;
      }
      setThreadRecords(result.records);
      setThreadRecordTotal(result.total);
      setThreadRecordHasMore(result.hasMore);
      setThreadRecordPage(result.page);
      setThreadRecordPageSize(result.pageSize);
    } catch (error) {
      if (threadRecordsRequestIdRef.current === requestId) {
        reportError(`加载线程记录失败：${String(error)}`);
      }
    } finally {
      if (threadRecordsRequestIdRef.current === requestId) {
        setThreadRecordsLoading(false);
      }
    }
  };

  const loadThreadSupportingDetails = async (
    thread: EngineThread,
    tabKey: 'summaries' | 'memories',
    pagination?: {
      page?: number;
      pageSize?: number;
      cursor?: SummaryListCursor | null;
    },
  ) => {
    const requestId = threadDetailRequestIdRef.current + 1;
    threadDetailRequestIdRef.current = requestId;
    const threadKey = threadScopeKey(thread) ?? '';
    setThreadDetailLoading(true);
    try {
      if (tabKey === 'summaries') {
        setThreadSummaries([]);
        const page = pagination?.page ?? threadSummaryPage;
        const pageSize = Math.max(1, pagination?.pageSize ?? threadSummaryPageSize);
        const cursor = pagination && 'cursor' in pagination
          ? pagination.cursor ?? null
          : threadSummaryPageCursors[page - 1] ?? null;
        const summaryQuery: ThreadSummariesQuery = {
          tenant_id: thread.tenant_id,
          source_id: thread.source_id,
          limit: pageSize,
          offset: 0,
          ...(cursor
            ? {
                after_level: cursor.level,
                after_created_at: cursor.createdAt,
                after_id: cursor.id,
              }
            : {}),
        };
        const summaries = await api.listThreadSummaries(thread.id, summaryQuery);
        if (
          threadDetailRequestIdRef.current !== requestId ||
          selectedThreadKeyRef.current !== threadKey ||
          detailTabRef.current !== 'summaries'
        ) {
          return;
        }
        setThreadSummaries(summaries.items);
        setThreadSummaryPage(page);
        setThreadSummaryPageSize(pageSize);
        setThreadSummaryHasMore(summaries.has_more);
        return;
      }

      setSubjectMemories([]);
      const memorySubjectId = threadMemorySubjectId(thread);
      if (!memorySubjectId) {
        return;
      }
      const memories = await api.listSubjectMemories(memorySubjectId, {
        tenant_id: thread.tenant_id,
        source_id: thread.source_id,
        limit: 100,
        offset: 0,
      });
      if (
        threadDetailRequestIdRef.current !== requestId ||
        selectedThreadKeyRef.current !== threadKey ||
        detailTabRef.current !== 'memories'
      ) {
        return;
      }
      setSubjectMemories(memories);
    } catch (error) {
      if (threadDetailRequestIdRef.current === requestId) {
        reportError(
          tabKey === 'summaries'
            ? `加载线程总结失败：${String(error)}`
            : `加载主题记忆失败：${String(error)}`,
        );
      }
    } finally {
      if (threadDetailRequestIdRef.current === requestId) {
        setThreadDetailLoading(false);
      }
    }
  };

  const loadThreadDetails = async (
    thread: EngineThread,
    options?: { page?: number; pageSize?: number; resetPage?: boolean },
  ) => {
    const requestId = threadRecordsRequestIdRef.current + 1;
    threadRecordsRequestIdRef.current = requestId;
    setThreadRecordsLoading(true);
    try {
      const nextThreadKey = threadScopeKey(thread) ?? '';
      const resetPage = options?.resetPage ?? nextThreadKey !== selectedThreadKey;
      const requestedPageSize = options?.pageSize ?? threadRecordPageSize;
      const requestedPage = options?.page ?? (resetPage ? 1 : threadRecordPage);
      const cursor = resetPage
        ? null
        : threadRecordPageCursors[requestedPage - 1] ?? null;
      if (resetPage) {
        setThreadRecordPageCursors([null]);
      }
      const recordResult = await fetchThreadRecordsPage(
        thread,
        requestedPage,
        requestedPageSize,
        cursor,
      );
      if (threadRecordsRequestIdRef.current !== requestId) {
        return;
      }
      selectedThreadKeyRef.current = nextThreadKey;
      setSelectedThread(thread);
      setThreadRecords(recordResult.records);
      setThreadRecordTotal(recordResult.total);
      setThreadRecordHasMore(recordResult.hasMore);
      setThreadRecordPage(recordResult.page);
      setThreadRecordPageSize(recordResult.pageSize);
      if (nextThreadKey !== selectedThreadKey) {
        setThreadSummaries([]);
        setThreadSummaryPage(1);
        setThreadSummaryHasMore(false);
        setThreadSummaryPageCursors([null]);
        setSubjectMemories([]);
      }
    } catch (error) {
      if (threadRecordsRequestIdRef.current === requestId) {
        reportError(`加载线程记录失败：${String(error)}`);
      }
    } finally {
      if (threadRecordsRequestIdRef.current === requestId) {
        setThreadRecordsLoading(false);
      }
    }
  };

  const loadThreads = async (
    filters?: ThreadQuery,
    pagination?: {
      page?: number;
      pageSize?: number;
      cursor?: ThreadListCursor | null;
    },
  ) => {
    const nextFilters = filters ?? threadFilters;
    const requestedPage = pagination?.page ?? threadPage;
    const requestedPageSize = Math.max(
      1,
      pagination?.pageSize ?? nextFilters.limit ?? threadPageSize,
    );
    const cursor = pagination && 'cursor' in pagination
      ? pagination.cursor ?? null
      : threadPageCursors[requestedPage - 1] ?? null;
    const requestId = threadsRequestIdRef.current + 1;
    threadsRequestIdRef.current = requestId;
    setThreadsLoading(true);
    try {
      const result = await api.listThreads({
        ...nextFilters,
        before_updated_at: cursor?.updatedAt,
        before_created_at: cursor?.createdAt,
        before_id: cursor?.id,
        limit: requestedPageSize + 1,
        offset: 0,
      });
      if (threadsRequestIdRef.current !== requestId) {
        return;
      }
      const hasMore = result.length > requestedPageSize;
      const items = result.slice(0, requestedPageSize);
      setThreads(items);
      setThreadPage(requestedPage);
      setThreadPageSize(requestedPageSize);
      setThreadHasMore(hasMore);
      if (items.length === 0) {
        selectedThreadKeyRef.current = '';
        setSelectedThread(null);
        setThreadRecords([]);
        setThreadRecordTotal(0);
        setThreadRecordHasMore(false);
        setThreadRecordPageCursors([null]);
        setThreadRecordPage(1);
        setThreadSummaries([]);
        setThreadSummaryPage(1);
        setThreadSummaryHasMore(false);
        setThreadSummaryPageCursors([null]);
        setSubjectMemories([]);
        return;
      }
      const currentThreadKey = selectedThreadKey;
      const nextSelected =
        items.find((item) => threadScopeKey(item) === currentThreadKey) ?? items[0];
      await loadThreadDetails(nextSelected, {
        resetPage: threadScopeKey(nextSelected) !== currentThreadKey,
      });
      const activeDetailTab = detailTabRef.current;
      if (threadScopeKey(nextSelected) === currentThreadKey && activeDetailTab !== 'records') {
        await loadThreadSupportingDetails(nextSelected, activeDetailTab);
      }
    } catch (error) {
      if (threadsRequestIdRef.current === requestId) {
        reportError(`加载线程列表失败：${String(error)}`);
      }
    } finally {
      if (threadsRequestIdRef.current === requestId) {
        setThreadsLoading(false);
      }
    }
  };

  useEffect(() => {
    if (tab === 'data' && threads.length === 0 && !threadsLoading) {
      void loadThreads();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  useEffect(() => {
    if (tab !== 'data' || !selectedThread || detailTab === 'records') {
      return;
    }
    void loadThreadSupportingDetails(
      selectedThread,
      detailTab === 'summaries' ? 'summaries' : 'memories',
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailTab, selectedThreadKey, tab]);

  const handleApplyThreadFilters = async () => {
    try {
      const values = await threadFilterForm.validateFields();
      const nextFilters = buildThreadFilters(values);
      const pageSize = Math.max(1, nextFilters.limit ?? 100);
      setThreadFilters(nextFilters);
      setThreadPageCursors([null]);
      await loadThreads(nextFilters, { page: 1, pageSize, cursor: null });
    } catch (error) {
      const text = String(error);
      if (!text.includes('validate')) {
        reportError(`应用线程筛选失败：${text}`);
      }
    }
  };

  const handleResetThreadFilters = async () => {
    threadFilterForm.setFieldsValue(DEFAULT_THREAD_FILTERS);
    setThreadFilters(DEFAULT_THREAD_FILTERS);
    setThreadPageCursors([null]);
    await loadThreads(DEFAULT_THREAD_FILTERS, {
      page: 1,
      pageSize: DEFAULT_THREAD_FILTERS.limit,
      cursor: null,
    });
  };

  const handleThreadPageChange = async (page: number, pageSize: number) => {
    const normalizedPageSize = Math.max(1, pageSize);
    if (normalizedPageSize !== threadPageSize) {
      const nextFilters = {
        ...threadFilters,
        limit: normalizedPageSize,
        offset: 0,
      };
      threadFilterForm.setFieldValue('limit', normalizedPageSize);
      setThreadFilters(nextFilters);
      setThreadPageCursors([null]);
      await loadThreads(nextFilters, {
        page: 1,
        pageSize: normalizedPageSize,
        cursor: null,
      });
      return;
    }
    if (page < 1 || page === threadPage) {
      return;
    }
    if (page <= threadPageCursors.length) {
      await loadThreads(threadFilters, {
        page,
        pageSize: normalizedPageSize,
        cursor: threadPageCursors[page - 1] ?? null,
      });
      return;
    }
    if (
      page !== threadPage + 1 ||
      threadPage !== threadPageCursors.length ||
      !threadHasMore
    ) {
      return;
    }
    const lastThread = threads[threads.length - 1];
    if (!lastThread) {
      return;
    }
    const nextCursor: ThreadListCursor = {
      updatedAt: lastThread.updated_at,
      createdAt: lastThread.created_at,
      id: lastThread.id,
    };
    setThreadPageCursors((current) => [
      ...current.slice(0, threadPage),
      nextCursor,
    ]);
    await loadThreads(threadFilters, {
      page,
      pageSize: normalizedPageSize,
      cursor: nextCursor,
    });
  };

  const handleThreadRecordPageChange = async (page: number, pageSize: number) => {
    if (!selectedThread) {
      return;
    }
    const normalizedPageSize = Math.max(1, pageSize);
    if (normalizedPageSize !== threadRecordPageSize) {
      setThreadRecordPageCursors([null]);
      await loadThreadRecordsPage(selectedThread, 1, normalizedPageSize, null);
      return;
    }
    if (page < 1 || page === threadRecordPage) {
      return;
    }
    if (page <= threadRecordPageCursors.length) {
      await loadThreadRecordsPage(
        selectedThread,
        page,
        normalizedPageSize,
        threadRecordPageCursors[page - 1] ?? null,
      );
      return;
    }
    if (
      page !== threadRecordPage + 1 ||
      threadRecordPage !== threadRecordPageCursors.length ||
      !threadRecordHasMore
    ) {
      return;
    }
    const lastRecord = threadRecords[threadRecords.length - 1];
    if (!lastRecord) {
      return;
    }
    const nextCursor: RecordListCursor = {
      createdAt: lastRecord.created_at,
      id: lastRecord.id,
    };
    setThreadRecordPageCursors((current) => [
      ...current.slice(0, threadRecordPage),
      nextCursor,
    ]);
    await loadThreadRecordsPage(
      selectedThread,
      page,
      normalizedPageSize,
      nextCursor,
    );
  };

  const handleThreadSummaryPageChange = async (page: number, pageSize: number) => {
    if (!selectedThread) {
      return;
    }
    const normalizedPageSize = Math.max(1, pageSize);
    if (normalizedPageSize !== threadSummaryPageSize) {
      setThreadSummaryPageCursors([null]);
      await loadThreadSupportingDetails(selectedThread, 'summaries', {
        page: 1,
        pageSize: normalizedPageSize,
        cursor: null,
      });
      return;
    }
    if (page < 1 || page === threadSummaryPage) {
      return;
    }
    if (page <= threadSummaryPageCursors.length) {
      await loadThreadSupportingDetails(selectedThread, 'summaries', {
        page,
        pageSize: normalizedPageSize,
        cursor: threadSummaryPageCursors[page - 1] ?? null,
      });
      return;
    }
    if (
      page !== threadSummaryPage + 1 ||
      threadSummaryPage !== threadSummaryPageCursors.length ||
      !threadSummaryHasMore
    ) {
      return;
    }
    const lastSummary = threadSummaries[threadSummaries.length - 1];
    if (!lastSummary) {
      return;
    }
    const nextCursor: SummaryListCursor = {
      level: lastSummary.level,
      createdAt: lastSummary.created_at,
      id: lastSummary.id,
    };
    setThreadSummaryPageCursors((current) => [
      ...current.slice(0, threadSummaryPage),
      nextCursor,
    ]);
    await loadThreadSupportingDetails(selectedThread, 'summaries', {
      page,
      pageSize: normalizedPageSize,
      cursor: nextCursor,
    });
  };

  return {
    threadsLoading,
    threadDetailLoading,
    threadRecordsLoading,
    threadFilters,
    threads,
    threadPage,
    threadPageSize,
    threadHasMore,
    threadMaxReachablePage: Math.max(
      threadPageCursors.length,
      threadPage === threadPageCursors.length && threadHasMore
        ? threadPage + 1
        : threadPage,
    ),
    selectedThread,
    threadRecords,
    threadRecordPage,
    threadRecordPageSize,
    threadRecordTotal,
    threadRecordMaxReachablePage: Math.max(
      threadRecordPageCursors.length,
      threadRecordPage === threadRecordPageCursors.length && threadRecordHasMore
        ? threadRecordPage + 1
        : threadRecordPage,
    ),
    threadSummaries,
    threadSummaryPage,
    threadSummaryPageSize,
    threadSummaryHasMore,
    threadSummaryMaxReachablePage: Math.max(
      threadSummaryPageCursors.length,
      threadSummaryPage === threadSummaryPageCursors.length && threadSummaryHasMore
        ? threadSummaryPage + 1
        : threadSummaryPage,
    ),
    subjectMemories,
    detailTab,
    setDetailTab,
    threadFilterForm,
    loadThreadDetails,
    loadThreads,
    handleApplyThreadFilters,
    handleResetThreadFilters,
    handleThreadPageChange,
    handleThreadRecordPageChange,
    handleThreadSummaryPageChange,
  };
}

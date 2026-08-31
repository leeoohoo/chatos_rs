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
  const [selectedThread, setSelectedThread] = useState<EngineThread | null>(null);
  const [threadRecords, setThreadRecords] = useState<EngineRecord[]>([]);
  const [threadRecordPage, setThreadRecordPage] = useState(1);
  const [threadRecordPageSize, setThreadRecordPageSize] = useState(DEFAULT_RECORD_PAGE_SIZE);
  const [threadRecordTotal, setThreadRecordTotal] = useState(0);
  const [threadSummaries, setThreadSummaries] = useState<EngineSummary[]>([]);
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
  ) => {
    const recordQueryBase: ThreadRecordsQuery = {
      tenant_id: thread.tenant_id,
      source_id: thread.source_id,
      order: 'asc',
    };
    const pageSize = Math.max(1, requestedPageSize);
    const offset = Math.max(0, (requestedPage - 1) * pageSize);
    const pageResult = await api.listThreadRecords(thread.id, {
      ...recordQueryBase,
      limit: pageSize,
      offset,
    });
    const total = pageResult.total;
    const maxPage = total > 0 ? Math.ceil(total / pageSize) : 1;
    const page = Math.min(Math.max(1, requestedPage), maxPage);
    const records = page === requestedPage
      ? pageResult.items
      : total > 0
        ? (await api.listThreadRecords(thread.id, {
            ...recordQueryBase,
            limit: pageSize,
            offset: (page - 1) * pageSize,
          })).items
        : [];

    return {
      records,
      total,
      page,
      pageSize,
    };
  };

  const loadThreadRecordsPage = async (
    thread: EngineThread,
    requestedPage: number,
    requestedPageSize: number,
  ) => {
    const requestId = threadRecordsRequestIdRef.current + 1;
    threadRecordsRequestIdRef.current = requestId;
    setThreadRecordsLoading(true);
    try {
      const result = await fetchThreadRecordsPage(thread, requestedPage, requestedPageSize);
      if (threadRecordsRequestIdRef.current !== requestId) {
        return;
      }
      setThreadRecords(result.records);
      setThreadRecordTotal(result.total);
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
  ) => {
    const requestId = threadDetailRequestIdRef.current + 1;
    threadDetailRequestIdRef.current = requestId;
    const threadKey = threadScopeKey(thread) ?? '';
    setThreadDetailLoading(true);
    try {
      if (tabKey === 'summaries') {
        setThreadSummaries([]);
        const summaryQuery: ThreadSummariesQuery = {
          tenant_id: thread.tenant_id,
          source_id: thread.source_id,
          limit: 200,
          offset: 0,
        };
        const summaries = await api.listThreadSummaries(thread.id, summaryQuery);
        if (
          threadDetailRequestIdRef.current !== requestId ||
          selectedThreadKeyRef.current !== threadKey ||
          detailTabRef.current !== 'summaries'
        ) {
          return;
        }
        setThreadSummaries(summaries);
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
      const recordResult = await fetchThreadRecordsPage(thread, requestedPage, requestedPageSize);
      if (threadRecordsRequestIdRef.current !== requestId) {
        return;
      }
      selectedThreadKeyRef.current = nextThreadKey;
      setSelectedThread(thread);
      setThreadRecords(recordResult.records);
      setThreadRecordTotal(recordResult.total);
      setThreadRecordPage(recordResult.page);
      setThreadRecordPageSize(recordResult.pageSize);
      if (nextThreadKey !== selectedThreadKey) {
        setThreadSummaries([]);
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

  const loadThreads = async (filters?: ThreadQuery) => {
    const nextFilters = filters ?? threadFilters;
    const requestId = threadsRequestIdRef.current + 1;
    threadsRequestIdRef.current = requestId;
    setThreadsLoading(true);
    try {
      const items = await api.listThreads(nextFilters);
      if (threadsRequestIdRef.current !== requestId) {
        return;
      }
      setThreads(items);
      if (items.length === 0) {
        selectedThreadKeyRef.current = '';
        setSelectedThread(null);
        setThreadRecords([]);
        setThreadRecordTotal(0);
        setThreadRecordPage(1);
        setThreadSummaries([]);
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
      setThreadFilters(nextFilters);
      await loadThreads(nextFilters);
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
    await loadThreads(DEFAULT_THREAD_FILTERS);
  };

  const handleThreadRecordPageChange = async (page: number, pageSize: number) => {
    if (!selectedThread) {
      return;
    }
    await loadThreadRecordsPage(selectedThread, page, pageSize);
  };

  return {
    threadsLoading,
    threadDetailLoading,
    threadRecordsLoading,
    threadFilters,
    threads,
    selectedThread,
    threadRecords,
    threadRecordPage,
    threadRecordPageSize,
    threadRecordTotal,
    threadSummaries,
    subjectMemories,
    detailTab,
    setDetailTab,
    threadFilterForm,
    loadThreadDetails,
    loadThreads,
    handleApplyThreadFilters,
    handleResetThreadFilters,
    handleThreadRecordPageChange,
  };
}

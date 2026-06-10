import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Button,
  Collapse,
  Descriptions,
  Drawer,
  Empty,
  List,
  Pagination,
  Select,
  Segmented,
  Space,
  Statistic,
  Table,
  Tag,
  Timeline,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api, buildEventSourceUrl } from '../api/client';
import { useI18n, type TranslateFn } from '../i18n/I18nProvider';
import type {
  RemoteServerRecord,
  TaskSummaryRecord,
  TaskRunEventRecord,
  TaskRunRecord,
  TaskRunStatus,
  UiPromptRecord,
  UiPromptStatus,
} from '../types';

const runColorMap: Record<TaskRunStatus, string> = {
  queued: 'default',
  running: 'processing',
  succeeded: 'success',
  failed: 'error',
  cancelled: 'default',
  blocked: 'warning',
};

const promptColorMap: Record<UiPromptStatus, string> = {
  pending: 'processing',
  submitted: 'success',
  cancelled: 'default',
  timed_out: 'warning',
  failed: 'error',
};

const runStatusFilterValues: Array<TaskRunStatus | 'all'> = [
  'all',
  'queued',
  'running',
  'succeeded',
  'failed',
];

export function RunsPage() {
  const { t } = useI18n();
  const DEFAULT_PAGE_SIZE = 10;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<'all' | TaskRunStatus>('all');
  const [runPage, setRunPage] = useState(1);
  const [runPageSize, setRunPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [runPromptPage, setRunPromptPage] = useState(1);
  const [runPromptPageSize, setRunPromptPageSize] = useState(10);
  const [taskSearchTerm, setTaskSearchTerm] = useState('');
  const taskFilterId = searchParams.get('task_id') || undefined;
  const routeRunId = searchParams.get('run_id') || undefined;
  const routeModelConfigId = searchParams.get('model_config_id') || undefined;
  const runStatusOptions = useMemo(
    () => runStatusFilterValues.map((value) => ({
      label: t(`runs.status.${value}`),
      value,
    })),
    [t],
  );
  const runStatusLabel = (status: TaskRunStatus) => t(`runs.status.${status}`);
  const promptStatusLabel = (status: UiPromptStatus) => t(`prompts.status.${status}`);

  const runsQuery = useQuery({
    queryKey: ['runs', taskFilterId, statusFilter, routeModelConfigId, runPage, runPageSize],
    queryFn: () =>
      api.listRunsPage({
        task_id: taskFilterId,
        status: statusFilter === 'all' ? undefined : statusFilter,
        model_config_id: routeModelConfigId,
        limit: runPageSize,
        offset: (runPage - 1) * runPageSize,
      }),
  });
  const modelsQuery = useQuery({
    queryKey: ['model-configs'],
    queryFn: api.listModelConfigs,
  });
  const remoteServersQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const selectedRunQuery = useQuery({
    queryKey: ['run', selectedRunId],
    queryFn: () => api.getRun(selectedRunId!),
    enabled: Boolean(selectedRunId),
  });
  const runEventsQuery = useQuery({
    queryKey: ['run-events', selectedRunId],
    queryFn: () => api.getRunEvents(selectedRunId!),
    enabled: Boolean(selectedRunId),
  });
  const runPromptsQuery = useQuery({
    queryKey: ['run-prompts', selectedRunId, runPromptPage, runPromptPageSize],
    queryFn: () =>
      api.listPromptsPage({
        runId: selectedRunId!,
        limit: runPromptPageSize,
        offset: (runPromptPage - 1) * runPromptPageSize,
      }),
    enabled: Boolean(selectedRunId),
  });

  const displayTaskIds = useMemo(() => {
    const ids = new Set<string>();
    if (taskFilterId) {
      ids.add(taskFilterId);
    }
    if (selectedRunQuery.data?.task_id) {
      ids.add(selectedRunQuery.data.task_id);
    }
    (runsQuery.data?.items || []).forEach((run) => ids.add(run.task_id));
    return Array.from(ids).sort();
  }, [taskFilterId, selectedRunQuery.data?.task_id, runsQuery.data?.items]);

  const taskSummariesQuery = useQuery({
    queryKey: ['task-summaries', displayTaskIds.join(',')],
    queryFn: () => api.listTaskSummaries({ ids: displayTaskIds }),
    enabled: displayTaskIds.length > 0,
  });

  const taskSearchQuery = useQuery({
    queryKey: ['task-summary-search', taskSearchTerm],
    queryFn: () =>
      api.listTaskSummaries({
        keyword: taskSearchTerm.trim() || undefined,
        limit: 20,
      }),
    enabled: taskSearchTerm.trim().length > 0,
  });

  useEffect(() => {
    setSelectedRunId(routeRunId ?? null);
  }, [routeRunId]);

  useEffect(() => {
    setRunPromptPage(1);
  }, [selectedRunId]);

  useEffect(() => {
    setRunPage(1);
  }, [taskFilterId, statusFilter, routeModelConfigId]);

  useEffect(() => {
    if (!selectedRunId) {
      return undefined;
    }

    const eventSource = new EventSource(
      buildEventSourceUrl(`/api/runs/${selectedRunId}/stream`),
    );
    const refresh = () => {
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['run', selectedRunId] }),
        queryClient.invalidateQueries({ queryKey: ['run-events', selectedRunId] }),
        queryClient.invalidateQueries({ queryKey: ['run-prompts', selectedRunId] }),
      ]);
    };

    eventSource.addEventListener('run_event', refresh);
    eventSource.onerror = () => {
      eventSource.close();
    };

    return () => {
      eventSource.removeEventListener('run_event', refresh);
      eventSource.close();
    };
  }, [queryClient, selectedRunId]);

  const cancelRunMutation = useMutation({
    mutationFn: api.cancelRun,
    onSuccess: async (_, runId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['run', runId] }),
        queryClient.invalidateQueries({ queryKey: ['run-events', runId] }),
      ]);
      messageApi.success(t('runs.cancelRequested'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const retryRunMutation = useMutation({
    mutationFn: api.retryRun,
    onSuccess: async (run) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
      ]);
      const next = new URLSearchParams(searchParams);
      next.set('run_id', run.id);
      next.set('task_id', run.task_id);
      setSearchParams(next);
      setSelectedRunId(run.id);
      messageApi.success(t('runs.retryCreated'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const taskMap = useMemo(() => {
    const map = new Map<string, TaskSummaryRecord>();
    (taskSummariesQuery.data || []).forEach((task) => map.set(task.id, task));
    return map;
  }, [taskSummariesQuery.data]);

  const selectedRun = useMemo(() => {
    if (!selectedRunId) {
      return null;
    }
    return (
      selectedRunQuery.data ||
      (runsQuery.data?.items || []).find((run) => run.id === selectedRunId) ||
      null
    );
  }, [selectedRunId, selectedRunQuery.data, runsQuery.data]);
  const selectedRunEvents = runEventsQuery.data || [];
  const selectedToolCalls = useMemo(
    () => collectToolCalls(selectedRunEvents, selectedRun?.report),
    [selectedRun?.report, selectedRunEvents],
  );
  const selectedToolResults = useMemo(
    () => collectToolResults(selectedRunEvents),
    [selectedRunEvents],
  );
  const selectedModelRequests = useMemo(
    () =>
      selectedRunEvents.filter((event) => event.event_type === 'model_request'),
    [selectedRunEvents],
  );
  const selectedStreamStats = useMemo(
    () => summarizeStreamEvents(selectedRunEvents),
    [selectedRunEvents],
  );

  const taskOptions = useMemo(
    () => {
      const map = new Map<string, { label: string; value: string }>();
      [...(taskSummariesQuery.data || []), ...(taskSearchQuery.data || [])].forEach((task) => {
        map.set(task.id, {
          label: task.title,
          value: task.id,
        });
      });
      return Array.from(map.values());
    },
    [taskSearchQuery.data, taskSummariesQuery.data],
  );

  const modelOptions = useMemo(
    () =>
      (modelsQuery.data || []).map((model) => ({
        label: `${model.name} (${model.model})`,
        value: model.id,
      })),
    [modelsQuery.data],
  );
  const modelNameMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => {
      map.set(model.id, model.name);
    });
    return map;
  }, [modelsQuery.data]);
  const remoteServerMap = useMemo(() => {
    const map = new Map<string, RemoteServerRecord>();
    (remoteServersQuery.data || []).forEach((server) => {
      map.set(server.id, server);
    });
    return map;
  }, [remoteServersQuery.data]);
  const selectedRemoteOperations = useMemo(
    () => collectRemoteToolOperations(selectedToolCalls, selectedToolResults, remoteServerMap),
    [remoteServerMap, selectedToolCalls, selectedToolResults],
  );
  const selectedRemoteOperationStats = useMemo(
    () => summarizeRemoteOperations(selectedRemoteOperations),
    [selectedRemoteOperations],
  );

  const columns: ColumnsType<TaskRunRecord> = [
    {
      title: t('runs.column.runId'),
      dataIndex: 'id',
      width: 260,
      render: (value: string) => <Typography.Text code>{value.slice(0, 12)}</Typography.Text>,
    },
    {
      title: t('runs.column.task'),
      dataIndex: 'task_id',
      render: (value: string) => (
        <Button type="link" size="small" onClick={() => navigate(`/tasks?task_id=${encodeURIComponent(value)}`)}>
          {taskMap.get(value)?.title || value}
        </Button>
      ),
    },
    {
      title: t('common.status'),
      dataIndex: 'status',
      width: 120,
      render: (status: TaskRunStatus) => (
        <Tag color={runColorMap[status]}>{runStatusLabel(status)}</Tag>
      ),
    },
    {
      title: t('runs.column.modelConfig'),
      dataIndex: 'model_config_id',
      width: 220,
      render: (value: string) => (
        <Button
          type="link"
          size="small"
          style={{ paddingInline: 0 }}
          onClick={() => navigate(`/models?model_id=${encodeURIComponent(value)}`)}
        >
          {modelNameMap.get(value) || value}
        </Button>
      ),
    },
    {
      title: t('runs.column.startedAt'),
      dataIndex: 'started_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: t('runs.column.finishedAt'),
      dataIndex: 'finished_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 220,
      render: (_, record) => (
        <Space>
          <Button
            size="small"
            onClick={() => {
              const next = new URLSearchParams(searchParams);
              next.set('run_id', record.id);
              setSearchParams(next);
            }}
          >
            {t('common.detail')}
          </Button>
          <Button
            size="small"
            disabled={record.status !== 'queued' && record.status !== 'running'}
            onClick={() => cancelRunMutation.mutate(record.id)}
          >
            {t('runs.action.cancel')}
          </Button>
          <Button
            size="small"
            disabled={record.status === 'queued' || record.status === 'running'}
            onClick={() => retryRunMutation.mutate(record.id)}
          >
            {t('runs.action.retry')}
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              {t('runs.title')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('runs.subtitle')}
            </Typography.Text>
          </Space>
          <Space>
            <Select
              allowClear
              showSearch
              filterOption={false}
              placeholder={t('runs.taskFilter')}
              style={{ width: 220 }}
              value={taskFilterId}
              options={taskOptions}
              onSearch={setTaskSearchTerm}
              onChange={(value) => {
                const next = new URLSearchParams(searchParams);
                if (value) {
                  next.set('task_id', value);
                } else {
                  next.delete('task_id');
                }
                setSearchParams(next);
              }}
            />
            <Select
              allowClear
              placeholder={t('runs.modelFilter')}
              style={{ width: 220 }}
              value={routeModelConfigId}
              options={modelOptions}
              onChange={(value) => {
                const next = new URLSearchParams(searchParams);
                if (value) {
                  next.set('model_config_id', value);
                } else {
                  next.delete('model_config_id');
                }
                setSearchParams(next);
              }}
            />
            <Segmented
              value={statusFilter}
              onChange={(value) => setStatusFilter(value as 'all' | TaskRunStatus)}
              options={runStatusOptions}
            />
            <Button
              onClick={() => {
                setStatusFilter('all');
                const next = new URLSearchParams(searchParams);
                next.delete('task_id');
                next.delete('model_config_id');
                setSearchParams(next);
              }}
            >
              {t('common.clearFilters')}
            </Button>
            <Button onClick={() => runsQuery.refetch()}>{t('common.refresh')}</Button>
          </Space>
        </Space>

        <Table<TaskRunRecord>
          rowKey="id"
          loading={runsQuery.isLoading}
          columns={columns}
          dataSource={runsQuery.data?.items || []}
          pagination={{
            current: runPage,
            pageSize: runPageSize,
            total: runsQuery.data?.total || 0,
            showSizeChanger: true,
            onChange: (page, pageSize) => {
              setRunPage(page);
              setRunPageSize(pageSize);
            },
          }}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('runs.empty')}
              />
            ),
          }}
        />
      </Space>

      <Drawer
        title={t('runs.detail.title')}
        open={Boolean(selectedRunId)}
        width={760}
        onClose={() => {
          const next = new URLSearchParams(searchParams);
          next.delete('run_id');
          setSearchParams(next);
          setSelectedRunId(null);
        }}
      >
        {selectedRun ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space>
              <Button onClick={() => navigate(`/tasks?task_id=${encodeURIComponent(selectedRun.task_id)}`)}>
                {t('runs.detail.openTask')}
              </Button>
              <Button
                disabled={selectedRun.status !== 'queued' && selectedRun.status !== 'running'}
                loading={cancelRunMutation.isPending}
                onClick={() => cancelRunMutation.mutate(selectedRun.id)}
              >
                {t('runs.detail.cancelRun')}
              </Button>
              <Button
                disabled={selectedRun.status === 'queued' || selectedRun.status === 'running'}
                loading={retryRunMutation.isPending}
                onClick={() => retryRunMutation.mutate(selectedRun.id)}
              >
                {t('runs.detail.retryWithCurrentConfig')}
              </Button>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('runs.column.runId')}>{selectedRun.id}</Descriptions.Item>
              <Descriptions.Item label={t('runs.column.task')}>
                {taskMap.get(selectedRun.task_id)?.title || selectedRun.task_id}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <Tag color={runColorMap[selectedRun.status]}>{runStatusLabel(selectedRun.status)}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.column.modelConfig')}>
                <Button
                  type="link"
                  size="small"
                  style={{ paddingInline: 0 }}
                  onClick={() =>
                    navigate(`/models?model_id=${encodeURIComponent(selectedRun.model_config_id)}`)
                  }
                >
                  {modelNameMap.get(selectedRun.model_config_id) || selectedRun.model_config_id}
                </Button>
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.column.startedAt')}>
                {selectedRun.started_at
                  ? dayjs(selectedRun.started_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.column.finishedAt')}>
                {selectedRun.finished_at
                  ? dayjs(selectedRun.finished_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.detail.resultSummary')}>
                {selectedRun.result_summary || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.detail.errorMessage')}>
                {selectedRun.error_message || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.detail.toolCallCount')}>
                {selectedToolCalls.length}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.detail.toolResultCount')}>
                {selectedToolResults.length}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.detail.modelRequestRounds')}>
                {selectedModelRequests.length}
              </Descriptions.Item>
              <Descriptions.Item label="Summary Job">
                {selectedRun.summary_job_run_id || '-'}
              </Descriptions.Item>
            </Descriptions>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('runs.detail.outputChunks')}>
                {t('runs.detail.chunkSummary', {
                  count: selectedStreamStats.chunkCount,
                  chars: selectedStreamStats.chunkChars,
                })}
              </Descriptions.Item>
              <Descriptions.Item label={t('runs.detail.thinkingChunks')}>
                {t('runs.detail.chunkSummary', {
                  count: selectedStreamStats.thinkingCount,
                  chars: selectedStreamStats.thinkingChars,
                })}
              </Descriptions.Item>
            </Descriptions>

            {selectedRemoteOperations.length ? (
              <div>
                <Space
                  style={{ justifyContent: 'space-between', width: '100%', marginBottom: 12 }}
                  align="start"
                >
                  <Space direction="vertical" size={0}>
                    <Typography.Title level={5} style={{ margin: 0 }}>
                      {t('runs.remote.title')}
                    </Typography.Title>
                    <Typography.Text type="secondary">
                      {t('runs.remote.description')}
                    </Typography.Text>
                  </Space>
                  <Button size="small" onClick={() => navigate('/servers')}>
                    {t('runs.remote.manageServers')}
                  </Button>
                </Space>

                <Space size="large" wrap style={{ marginBottom: 12 }}>
                  <Statistic title={t('tasks.detail.remoteOperationCount')} value={selectedRemoteOperationStats.total} />
                  <Statistic title={t('tasks.detail.involvedServers')} value={selectedRemoteOperationStats.serverCount} />
                  <Statistic title={t('tasks.detail.success')} value={selectedRemoteOperationStats.successCount} />
                  <Statistic title={t('tasks.detail.failed')} value={selectedRemoteOperationStats.failedCount} />
                </Space>

                <Collapse
                  ghost
                  items={selectedRemoteOperations.map((operation, index) => ({
                    key: `${operation.toolCallId || operation.name}-${index}`,
                    label: (
                      <Space wrap>
                        <Tag color={operation.success ? 'success' : 'error'}>
                          {operation.success ? t('common.success') : t('common.failed')}
                        </Tag>
                        <Typography.Text strong>{operation.name}</Typography.Text>
                        {operation.connectionName ? (
                          <Button
                            type="link"
                            size="small"
                            style={{ paddingInline: 0 }}
                            onClick={(event) => {
                              event.preventDefault();
                              if (!operation.connectionId) {
                                navigate('/servers');
                                return;
                              }
                              navigate(
                                `/servers?server_id=${encodeURIComponent(operation.connectionId)}`,
                              );
                            }}
                          >
                            {operation.connectionName}
                          </Button>
                        ) : operation.connectionId ? (
                          <Typography.Text code>{operation.connectionId.slice(0, 12)}</Typography.Text>
                        ) : null}
                        {operation.summary ? (
                          <Typography.Text type="secondary">{operation.summary}</Typography.Text>
                        ) : null}
                      </Space>
                    ),
                    children: (
                      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                        <Descriptions bordered column={1} size="small">
                          <Descriptions.Item label={t('runs.remote.operation')}>{operation.name}</Descriptions.Item>
                          <Descriptions.Item label={t('tasks.detail.server')}>
                            {operation.connectionName || operation.connectionId || '-'}
                          </Descriptions.Item>
                          <Descriptions.Item label={t('tasks.detail.host')}>
                            {formatRemoteEndpoint(
                              operation.username,
                              operation.host,
                              operation.port,
                            ) || '-'}
                          </Descriptions.Item>
                          <Descriptions.Item label={t('runs.remote.command')}>
                            {operation.command || '-'}
                          </Descriptions.Item>
                          <Descriptions.Item label={t('runs.remote.path')}>
                            {operation.path || '-'}
                          </Descriptions.Item>
                          <Descriptions.Item label={t('tasks.detail.remoteHost')}>
                            {operation.remoteHost || '-'}
                          </Descriptions.Item>
                          <Descriptions.Item label={t('runs.remote.outputTruncated')}>
                            {operation.outputTruncated === undefined
                              ? '-'
                              : operation.outputTruncated
                                ? t('common.yes')
                                : t('common.no')}
                          </Descriptions.Item>
                          <Descriptions.Item label={t('runs.remote.volume')}>
                            {formatRemoteVolume(operation)}
                          </Descriptions.Item>
                        </Descriptions>

                        {operation.content ? (
                          <div>
                            <Typography.Text strong>{t('runs.detail.resultSummary')}</Typography.Text>
                            <CodeParagraph value={operation.content} />
                          </div>
                        ) : null}

                        {operation.output ? (
                          <div>
                            <Typography.Text strong>{t('runs.remote.commandOutput')}</Typography.Text>
                            <CodeParagraph value={operation.output} />
                          </div>
                        ) : null}

                        {operation.result !== undefined ? (
                          <div>
                            <Typography.Text strong>{t('runs.remote.structuredResult')}</Typography.Text>
                            <CodeParagraph value={operation.result} />
                          </div>
                        ) : null}
                      </Space>
                    ),
                  }))}
                />
              </div>
            ) : null}

            <div>
              <Typography.Title level={5}>{t('runs.tools.plan')}</Typography.Title>
              {selectedToolCalls.length ? (
                <List
                  bordered
                  dataSource={selectedToolCalls}
                  renderItem={(toolCall) => (
                    <List.Item>
                      <Space direction="vertical" size={8} style={{ width: '100%' }}>
                        <Space wrap>
                          <Tag color="processing">{toolCall.name}</Tag>
                          <Typography.Text code>
                            {toolCall.callId || 'no-call-id'}
                          </Typography.Text>
                        </Space>
                        {toolCall.arguments ? (
                          <CodeParagraph value={toolCall.arguments} />
                        ) : (
                          <Typography.Text type="secondary">{t('runs.tools.noArguments')}</Typography.Text>
                        )}
                      </Space>
                    </List.Item>
                  )}
                />
              ) : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('runs.tools.noCalls')} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('runs.tools.results')}</Typography.Title>
              {selectedToolResults.length ? (
                <Collapse
                  ghost
                  items={selectedToolResults.map((result, index) => ({
                    key: `${result.toolCallId || result.name}-${index}`,
                    label: (
                      <Space wrap>
                        <Tag color={result.success ? 'success' : 'error'}>
                          {result.success ? t('common.success') : t('common.failed')}
                        </Tag>
                        <Typography.Text strong>{result.name}</Typography.Text>
                        <Typography.Text code>
                          {result.toolCallId || 'no-call-id'}
                        </Typography.Text>
                      </Space>
                    ),
                    children: (
                      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                        <Typography.Text>{result.content || '-'}</Typography.Text>
                        {result.result !== undefined ? (
                          <CodeParagraph value={result.result} />
                        ) : null}
                      </Space>
                    ),
                  }))}
                />
              ) : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('runs.tools.noResults')} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('runs.modelRequests.title')}</Typography.Title>
              {selectedModelRequests.length ? (
                <Collapse
                  ghost
                  items={selectedModelRequests.map((event, index) => ({
                    key: `${event.id}-${index}`,
                    label: (
                      <Space wrap>
                        <Typography.Text strong>
                          {t('runs.modelRequests.request', { index: index + 1 })}
                        </Typography.Text>
                        <Typography.Text type="secondary">
                          {dayjs(event.created_at).format('YYYY-MM-DD HH:mm:ss')}
                        </Typography.Text>
                      </Space>
                    ),
                    children: event.payload ? (
                      <CollapsiblePayload value={event.payload} t={t} />
                    ) : (
                      <Typography.Text type="secondary">{t('runs.modelRequests.noPayload')}</Typography.Text>
                    ),
                  }))}
                />
              ) : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('runs.modelRequests.empty')} />
              )}
            </div>

            <JsonBlock title={t('runs.snapshot.input')} value={selectedRun.input_snapshot} t={t} />
            <JsonBlock
              title={t('runs.snapshot.context')}
              value={selectedRun.context_snapshot}
              collapsible
              defaultOpen={false}
              t={t}
            />
            <JsonBlock title={t('runs.snapshot.usage')} value={selectedRun.usage} t={t} />
            <JsonBlock title={t('runs.snapshot.report')} value={selectedRun.report} t={t} />

            <div>
              <Typography.Title level={5}>{t('runs.prompts.title')}</Typography.Title>
              {runPromptsQuery.data?.items.length ? (
                <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                  <List
                    bordered
                    dataSource={runPromptsQuery.data.items}
                    renderItem={(prompt: UiPromptRecord) => (
                      <List.Item
                        actions={[
                          <Button
                            key="open-prompt"
                            size="small"
                            onClick={() =>
                              navigate(
                                `/prompts?prompt_id=${encodeURIComponent(prompt.id)}&run_id=${encodeURIComponent(selectedRun.id)}`,
                              )
                            }
                          >
                            {t('common.open')}
                          </Button>,
                        ]}
                      >
                        <Space
                          direction="vertical"
                          size={2}
                          style={{ width: '100%', alignItems: 'flex-start' }}
                        >
                          <Space wrap>
                            <Typography.Text strong>
                              {prompt.title || prompt.message || prompt.kind}
                            </Typography.Text>
                            <Tag color={promptColorMap[prompt.status]}>
                              {promptStatusLabel(prompt.status)}
                            </Tag>
                            <Typography.Text code>{prompt.id.slice(0, 12)}</Typography.Text>
                          </Space>
                          {prompt.message ? (
                            <Typography.Text>{prompt.message}</Typography.Text>
                          ) : null}
                          <Typography.Text type="secondary">
                            {dayjs(prompt.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                          </Typography.Text>
                        </Space>
                      </List.Item>
                    )}
                  />
                  <Pagination
                    size="small"
                    current={runPromptPage}
                    pageSize={runPromptPageSize}
                    total={runPromptsQuery.data.total}
                    showSizeChanger
                    onChange={(page, pageSize) => {
                      setRunPromptPage(page);
                      setRunPromptPageSize(pageSize);
                    }}
                  />
                </Space>
              ) : runPromptsQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('runs.events.title')}</Typography.Title>
              {selectedRunEvents.length ? (
                <Timeline
                  items={selectedRunEvents.map((event) => ({
                    color:
                      event.event_type.includes('failed')
                        ? 'red'
                        : event.event_type.includes('cancel')
                          ? 'gray'
                          : event.event_type.includes('completed')
                            ? 'green'
                            : 'blue',
                    children: (
                      <Space direction="vertical" size={2} style={{ width: '100%' }}>
                        <Typography.Text strong>{describeRunEventType(event, t)}</Typography.Text>
                        <Typography.Text type="secondary">
                          {dayjs(event.created_at).format('YYYY-MM-DD HH:mm:ss')}
                        </Typography.Text>
                        {event.message ? <Typography.Text>{event.message}</Typography.Text> : null}
                        <RunEventPayload event={event} t={t} />
                      </Space>
                    ),
                  }))}
                />
              ) : runEventsQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
              )}
            </div>
          </Space>
        ) : selectedRunQuery.isLoading ? null : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
        )}
      </Drawer>
    </>
  );
}

function JsonBlock({
  title,
  value,
  t,
  collapsible = false,
  defaultOpen = true,
}: {
  title: string;
  value: unknown;
  t: TranslateFn;
  collapsible?: boolean;
  defaultOpen?: boolean;
}) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      {!value ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : collapsible ? (
        <Collapse
          ghost
          size="small"
          defaultActiveKey={defaultOpen ? ['content'] : []}
          items={[
            {
              key: 'content',
              label: (
                <Typography.Text type="secondary">
                  {describeStructuredValueSummary(
                    value,
                    t('runs.viewNamedPayload', { title }),
                  )}
                </Typography.Text>
              ),
              children: <CodeParagraph value={value} />,
            },
          ]}
        />
      ) : (
        <CodeParagraph value={value} />
      )}
    </div>
  );
}

function CodeParagraph({ value }: { value: unknown }) {
  return (
    <Typography.Paragraph
      style={{
        background: '#fafafa',
        padding: 12,
        borderRadius: 6,
        marginBottom: 0,
        whiteSpace: 'pre-wrap',
        fontFamily: 'monospace',
      }}
    >
      {JSON.stringify(value, null, 2)}
    </Typography.Paragraph>
  );
}

function CollapsiblePayload({
  value,
  t,
  defaultOpen = false,
}: {
  value: unknown;
  t: TranslateFn;
  defaultOpen?: boolean;
}) {
  return (
    <Collapse
      ghost
      size="small"
      defaultActiveKey={defaultOpen ? ['payload'] : []}
      items={[
        {
          key: 'payload',
          label: (
            <Typography.Text type="secondary">
              {describeStructuredValueSummary(value, t('runs.viewPayload'))}
            </Typography.Text>
          ),
          children: <CodeParagraph value={value} />,
        },
      ]}
    />
  );
}

type ToolCallView = {
  callId: string;
  name: string;
  arguments?: unknown;
};

type ToolResultView = {
  toolCallId: string;
  name: string;
  success: boolean;
  content: string;
  result?: unknown;
};

type RemoteOperationView = {
  toolCallId: string;
  name: string;
  success: boolean;
  connectionId?: string;
  connectionName?: string;
  username?: string;
  host?: string;
  port?: number;
  command?: string;
  path?: string;
  remoteHost?: string;
  output?: string;
  outputTruncated?: boolean;
  entryCount?: number;
  sourceSizeBytes?: number;
  outputChars?: number;
  maxBytes?: number;
  content?: string;
  result?: unknown;
  summary?: string;
};

function collectToolCalls(events: TaskRunEventRecord[], report: unknown): ToolCallView[] {
  const fromEvents = events
    .filter((event) => event.event_type === 'tools_start')
    .flatMap((event) => extractToolCallArray(event.payload));
  if (fromEvents.length) {
    return dedupeToolCalls(fromEvents);
  }
  const reportToolCalls = asRecord(report)?.tool_calls;
  return dedupeToolCalls(extractToolCallArray(reportToolCalls));
}

function dedupeToolCalls(items: ToolCallView[]): ToolCallView[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = `${item.callId}::${item.name}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function extractToolCallArray(value: unknown): ToolCallView[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => Boolean(item))
    .map((toolCall) => ({
      callId:
        asOptionalString(toolCall.id) ||
        asOptionalString(toolCall.call_id) ||
        asOptionalString(toolCall.tool_call_id) ||
        '',
      name:
        asOptionalString(toolCall.name) ||
        asOptionalString(asRecord(toolCall.function)?.name) ||
        'unknown_tool',
      arguments:
        parseJsonLike(
          asOptionalString(toolCall.arguments) ||
            asOptionalString(asRecord(toolCall.function)?.arguments),
        ) ?? toolCall.arguments ?? asRecord(toolCall.function)?.arguments,
    }))
    .filter((item) => item.name);
}

function collectToolResults(events: TaskRunEventRecord[]): ToolResultView[] {
  return events
    .filter((event) => event.event_type === 'tool_stream')
    .map((event) => asRecord(event.payload))
    .filter((payload): payload is Record<string, unknown> => Boolean(payload))
    .map((payload) => ({
      toolCallId: asOptionalString(payload.tool_call_id) || '',
      name: asOptionalString(payload.name) || 'unknown_tool',
      success: Boolean(payload.success) && !Boolean(payload.is_error),
      content: asOptionalString(payload.content) || '',
      result: payload.result,
    }));
}

function collectRemoteToolOperations(
  toolCalls: ToolCallView[],
  toolResults: ToolResultView[],
  remoteServerMap: Map<string, { id: string; name: string; host: string; port: number; username: string }>,
): RemoteOperationView[] {
  const toolCallMap = new Map<string, ToolCallView>();
  toolCalls.forEach((toolCall) => {
    toolCallMap.set(`${toolCall.callId}::${toolCall.name}`, toolCall);
  });

  return toolResults
    .filter((result) => isRemoteToolName(result.name))
    .map((result) => {
      const toolCall = toolCallMap.get(`${result.toolCallId}::${result.name}`);
      const toolCallArgs = asRecord(toolCall?.arguments);
      const structured = asRecord(result.result);
      const nestedResult = asRecord(structured?.result);
      const connectionId =
        asOptionalString(structured?.connection_id) ||
        asOptionalString(toolCallArgs?.connection_id);
      const remoteServer = connectionId ? remoteServerMap.get(connectionId) : undefined;
      const name =
        asOptionalString(structured?.name) || remoteServer?.name || result.name;
      const username =
        asOptionalString(structured?.username) || remoteServer?.username;
      const host = asOptionalString(structured?.host) || remoteServer?.host;
      const port = asOptionalNumber(structured?.port) || remoteServer?.port;
      const command =
        asOptionalString(structured?.command) || asOptionalString(toolCallArgs?.command);
      const path =
        asOptionalString(structured?.path) || asOptionalString(toolCallArgs?.path);
      const remoteHost = asOptionalString(nestedResult?.remote_host);
      const output = asOptionalString(structured?.output);
      const outputTruncated = asOptionalBoolean(structured?.output_truncated);
      const entryCount = asOptionalNumber(structured?.count);
      const sourceSizeBytes = asOptionalNumber(structured?.source_size_bytes);
      const outputChars = asOptionalNumber(structured?.output_chars);
      const maxBytes = asOptionalNumber(structured?.max_bytes);

      return {
        toolCallId: result.toolCallId,
        name: result.name,
        success: result.success,
        connectionId,
        connectionName: name,
        username,
        host,
        port,
        command,
        path,
        remoteHost,
        output,
        outputTruncated,
        entryCount,
        sourceSizeBytes,
        outputChars,
        maxBytes,
        content: result.content,
        result: result.result,
        summary: summarizeRemoteOperation(result.name, command, path, outputChars, entryCount),
      };
    });
}

function summarizeRemoteOperations(items: RemoteOperationView[]) {
  const serverIds = new Set(items.map((item) => item.connectionId).filter(Boolean));
  const successCount = items.filter((item) => item.success).length;
  return {
    total: items.length,
    serverCount: serverIds.size,
    successCount,
    failedCount: items.length - successCount,
  };
}

function isRemoteToolName(name: string): boolean {
  return (
    name === 'list_connections' ||
    name === 'test_connection' ||
    name === 'run_command' ||
    name === 'list_directory' ||
    name === 'read_file'
  );
}

function summarizeRemoteOperation(
  name: string,
  command?: string,
  path?: string,
  outputChars?: number,
  entryCount?: number,
): string | undefined {
  if (name === 'run_command' && command) {
    return command;
  }
  if ((name === 'list_directory' || name === 'read_file') && path) {
    return path;
  }
  if (name === 'list_connections') {
    return entryCount === undefined ? undefined : `${entryCount} connections`;
  }
  if (name === 'run_command' && outputChars !== undefined) {
    return `${outputChars} chars`;
  }
  return undefined;
}

function summarizeStreamEvents(events: TaskRunEventRecord[]) {
  let chunkCount = 0;
  let chunkChars = 0;
  let thinkingCount = 0;
  let thinkingChars = 0;

  events.forEach((event) => {
    const payload = asRecord(event.payload);
    const chunkCountValue = asOptionalNumber(payload?.chunk_count) || 1;
    const chunkCharsValue =
      asOptionalNumber(payload?.chunk_chars) ||
      (asOptionalString(payload?.text) || asOptionalString(payload?.chunk) || '').length;
    if (event.event_type === 'chunk') {
      chunkCount += chunkCountValue;
      chunkChars += chunkCharsValue;
    }
    if (event.event_type === 'thinking') {
      thinkingCount += chunkCountValue;
      thinkingChars += chunkCharsValue;
    }
  });

  return {
    chunkCount,
    chunkChars,
    thinkingCount,
    thinkingChars,
  };
}

function describeRunEventType(event: TaskRunEventRecord, t: TranslateFn): string {
  if (event.event_type === 'chunk') {
    return t('runs.event.modelReply');
  }
  if (event.event_type === 'thinking') {
    return t('runs.event.thinking');
  }
  return event.event_type;
}

function RunEventPayload({ event, t }: { event: TaskRunEventRecord; t: TranslateFn }) {
  const payload = asRecord(event.payload);
  const aggregatedText = asOptionalString(payload?.text);
  if (
    (event.event_type === 'chunk' || event.event_type === 'thinking') &&
    aggregatedText !== undefined
  ) {
    const aggregatedCount = asOptionalNumber(payload?.chunk_count) || 1;
    const aggregatedChars = asOptionalNumber(payload?.chunk_chars) || aggregatedText.length;
    return (
      <Space direction="vertical" size={8} style={{ width: '100%' }}>
        <Typography.Text type="secondary">
          {t('runs.event.fragmentSummary', {
            count: aggregatedCount,
            chars: aggregatedChars,
          })}
        </Typography.Text>
        <Typography.Paragraph
          style={{
            background: '#fafafa',
            padding: 12,
            borderRadius: 6,
            marginBottom: 0,
            whiteSpace: 'pre-wrap',
          }}
          ellipsis={{ rows: 8, expandable: 'collapsible' }}
        >
          {aggregatedText || '(empty)'}
        </Typography.Paragraph>
      </Space>
    );
  }

  if (!event.payload) {
    return null;
  }

  return <CollapsiblePayload value={event.payload} t={t} />;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asOptionalString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const text = value.trim();
  return text ? text : undefined;
}

function asOptionalNumber(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  return undefined;
}

function asOptionalBoolean(value: unknown): boolean | undefined {
  if (typeof value === 'boolean') {
    return value;
  }
  return undefined;
}

function describeStructuredValueSummary(value: unknown, labelPrefix: string): string {
  if (Array.isArray(value)) {
    return `${labelPrefix} (${value.length} items)`;
  }
  if (value && typeof value === 'object') {
    return `${labelPrefix} (${Object.keys(value as Record<string, unknown>).length} keys)`;
  }
  if (typeof value === 'string') {
    return `${labelPrefix} (${value.length} chars)`;
  }
  return labelPrefix;
}

function formatRemoteEndpoint(
  username?: string,
  host?: string,
  port?: number,
): string | undefined {
  if (!host) {
    return undefined;
  }
  const userPrefix = username ? `${username}@` : '';
  const portSuffix = port ? `:${port}` : '';
  return `${userPrefix}${host}${portSuffix}`;
}

function formatRemoteVolume(operation: RemoteOperationView): string {
  if (operation.entryCount !== undefined) {
    return `${operation.entryCount} entries`;
  }
  if (operation.sourceSizeBytes !== undefined) {
    return `${operation.sourceSizeBytes} bytes`;
  }
  if (operation.outputChars !== undefined) {
    return `${operation.outputChars} chars`;
  }
  if (operation.maxBytes !== undefined) {
    return `limit ${operation.maxBytes} bytes`;
  }
  return '-';
}

function parseJsonLike(value: string | undefined): unknown {
  if (!value) {
    return undefined;
  }
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

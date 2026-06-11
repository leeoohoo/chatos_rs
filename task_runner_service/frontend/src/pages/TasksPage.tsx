import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Button,
  Checkbox,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  List,
  Modal,
  Select,
  Segmented,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { TableRowSelection } from 'antd/es/table/interface';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { McpPromptPreviewCard } from '../components/McpPromptPreviewCard';
import { useI18n, type TranslateFn } from '../i18n/I18nProvider';
import type {
  BatchTaskOperationResponse,
  CreateTaskPayload,
  EngineRecord,
  McpCatalogEntry,
  RemoteServerRecord,
  StartTaskRunPayload,
  TaskBuiltinPromptMode,
  TaskMcpConfig,
  TaskMcpInitMode,
  TaskRunEventRecord,
  TaskRunRecord,
  TaskScheduleConfig,
  TaskScheduleMode,
  TaskRecord,
  TaskStatus,
  UiPromptRecord,
  UiPromptStatus,
} from '../types';

type TaskFormValues = {
  title: string;
  objective: string;
  description?: string;
  priority?: number;
  status: TaskStatus;
  default_model_config_id?: string;
  prerequisite_task_ids?: string[];
  tagsText?: string;
  mcpEnabled: boolean;
  mcpInitMode: TaskMcpInitMode;
  builtinPromptMode: TaskBuiltinPromptMode;
  builtinPromptLocale: string;
  enabledBuiltinKinds: string[];
  workspaceDir?: string;
  defaultRemoteServerId?: string;
  scheduleMode: TaskScheduleMode;
  scheduleRunAt?: string;
  scheduleIntervalSeconds?: number;
};

type RunTaskFormValues = {
  model_config_id?: string;
  prompt_override?: string;
};

const statusColorMap: Record<TaskStatus, string> = {
  draft: 'default',
  ready: 'blue',
  running: 'processing',
  succeeded: 'success',
  failed: 'error',
  blocked: 'warning',
  cancelled: 'default',
  archived: 'default',
};

const taskStatusValues: TaskStatus[] = [
  'draft',
  'ready',
  'running',
  'succeeded',
  'failed',
  'blocked',
  'cancelled',
  'archived',
];

const statusFilterValues: Array<'all' | TaskStatus> = [
  'all',
  'draft',
  'ready',
  'running',
  'succeeded',
  'failed',
];

const runStatusColorMap: Record<TaskRunRecord['status'], string> = {
  queued: 'default',
  running: 'processing',
  succeeded: 'success',
  failed: 'error',
  cancelled: 'default',
  blocked: 'warning',
};

const scheduleModeLabelKeys: Record<TaskScheduleMode, string> = {
  manual: 'tasks.schedule.manual',
  once: 'tasks.schedule.once',
  interval: 'tasks.schedule.interval',
};

const scheduleModeDescriptionKeys: Record<TaskScheduleMode, string> = {
  manual: 'tasks.schedule.manualDescription',
  once: 'tasks.schedule.onceDescription',
  interval: 'tasks.schedule.intervalDescription',
};

const promptStatusColorMap: Record<UiPromptStatus, string> = {
  pending: 'processing',
  submitted: 'success',
  cancelled: 'default',
  timed_out: 'warning',
  failed: 'error',
};

function taskCreatorLabel(task: TaskRecord): string {
  const displayName = task.creator_display_name?.trim();
  const username = task.creator_username?.trim();
  if (displayName && username && displayName !== username) {
    return `${displayName} (${username})`;
  }
  return displayName || username || '-';
}

function taskModelOptionLabel(
  model: {
    name: string;
    model: string;
    usage_scenario?: string | null;
    enabled?: boolean;
  },
  t: TranslateFn,
): string {
  const parts = [`${model.name} (${model.model})`];
  const usageScenario = model.usage_scenario?.trim();
  if (usageScenario) {
    parts.push(usageScenario);
  }
  let label = parts.join(' - ');
  if (model.enabled === false) {
    label = `${label} / ${t('common.disabled')}`;
  }
  return label;
}

export function TasksPage() {
  const { locale, t } = useI18n();
  const DEFAULT_PAGE_SIZE = 8;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingTask, setEditingTask] = useState<TaskRecord | null>(null);
  const [runningTask, setRunningTask] = useState<TaskRecord | null>(null);
  const [batchRunTaskIds, setBatchRunTaskIds] = useState<string[]>([]);
  const [detailTaskId, setDetailTaskId] = useState<string | null>(null);
  const [detailTaskPreview, setDetailTaskPreview] = useState<TaskRecord | null>(null);
  const [memoryTask, setMemoryTask] = useState<TaskRecord | null>(null);
  const [draftMcpPreviewOpen, setDraftMcpPreviewOpen] = useState(false);
  const [mcpPreviewTask, setMcpPreviewTask] = useState<TaskRecord | null>(null);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([]);
  const [statusFilter, setStatusFilter] = useState<'all' | TaskStatus>('all');
  const [keywordFilter, setKeywordFilter] = useState('');
  const [tagFilter, setTagFilter] = useState<string | undefined>(undefined);
  const [scheduledOnly, setScheduledOnly] = useState(false);
  const [taskPage, setTaskPage] = useState(1);
  const [taskPageSize, setTaskPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [memoryRoleFilter, setMemoryRoleFilter] = useState<
    'all' | 'user' | 'assistant' | 'tool' | 'system'
  >('all');
  const [memorySummaryFilter, setMemorySummaryFilter] = useState<'all' | 'pending' | 'done'>(
    'all',
  );
  const [memoryLimit, setMemoryLimit] = useState<number>(50);
  const [form] = Form.useForm<TaskFormValues>();
  const [runForm] = Form.useForm<RunTaskFormValues>();
  const [batchRunForm] = Form.useForm<RunTaskFormValues>();
  const mcpEnabled = Form.useWatch('mcpEnabled', form);
  const enabledBuiltinKinds = Form.useWatch('enabledBuiltinKinds', form) || [];
  const defaultRemoteServerId = Form.useWatch('defaultRemoteServerId', form);
  const scheduleMode = Form.useWatch('scheduleMode', form);
  const effectiveScheduleMode = scheduleMode ?? 'manual';
  const scheduleModeLabels = useMemo(
    () => (Object.fromEntries(
      (['manual', 'once', 'interval'] as TaskScheduleMode[])
        .map((value) => [value, t(scheduleModeLabelKeys[value])]),
    ) as Record<TaskScheduleMode, string>),
    [t],
  );
  const scheduleModeDescriptions = useMemo(
    () => (Object.fromEntries(
      (['manual', 'once', 'interval'] as TaskScheduleMode[])
        .map((value) => [value, t(scheduleModeDescriptionKeys[value])]),
    ) as Record<TaskScheduleMode, string>),
    [t],
  );
  const scheduleModeOptions = useMemo(
    () => (['manual', 'once', 'interval'] as TaskScheduleMode[]).map((value) => ({
      label: scheduleModeLabels[value],
      value,
    })),
    [scheduleModeLabels],
  );
  const taskStatusOptions = useMemo(
    () => taskStatusValues.map((value) => ({
      label: t(`tasks.status.${value}`),
      value,
    })),
    [t],
  );
  const statusFilterOptions = useMemo(
    () => statusFilterValues.map((value) => ({
      label: t(`tasks.status.${value}`),
      value,
    })),
    [t],
  );
  const taskStatusLabel = (status: TaskStatus) => t(`tasks.status.${status}`);
  const routeTaskId = searchParams.get('task_id');
  const routeModelConfigId = searchParams.get('model_config_id') || undefined;

  const tasksQuery = useQuery({
    queryKey: [
      'tasks',
      statusFilter,
      keywordFilter,
      tagFilter,
      routeModelConfigId,
      scheduledOnly,
      taskPage,
      taskPageSize,
    ],
    queryFn: () =>
      api.listTasksPage({
        status: statusFilter === 'all' ? undefined : statusFilter,
        keyword: keywordFilter.trim() || undefined,
        tag: tagFilter,
        model_config_id: routeModelConfigId,
        scheduled_only: scheduledOnly || undefined,
        limit: taskPageSize,
        offset: (taskPage - 1) * taskPageSize,
      }),
  });
  const taskStatsQuery = useQuery({
    queryKey: ['task-stats'],
    queryFn: api.getTaskStats,
  });
  const taskIndexQuery = useQuery({
    queryKey: ['task-index'],
    queryFn: api.getTaskIndex,
  });
  const selectedTaskQuery = useQuery({
    queryKey: ['task', detailTaskId],
    queryFn: () => api.getTask(detailTaskId!),
    enabled: Boolean(detailTaskId),
  });
  const taskRecentRunsQuery = useQuery({
    queryKey: ['task-recent-runs', detailTaskId],
    queryFn: () => api.listTaskRuns(detailTaskId!, { limit: 5 }),
    enabled: Boolean(detailTaskId),
  });
  const detailLastRunId = selectedTaskQuery.data?.last_run_id ?? detailTaskPreview?.last_run_id;
  const detailLastRunQuery = useQuery({
    queryKey: ['task-detail-last-run', detailLastRunId],
    queryFn: () => api.getRun(detailLastRunId!),
    enabled: Boolean(detailLastRunId),
  });
  const detailLastRunEventsQuery = useQuery({
    queryKey: ['task-detail-last-run-events', detailLastRunId],
    queryFn: () => api.getRunEvents(detailLastRunId!),
    enabled: Boolean(detailLastRunId),
  });
  const taskFollowUpQuery = useQuery({
    queryKey: ['task-follow-ups', detailTaskId],
    queryFn: () =>
      api.listTasks({
        parent_task_id: detailTaskId!,
        limit: 50,
      }),
    enabled: Boolean(detailTaskId),
  });
  const taskRunDerivedQuery = useQuery({
    queryKey: ['task-run-derived', detailLastRunId],
    queryFn: () =>
      api.listTasks({
        source_run_id: detailLastRunId!,
        limit: 50,
      }),
    enabled: Boolean(detailLastRunId),
  });
  const taskPromptsQuery = useQuery({
    queryKey: ['task-prompts', detailTaskId],
    queryFn: () =>
      api.listPromptsPage({
        taskId: detailTaskId!,
        limit: 6,
        offset: 0,
      }),
    enabled: Boolean(detailTaskId),
  });
  const modelsQuery = useQuery({
    queryKey: ['model-configs'],
    queryFn: api.listModelConfigs,
  });
  const mcpCatalogQuery = useQuery({
    queryKey: ['mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });
  const remoteServersQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const pendingPromptTaskCountsQuery = useQuery({
    queryKey: ['prompt-task-counts', 'pending'],
    queryFn: () => api.listPromptTaskCounts({ status: 'pending' }),
  });
  const taskMemoryContextQuery = useQuery({
    queryKey: ['task-memory-context', memoryTask?.id],
    queryFn: () =>
      api.getTaskMemoryContext(memoryTask!.id, {
        include_recent_records: true,
        include_thread_summary: true,
        include_subject_memory: false,
        recent_record_limit: 12,
        summary_limit: 6,
      }),
    enabled: Boolean(memoryTask),
  });
  const taskMemoryRecordsQuery = useQuery({
    queryKey: ['task-memory-records', memoryTask?.id, memoryRoleFilter, memorySummaryFilter, memoryLimit],
    queryFn: () =>
      api.getTaskMemoryRecords(memoryTask!.id, {
        role: memoryRoleFilter === 'all' ? undefined : memoryRoleFilter,
        summary_status: memorySummaryFilter === 'all' ? undefined : memorySummaryFilter,
        limit: memoryLimit,
        offset: 0,
        order: 'desc',
      }),
    enabled: Boolean(memoryTask),
  });
  const taskMcpPromptPreviewQuery = useQuery({
    queryKey: ['task-mcp-prompt-preview', mcpPreviewTask?.id],
    queryFn: () => api.previewTaskMcpPrompt(mcpPreviewTask!.id),
    enabled: Boolean(mcpPreviewTask),
  });
  const visibleTaskLastRunIds = useMemo(
    () =>
      Array.from(
        new Set(
          (tasksQuery.data?.items || [])
            .map((task) => task.last_run_id)
            .filter((value): value is string => Boolean(value)),
        ),
      ),
    [tasksQuery.data?.items],
  );
  const taskListLastRunEventQueries = useQueries({
    queries: visibleTaskLastRunIds.map((runId) => ({
      queryKey: ['task-list-last-run-events', runId],
      queryFn: () => api.getRunEvents(runId),
      enabled: Boolean(runId),
    })),
  });

  useEffect(() => {
    if (!tasksQuery.data) {
      return;
    }
    const visibleIds = new Set(tasksQuery.data.items.map((task) => task.id));
    setSelectedTaskIds((current) => current.filter((taskId) => visibleIds.has(taskId)));
  }, [tasksQuery.data]);

  useEffect(() => {
    setTaskPage(1);
  }, [statusFilter, keywordFilter, tagFilter, routeModelConfigId, scheduledOnly]);

  useEffect(() => {
    if (routeTaskId) {
      setDetailTaskId(routeTaskId);
      setDetailTaskPreview((current) => {
        if (current?.id === routeTaskId) {
          return current;
        }
        return tasksQuery.data?.items.find((task) => task.id === routeTaskId) || null;
      });
      return;
    }
    setDetailTaskId(null);
    setDetailTaskPreview(null);
  }, [routeTaskId, tasksQuery.data]);

  const createTaskMutation = useMutation({
    mutationFn: api.createTask,
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.created'));
      closeTaskDrawer();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const updateTaskMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: Partial<CreateTaskPayload> }) =>
      api.updateTask(id, payload),
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.updated'));
      closeTaskDrawer();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const deleteTaskMutation = useMutation({
    mutationFn: api.deleteTask,
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.deleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const runTaskMutation = useMutation({
    mutationFn: ({ taskId, payload }: { taskId: string; payload: StartTaskRunPayload }) =>
      api.startTaskRun(taskId, payload),
    onSuccess: async () => {
      await invalidateTaskQueries();
      messageApi.success(t('tasks.started'));
      closeRunModal();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const batchUpdateTaskStatusMutation = useMutation({
    mutationFn: api.batchUpdateTaskStatus,
    onSuccess: async (result, payload) => {
      await invalidateTaskQueries();
      setSelectedTaskIds([]);
      showBatchOperationResult(t('tasks.batchUpdateAction', { status: payload.status }), result);
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const batchDeleteTasksMutation = useMutation({
    mutationFn: api.batchDeleteTasks,
    onSuccess: async (result) => {
      await invalidateTaskQueries();
      setSelectedTaskIds([]);
      showBatchOperationResult(t('tasks.batchDeleteAction'), result);
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const batchStartTaskRunsMutation = useMutation({
    mutationFn: api.batchStartTaskRuns,
    onSuccess: async (result) => {
      await invalidateTaskQueries();
      setSelectedTaskIds([]);
      closeBatchRunModal();
      showBatchOperationResult(t('tasks.batchRunAction'), result);
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const summarizeTaskMemoryMutation = useMutation({
    mutationFn: api.summarizeTaskMemory,
    onSuccess: async (_, taskId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-memory-context', taskId] }),
        queryClient.invalidateQueries({ queryKey: ['task-memory-records', taskId] }),
      ]);
      messageApi.success(t('tasks.memorySummarizeStarted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const draftMcpPreviewMutation = useMutation({
    mutationFn: api.previewMcpPrompt,
    onError: (error: Error) => messageApi.error(error.message),
  });

  const modelOptions = useMemo(
    () =>
      (modelsQuery.data || []).map((model) => ({
        label: taskModelOptionLabel(model, t),
        value: model.id,
        disabled: !model.enabled,
      })),
    [modelsQuery.data, t],
  );

  const modelNameMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => {
      map.set(model.id, model.name);
    });
    return map;
  }, [modelsQuery.data]);

  const modelLabelMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => {
      map.set(model.id, taskModelOptionLabel(model, t));
    });
    return map;
  }, [modelsQuery.data, t]);

  const taskSummaryMap = useMemo(() => {
    const map = new Map<string, string>();
    (taskIndexQuery.data?.tasks || []).forEach((task) => {
      map.set(task.id, task.title);
    });
    return map;
  }, [taskIndexQuery.data?.tasks]);

  const prerequisiteTaskOptions = useMemo(
    () =>
      (taskIndexQuery.data?.tasks || [])
        .filter((task) => task.id !== editingTask?.id)
        .map((task) => ({
          label: `${task.title} (${task.status})`,
          value: task.id,
        })),
    [editingTask?.id, taskIndexQuery.data?.tasks],
  );

  const mcpOptions = useMemo(
    () =>
      (mcpCatalogQuery.data || []).map((entry) => ({
        label: entry.kind,
        value: entry.kind,
        disabled: !entry.implemented,
        description: entry.description,
        useCases: entry.use_cases,
        capabilities: entry.capabilities,
        message: entry.message || undefined,
      })),
    [mcpCatalogQuery.data],
  );
  const tagOptions = useMemo(
    () =>
      (taskIndexQuery.data?.tags || []).map((tag) => ({
        label: tag,
        value: tag,
      })),
    [taskIndexQuery.data?.tags],
  );
  const remoteControllerEntry = useMemo(
    () =>
      (mcpCatalogQuery.data || []).find((entry) => entry.kind === 'RemoteConnectionController') ||
      null,
    [mcpCatalogQuery.data],
  );
  const enabledRemoteServerCount = useMemo(
    () => (remoteServersQuery.data || []).filter((item) => item.enabled).length,
    [remoteServersQuery.data],
  );
  const remoteServerTotalCount = remoteServersQuery.data?.length || 0;
  const remoteControllerEffectiveSelected = Boolean(
    mcpEnabled &&
      (enabledBuiltinKinds.length === 0
        ? remoteControllerEntry
        : enabledBuiltinKinds.includes('RemoteConnectionController')),
  );
  const remoteServerMap = useMemo(() => {
    const map = new Map<string, RemoteServerRecord>();
    (remoteServersQuery.data || []).forEach((server) => {
      map.set(server.id, server);
    });
    return map;
  }, [remoteServersQuery.data]);
  const remoteServerOptions = useMemo(
    () =>
      (remoteServersQuery.data || []).map((server) => ({
        label: `${server.name} (${server.host}:${server.port})${server.enabled ? '' : ' / disabled'}`,
        value: server.id,
        disabled: !server.enabled,
      })),
    [remoteServersQuery.data],
  );
  const selectedTask = useMemo(
    () => selectedTaskQuery.data || detailTaskPreview,
    [detailTaskPreview, selectedTaskQuery.data],
  );
  const detailRemoteOperations = useMemo(
    () =>
      collectTaskRemoteOperations(
        detailLastRunEventsQuery.data || [],
        remoteServerMap,
      ),
    [detailLastRunEventsQuery.data, remoteServerMap],
  );
  const detailRemoteOperationStats = useMemo(
    () => summarizeTaskRemoteOperations(detailRemoteOperations),
    [detailRemoteOperations],
  );
  const latestRemoteOperation = detailRemoteOperations.length
    ? detailRemoteOperations[detailRemoteOperations.length - 1]
    : null;
  const recentRemoteOperations = useMemo(
    () => [...detailRemoteOperations].slice(-3).reverse(),
    [detailRemoteOperations],
  );
  const taskRowRemoteActivityByTaskId = useMemo(() => {
    const runEventsByRunId = new Map<string, TaskRunEventRecord[]>();
    visibleTaskLastRunIds.forEach((runId, index) => {
      const query = taskListLastRunEventQueries[index];
      if (query?.data) {
        runEventsByRunId.set(runId, query.data);
      }
    });

    const taskActivityMap = new Map<
      string,
      ReturnType<typeof summarizeTaskRemoteOperations> & {
        latest: TaskRemoteOperationView | null;
      }
    >();
    (tasksQuery.data?.items || []).forEach((task) => {
      if (!task.last_run_id) {
        return;
      }
      const events = runEventsByRunId.get(task.last_run_id) || [];
      const operations = collectTaskRemoteOperations(events, remoteServerMap);
      if (!operations.length) {
        return;
      }
      taskActivityMap.set(task.id, {
        ...summarizeTaskRemoteOperations(operations),
        latest: operations[operations.length - 1] || null,
      });
    });
    return taskActivityMap;
  }, [remoteServerMap, taskListLastRunEventQueries, tasksQuery.data?.items, visibleTaskLastRunIds]);
  const pendingPromptCountByTaskId = useMemo(() => {
    const map = new Map<string, number>();
    (pendingPromptTaskCountsQuery.data || []).forEach((item) => {
      map.set(item.task_id, item.count);
    });
    return map;
  }, [pendingPromptTaskCountsQuery.data]);
  const batchRunTasks = useMemo(() => {
    const taskMap = new Map((tasksQuery.data?.items || []).map((task) => [task.id, task]));
    return batchRunTaskIds
      .map((taskId) => taskMap.get(taskId))
      .filter((task): task is TaskRecord => Boolean(task));
  }, [batchRunTaskIds, tasksQuery.data]);
  const hasSelectedTasks = selectedTaskIds.length > 0;
  const batchActionPending =
    batchUpdateTaskStatusMutation.isPending ||
    batchDeleteTasksMutation.isPending ||
    batchStartTaskRunsMutation.isPending;

  const columns: ColumnsType<TaskRecord> = [
    {
      title: t('tasks.column.task'),
      dataIndex: 'title',
      width: 320,
      render: (_, record) => {
        const remoteActivity = taskRowRemoteActivityByTaskId.get(record.id);
        return (
          <Space direction="vertical" size={4}>
            <Space direction="vertical" size={0}>
              <Typography.Text strong>{record.title}</Typography.Text>
              <Typography.Text type="secondary">{record.objective}</Typography.Text>
            </Space>
            <Space size={[4, 4]} wrap>
              {record.parent_task_id ? (
                <Tag color="purple">{t('tasks.followUp')}</Tag>
              ) : (
                <Tag>{t('tasks.manual')}</Tag>
              )}
              {record.parent_task_id ? (
                <Typography.Text type="secondary">
                  parent: {record.parent_task_id.slice(0, 8)}
                </Typography.Text>
              ) : null}
              {record.source_run_id ? (
                <Typography.Text type="secondary">
                  run: {record.source_run_id.slice(0, 8)}
                </Typography.Text>
              ) : null}
            </Space>
            {record.tags.length || (pendingPromptCountByTaskId.get(record.id) || 0) > 0 ? (
              <Space size={[4, 4]} wrap>
                {record.tags.map((tag) => (
                  <Tag key={tag}>{tag}</Tag>
                ))}
                {(pendingPromptCountByTaskId.get(record.id) || 0) > 0 ? (
                  <Tag color="gold">
                    {t('tasks.pendingPrompts', {
                      count: pendingPromptCountByTaskId.get(record.id) || 0,
                    })}
                  </Tag>
                ) : null}
              </Space>
            ) : null}
            {remoteActivity ? (
              <Space direction="vertical" size={0}>
                <Space size={[4, 4]} wrap>
                  <Tag color={remoteActivity.failedCount > 0 ? 'error' : 'success'}>
                    {t('tasks.remoteOperations', { count: remoteActivity.total })}
                  </Tag>
                  <Tag>{t('tasks.remoteServers', { count: remoteActivity.serverCount })}</Tag>
                  {remoteActivity.latest?.connectionName ? (
                    <Tag color="blue">{remoteActivity.latest.connectionName}</Tag>
                  ) : null}
                </Space>
                <Typography.Text type="secondary">
                  {remoteActivity.latest?.command ||
                    remoteActivity.latest?.path ||
                    remoteActivity.latest?.summary ||
                    t('tasks.remoteActivityFallback')}
                </Typography.Text>
              </Space>
            ) : null}
          </Space>
        );
      },
    },
    {
      title: t('common.status'),
      dataIndex: 'status',
      width: 120,
      render: (status: TaskStatus) => (
        <Tag color={statusColorMap[status]}>{taskStatusLabel(status)}</Tag>
      ),
    },
    {
      title: t('tasks.column.creator'),
      dataIndex: 'creator_display_name',
      width: 150,
      render: (_, record) => taskCreatorLabel(record),
    },
    {
      title: t('tasks.column.model'),
      dataIndex: 'default_model_config_id',
      width: 220,
      render: (value?: string | null) => {
        if (!value) {
          return t('tasks.modelUnbound');
        }
        return (
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0 }}
            onClick={() => navigate(`/models?model_id=${encodeURIComponent(value)}`)}
          >
            {modelNameMap.get(value) || value}
          </Button>
        );
      },
    },
    {
      title: t('tasks.column.mcp'),
      dataIndex: 'mcp_config',
      width: 220,
      render: (mcpConfig: TaskMcpConfig) => (
        <Space size={[4, 4]} wrap>
          <Tag color={mcpConfig.enabled ? 'processing' : 'default'}>
            {mcpConfig.enabled ? t('common.enabled') : t('common.disabled')}
          </Tag>
          <Tag>{mcpConfig.init_mode}</Tag>
          <Tag>{t('tasks.mcpTools', { count: mcpConfig.enabled_builtin_kinds.length })}</Tag>
        </Space>
      ),
    },
    {
      title: t('tasks.column.schedule'),
      dataIndex: 'schedule',
      width: 220,
      render: (schedule: TaskScheduleConfig) => {
        if (schedule.mode === 'manual') {
          return <Tag>{scheduleModeLabels.manual}</Tag>;
        }
        return (
          <Space direction="vertical" size={2}>
            <Space size={[4, 4]} wrap>
              <Tag color="processing">{scheduleModeLabels[schedule.mode]}</Tag>
              {schedule.interval_seconds ? <Tag>{schedule.interval_seconds}s</Tag> : null}
            </Space>
            <Typography.Text type="secondary">
              {t('tasks.schedule.next')}:{' '}
              {schedule.next_run_at
                ? dayjs(schedule.next_run_at).format('YYYY-MM-DD HH:mm:ss')
                : '-'}
            </Typography.Text>
          </Space>
        );
      },
    },
    {
      title: t('tasks.column.summary'),
      dataIndex: 'result_summary',
      render: (value?: string | null) =>
        value ? (
          <Typography.Paragraph
            type="secondary"
            ellipsis={{ rows: 2 }}
            style={{ marginBottom: 0 }}
          >
            {value}
          </Typography.Paragraph>
        ) : (
          '-'
        ),
    },
    {
      title: t('tasks.column.priority'),
      dataIndex: 'priority',
      width: 90,
    },
    {
      title: t('tasks.column.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 430,
      render: (_, record) => (
        <Space wrap>
          <Button size="small" onClick={() => openDetailDrawer(record)}>
            {t('tasks.action.detail')}
          </Button>
          <Button size="small" onClick={() => openEditDrawer(record)}>
            {t('tasks.action.edit')}
          </Button>
          <Button
            size="small"
            onClick={() => navigate(`/runs?task_id=${encodeURIComponent(record.id)}`)}
          >
            {t('tasks.action.history')}
          </Button>
          <Button
            size="small"
            onClick={() => navigate(`/prompts?task_id=${encodeURIComponent(record.id)}`)}
          >
            {t('tasks.action.prompts')}
          </Button>
          <Button size="small" onClick={() => openMemoryDrawer(record)}>
            Memory
          </Button>
          <Button
            size="small"
            type="primary"
            disabled={record.status === 'running'}
            onClick={() => openRunModal(record)}
          >
            {t('tasks.action.run')}
          </Button>
          <Button size="small" danger onClick={() => confirmDelete(record)}>
            {t('tasks.action.delete')}
          </Button>
        </Space>
      ),
    },
  ];
  const rowSelection: TableRowSelection<TaskRecord> = {
    selectedRowKeys: selectedTaskIds,
    onChange: (selectedRowKeys) => setSelectedTaskIds(selectedRowKeys.map(String)),
  };

  const memoryRecordColumns: ColumnsType<EngineRecord> = [
    {
      title: t('tasks.memory.column.time'),
      dataIndex: 'created_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('tasks.memory.column.role'),
      dataIndex: 'role',
      width: 110,
      render: (value: string) => <Tag color={memoryRoleColor(value)}>{value}</Tag>,
    },
    {
      title: t('tasks.memory.column.type'),
      dataIndex: 'record_type',
      width: 150,
      render: (value: string) => <Typography.Text code>{value}</Typography.Text>,
    },
    {
      title: t('tasks.memory.column.summaryStatus'),
      dataIndex: 'summary_status',
      width: 120,
      render: (value: string) => <Tag color={memorySummaryColor(value)}>{value}</Tag>,
    },
    {
      title: t('tasks.memory.column.content'),
      dataIndex: 'content',
      render: (value: string) => (
        <Typography.Paragraph ellipsis={{ rows: 3, expandable: true }} style={{ marginBottom: 0 }}>
          {value}
        </Typography.Paragraph>
      ),
    },
  ];

  function closeTaskDrawer() {
    setDrawerOpen(false);
    setDraftMcpPreviewOpen(false);
    setEditingTask(null);
    form.resetFields();
  }

  function closeRunModal() {
    setRunningTask(null);
    runForm.resetFields();
  }

  function closeBatchRunModal() {
    setBatchRunTaskIds([]);
    batchRunForm.resetFields();
  }

  function closeDetailDrawer() {
    setMcpPreviewTask(null);
    const next = new URLSearchParams(searchParams);
    next.delete('task_id');
    setSearchParams(next);
  }

  function closeMemoryDrawer() {
    setMemoryTask(null);
  }

  function closeTaskMcpPreviewModal() {
    setMcpPreviewTask(null);
  }

  function closeDraftMcpPreviewModal() {
    setDraftMcpPreviewOpen(false);
  }

  function openCreateDrawer() {
    setEditingTask(null);
    form.setFieldsValue({
      title: '',
      objective: '',
      description: '',
      priority: 0,
      status: 'draft',
      default_model_config_id: undefined,
      prerequisite_task_ids: [],
      tagsText: '',
      mcpEnabled: true,
      mcpInitMode: 'builtin_only',
      builtinPromptMode: 'effective',
      builtinPromptLocale: locale,
      enabledBuiltinKinds: mcpOptions.map((item) => String(item.value)),
      workspaceDir: '',
      defaultRemoteServerId: undefined,
      scheduleMode: 'manual',
      scheduleRunAt: undefined,
      scheduleIntervalSeconds: undefined,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(task: TaskRecord) {
    setEditingTask(task);
    form.setFieldsValue({
      title: task.title,
      objective: task.objective,
      description: task.description || '',
      priority: task.priority,
      status: task.status,
      default_model_config_id: task.default_model_config_id || undefined,
      prerequisite_task_ids: task.prerequisite_task_ids || [],
      tagsText: task.tags.join(', '),
      mcpEnabled: task.mcp_config.enabled,
      mcpInitMode: task.mcp_config.init_mode,
      builtinPromptMode: task.mcp_config.builtin_prompt_mode,
      builtinPromptLocale: task.mcp_config.builtin_prompt_locale,
      enabledBuiltinKinds: task.mcp_config.enabled_builtin_kinds,
      workspaceDir: task.mcp_config.workspace_dir || '',
      defaultRemoteServerId: task.mcp_config.default_remote_server_id || undefined,
      scheduleMode: task.schedule.mode,
      scheduleRunAt: formatScheduleInput(task.schedule.run_at ?? task.schedule.next_run_at),
      scheduleIntervalSeconds: task.schedule.interval_seconds || undefined,
    });
    setDrawerOpen(true);
  }

  function openDetailDrawer(task: TaskRecord) {
    setDetailTaskId(task.id);
    setDetailTaskPreview(task);
    const next = new URLSearchParams(searchParams);
    next.set('task_id', task.id);
    setSearchParams(next);
  }

  function openRunModal(task: TaskRecord) {
    setRunningTask(task);
    runForm.setFieldsValue({
      model_config_id: task.default_model_config_id || undefined,
      prompt_override: '',
    });
  }

  function openBatchRunModal() {
    if (!selectedTaskIds.length) {
      return;
    }
    setBatchRunTaskIds(selectedTaskIds);
    batchRunForm.setFieldsValue({
      model_config_id: undefined,
      prompt_override: '',
    });
  }

  function openMemoryDrawer(task: TaskRecord) {
    setMemoryTask(task);
    setMemoryRoleFilter('all');
    setMemorySummaryFilter('all');
    setMemoryLimit(50);
  }

  function openTaskMcpPreviewModal(task: TaskRecord) {
    setMcpPreviewTask(task);
  }

  function openDraftMcpPreviewModal() {
    const values = form.getFieldsValue([
      'mcpEnabled',
      'mcpInitMode',
      'builtinPromptMode',
      'builtinPromptLocale',
      'enabledBuiltinKinds',
      'workspaceDir',
      'defaultRemoteServerId',
    ]) as Partial<TaskFormValues>;
    setDraftMcpPreviewOpen(true);
    draftMcpPreviewMutation.mutate({
      enabled: values.mcpEnabled ?? true,
      init_mode: values.mcpInitMode ?? 'builtin_only',
      builtin_prompt_mode: values.builtinPromptMode ?? 'effective',
      builtin_prompt_locale: values.builtinPromptLocale || locale,
      enabled_builtin_kinds: values.enabledBuiltinKinds || [],
      workspace_dir: values.workspaceDir?.trim() || undefined,
      default_remote_server_id: values.defaultRemoteServerId,
    });
  }

  function jumpToRunHistory(taskId: string, runId?: string) {
    const search = new URLSearchParams();
    search.set('task_id', taskId);
    if (runId) {
      search.set('run_id', runId);
    }
    navigate(`/runs?${search.toString()}`);
  }

  function confirmDelete(task: TaskRecord) {
    Modal.confirm({
      title: t('tasks.deleteConfirmTitle', { title: task.title }),
      content: t('tasks.deleteConfirmContent'),
      okButtonProps: { danger: true },
      onOk: () => deleteTaskMutation.mutate(task.id),
    });
  }

  function confirmBatchDelete() {
    if (!selectedTaskIds.length) {
      return;
    }
    Modal.confirm({
      title: t('tasks.batchDeleteConfirmTitle', { count: selectedTaskIds.length }),
      content: t('tasks.batchDeleteConfirmContent'),
      okButtonProps: { danger: true },
      onOk: () => batchDeleteTasksMutation.mutate({ task_ids: selectedTaskIds }),
    });
  }

  async function invalidateTaskQueries() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['tasks'] }),
      queryClient.invalidateQueries({ queryKey: ['task-index'] }),
      queryClient.invalidateQueries({ queryKey: ['task-stats'] }),
      queryClient.invalidateQueries({ queryKey: ['task'] }),
      queryClient.invalidateQueries({ queryKey: ['task-recent-runs'] }),
      queryClient.invalidateQueries({ queryKey: ['task-prompts'] }),
      queryClient.invalidateQueries({ queryKey: ['runs'] }),
      queryClient.invalidateQueries({ queryKey: ['run-index'] }),
      queryClient.invalidateQueries({ queryKey: ['task-list-last-run-events'] }),
      queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
      queryClient.invalidateQueries({ queryKey: ['prompts'] }),
      queryClient.invalidateQueries({ queryKey: ['prompt-task-counts'] }),
    ]);
  }

  function showBatchOperationResult(action: string, result: BatchTaskOperationResponse) {
    const failedItems = result.results.filter((item) => !item.ok);
    const summary = t('tasks.batchSummary', {
      action,
      succeeded: result.succeeded,
      total: result.total,
    });
    if (!failedItems.length) {
      messageApi.success(summary);
      return;
    }

    const detail = failedItems
      .slice(0, 3)
      .map((item) => `${item.task_id.slice(0, 8)}: ${item.message || t('tasks.batchFailedFallback')}`)
      .join('；');
    const messageText = t('tasks.batchMessage', {
      summary,
      failed: result.failed,
      detail: detail ? t('tasks.batchDetailPrefix', { detail }) : '',
    });
    if (result.succeeded > 0) {
      messageApi.warning(messageText);
    } else {
      messageApi.error(messageText);
    }
  }

  function buildTaskPayload(values: TaskFormValues): CreateTaskPayload | null {
    const schedule = buildSchedulePayload(values);
    if (!schedule) {
      messageApi.error(t('tasks.scheduleInvalid'));
      return null;
    }

    return {
      title: values.title,
      objective: values.objective,
      description: values.description?.trim() || undefined,
      priority: values.priority,
      status: values.status,
      default_model_config_id: values.default_model_config_id,
      prerequisite_task_ids: values.prerequisite_task_ids || [],
      tags: values.tagsText
        ?.split(',')
        .map((item) => item.trim())
        .filter(Boolean),
      schedule,
      mcp_config: {
        enabled: values.mcpEnabled,
        init_mode: values.mcpInitMode,
        builtin_prompt_mode: values.builtinPromptMode,
        builtin_prompt_locale: values.builtinPromptLocale,
        enabled_builtin_kinds: values.enabledBuiltinKinds || [],
        workspace_dir: values.workspaceDir?.trim() || undefined,
        default_remote_server_id: values.defaultRemoteServerId,
      },
    };
  }

  function handleSubmit(values: TaskFormValues) {
    const payload = buildTaskPayload(values);
    if (!payload) {
      return;
    }

    if (editingTask) {
      updateTaskMutation.mutate({ id: editingTask.id, payload });
    } else {
      createTaskMutation.mutate(payload);
    }
  }

  function handleRunTask(values: RunTaskFormValues) {
    if (!runningTask) {
      return;
    }
    const payload: StartTaskRunPayload = {
      model_config_id: values.model_config_id,
      prompt_override: values.prompt_override?.trim() || undefined,
    };
    runTaskMutation.mutate({ taskId: runningTask.id, payload });
  }

  function handleBatchRunTask(values: RunTaskFormValues) {
    if (!batchRunTaskIds.length) {
      return;
    }
    batchStartTaskRunsMutation.mutate({
      task_ids: batchRunTaskIds,
      model_config_id: values.model_config_id,
      prompt_override: values.prompt_override?.trim() || undefined,
    });
  }

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              {t('tasks.title')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('tasks.subtitle')}
            </Typography.Text>
          </Space>
          <Space wrap>
            <Input.Search
              allowClear
              placeholder={t('tasks.searchPlaceholder')}
              style={{ width: 260 }}
              value={keywordFilter}
              onChange={(event) => setKeywordFilter(event.target.value)}
            />
            <Select
              allowClear
              placeholder={t('tasks.tagFilter')}
              style={{ width: 180 }}
              value={tagFilter}
              options={tagOptions}
              onChange={(value) => setTagFilter(value)}
            />
            <Select
              allowClear
              placeholder={t('tasks.modelFilter')}
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
              onChange={(value) => setStatusFilter(value as 'all' | TaskStatus)}
              options={statusFilterOptions}
            />
            <Space size={8}>
              <Typography.Text type="secondary">{t('tasks.scheduledOnly')}</Typography.Text>
              <Switch checked={scheduledOnly} onChange={setScheduledOnly} />
            </Space>
            <Button
              onClick={() => {
                void Promise.all([tasksQuery.refetch(), taskStatsQuery.refetch()]);
              }}
            >
              {t('common.refresh')}
            </Button>
            <Button type="primary" onClick={openCreateDrawer}>
              {t('tasks.newTask')}
            </Button>
          </Space>
        </Space>

        <div
          style={{
            display: 'grid',
            gap: 12,
            gridTemplateColumns: 'repeat(auto-fit, minmax(132px, 1fr))',
            width: '100%',
          }}
        >
          {[
            { title: t('tasks.stats.total'), value: taskStatsQuery.data?.total || 0 },
            { title: t('tasks.stats.scheduled'), value: taskStatsQuery.data?.scheduled || 0 },
            { title: t('tasks.stats.followUp'), value: taskStatsQuery.data?.follow_up || 0 },
            { title: 'Ready', value: taskStatsQuery.data?.ready || 0 },
            { title: t('tasks.stats.running'), value: taskStatsQuery.data?.running || 0 },
            { title: t('tasks.stats.succeeded'), value: taskStatsQuery.data?.succeeded || 0 },
            { title: t('tasks.stats.failed'), value: taskStatsQuery.data?.failed || 0 },
            { title: t('tasks.stats.blocked'), value: taskStatsQuery.data?.blocked || 0 },
          ].map((item) => (
            <div
              key={item.title}
              style={{
                background: '#fff',
                border: '1px solid #f0f0f0',
                borderRadius: 8,
                padding: 16,
              }}
            >
              <Statistic
                title={item.title}
                value={item.value}
                loading={taskStatsQuery.isLoading}
              />
            </div>
          ))}
        </div>

        <Space style={{ justifyContent: 'space-between', width: '100%' }} wrap>
          <Typography.Text type="secondary">
            {t('tasks.selectedCount', { count: selectedTaskIds.length })}
          </Typography.Text>
          <Space wrap>
            <Button
              disabled={!hasSelectedTasks || batchActionPending}
              loading={batchStartTaskRunsMutation.isPending}
              onClick={openBatchRunModal}
            >
              {t('tasks.batchRun')}
            </Button>
            <Button
              disabled={!hasSelectedTasks || batchActionPending}
              loading={batchUpdateTaskStatusMutation.isPending}
              onClick={() =>
                batchUpdateTaskStatusMutation.mutate({
                  task_ids: selectedTaskIds,
                  status: 'ready',
                })
              }
            >
              {t('tasks.setReady')}
            </Button>
            <Button
              disabled={!hasSelectedTasks || batchActionPending}
              loading={batchUpdateTaskStatusMutation.isPending}
              onClick={() =>
                batchUpdateTaskStatusMutation.mutate({
                  task_ids: selectedTaskIds,
                  status: 'archived',
                })
              }
            >
              {t('tasks.batchArchive')}
            </Button>
            <Button
              danger
              disabled={!hasSelectedTasks || batchActionPending}
              loading={batchDeleteTasksMutation.isPending}
              onClick={confirmBatchDelete}
            >
              {t('tasks.batchDelete')}
            </Button>
          </Space>
        </Space>

        <Table<TaskRecord>
          rowKey="id"
          rowSelection={rowSelection}
          loading={tasksQuery.isLoading}
          columns={columns}
          dataSource={tasksQuery.data?.items || []}
          pagination={{
            current: taskPage,
            pageSize: taskPageSize,
            total: tasksQuery.data?.total || 0,
            showSizeChanger: true,
            onChange: (page, pageSize) => {
              setTaskPage(page);
              setTaskPageSize(pageSize);
            },
          }}
          scroll={{ x: 1460 }}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('tasks.empty')}
              />
            ),
          }}
        />
      </Space>

      <Drawer
        title={selectedTask
          ? t('tasks.detail.titleWithName', { title: selectedTask.title })
          : t('tasks.detail.title')}
        open={Boolean(detailTaskId)}
        width={760}
        onClose={closeDetailDrawer}
      >
        {selectedTask ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Button
                onClick={() => {
                  closeDetailDrawer();
                  openEditDrawer(selectedTask);
                }}
              >
                {t('tasks.detail.editTask')}
              </Button>
              <Button
                type="primary"
                disabled={selectedTask.status === 'running'}
                onClick={() => {
                  closeDetailDrawer();
                  openRunModal(selectedTask);
                }}
              >
                {t('tasks.detail.runNow')}
              </Button>
              <Button onClick={() => jumpToRunHistory(selectedTask.id)}>
                {t('tasks.detail.allRunHistory')}
              </Button>
              <Button
                onClick={() => {
                  closeDetailDrawer();
                  openMemoryDrawer(selectedTask);
                }}
              >
                {t('tasks.detail.openMemory')}
              </Button>
              <Button onClick={() => openTaskMcpPreviewModal(selectedTask)}>
                {t('tasks.detail.previewMcpPrompt')}
              </Button>
              <Button
                onClick={() =>
                  navigate(`/prompts?task_id=${encodeURIComponent(selectedTask.id)}`)
                }
              >
                {t('tasks.detail.relatedPrompts')}
              </Button>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('tasks.detail.taskId')}>{selectedTask.id}</Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <Tag color={statusColorMap[selectedTask.status]}>
                  {taskStatusLabel(selectedTask.status)}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.column.creator')}>
                {taskCreatorLabel(selectedTask)}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.defaultModel')}>
                {selectedTask.default_model_config_id
                  ? (
                    <Button
                      type="link"
                      size="small"
                      style={{ paddingInline: 0 }}
                    onClick={() =>
                        navigate(
                          `/models?model_id=${encodeURIComponent(selectedTask.default_model_config_id!)}`,
                        )
                      }
                    >
                      {modelLabelMap.get(selectedTask.default_model_config_id) ||
                        selectedTask.default_model_config_id}
                    </Button>
                  )
                  : t('tasks.modelUnbound')}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.column.priority')}>{selectedTask.priority}</Descriptions.Item>
              <Descriptions.Item label={t('tasks.column.schedule')}>
                {describeTaskSchedule(selectedTask.schedule, t)}
              </Descriptions.Item>
              <Descriptions.Item label="前置任务">
                {selectedTask.prerequisite_task_ids.length ? (
                  <Space wrap>
                    {selectedTask.prerequisite_task_ids.map((taskId) => (
                      <Tag key={taskId}>
                        {taskSummaryMap.get(taskId) || taskId.slice(0, 8)}
                      </Tag>
                    ))}
                  </Space>
                ) : (
                  '-'
                )}
              </Descriptions.Item>
              <Descriptions.Item label="Memory Thread">
                <Typography.Text code>{selectedTask.memory_thread_id}</Typography.Text>
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.recentRun')}>
                {selectedTask.last_run_id || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.mcpWorkspace')}>
                {selectedTask.mcp_config.workspace_dir || t('tasks.detail.workspaceNotConfigured')}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.defaultServer')}>
                {selectedTask.mcp_config.default_remote_server_id
                  ? remoteServerMap.get(selectedTask.mcp_config.default_remote_server_id)?.name ||
                    selectedTask.mcp_config.default_remote_server_id
                  : t('tasks.modelUnbound')}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.createdAt')}>
                {dayjs(selectedTask.created_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.column.updatedAt')}>
                {dayjs(selectedTask.updated_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
            </Descriptions>

            <div>
              <Typography.Title level={5}>{t('tasks.detail.objective')}</Typography.Title>
              <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
                {selectedTask.objective}
              </Typography.Paragraph>
            </div>

            {selectedTask.description ? (
              <div>
                <Typography.Title level={5}>{t('tasks.detail.description')}</Typography.Title>
                <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
                  {selectedTask.description}
                </Typography.Paragraph>
              </div>
            ) : null}

            {selectedTask.process_log ? (
              <div>
                <Typography.Title level={5}>{t('tasks.detail.processLog')}</Typography.Title>
                <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
                  {selectedTask.process_log}
                </Typography.Paragraph>
              </div>
            ) : null}

            {selectedTask.result_summary ? (
              <div>
                <Typography.Title level={5}>{t('tasks.detail.latestSummary')}</Typography.Title>
                <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
                  {selectedTask.result_summary}
                </Typography.Paragraph>
              </div>
            ) : null}

            {selectedTask.task_tool_state.outcome_items.length ? (
              <div>
                <Typography.Title level={5}>{t('tasks.detail.outcomes')}</Typography.Title>
                <List
                  bordered
                  dataSource={selectedTask.task_tool_state.outcome_items}
                  renderItem={(item) => (
                    <List.Item>
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Tag color="processing">{item.kind}</Tag>
                          {item.importance ? <Tag>{item.importance}</Tag> : null}
                        </Space>
                        <Typography.Text>{item.text}</Typography.Text>
                        {item.refs.length ? (
                    <Typography.Text type="secondary">
                      refs: {item.refs.join(', ')}
                    </Typography.Text>
                        ) : null}
                      </Space>
                    </List.Item>
                  )}
                />
              </div>
            ) : null}

            {detailLastRunId ? (
              <div>
                <Space
                  style={{ justifyContent: 'space-between', width: '100%', marginBottom: 12 }}
                  align="start"
                >
                  <Space direction="vertical" size={0}>
                    <Typography.Title level={5} style={{ margin: 0 }}>
                      {t('tasks.detail.recentRemoteOperations')}
                    </Typography.Title>
                    <Typography.Text type="secondary">
                      {t('tasks.detail.remoteDescription')}
                    </Typography.Text>
                  </Space>
                  <Space>
                    <Button
                      size="small"
                      onClick={() => jumpToRunHistory(selectedTask.id, detailLastRunId)}
                    >
                      {t('tasks.detail.openRecentRun')}
                    </Button>
                    <Button size="small" onClick={() => navigate('/servers')}>
                      {t('tasks.detail.servers')}
                    </Button>
                  </Space>
                </Space>

                {detailRemoteOperations.length ? (
                  <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                    <Space size="large" wrap>
                      <Statistic title={t('tasks.detail.remoteOperationCount')} value={detailRemoteOperationStats.total} />
                      <Statistic title={t('tasks.detail.involvedServers')} value={detailRemoteOperationStats.serverCount} />
                      <Statistic title={t('tasks.detail.success')} value={detailRemoteOperationStats.successCount} />
                      <Statistic title={t('tasks.detail.failed')} value={detailRemoteOperationStats.failedCount} />
                    </Space>

                    {latestRemoteOperation ? (
                      <Descriptions bordered column={1} size="small">
                        <Descriptions.Item label={t('tasks.detail.latestOperation')}>
                          <Space wrap>
                            <Tag color={latestRemoteOperation.success ? 'success' : 'error'}>
                              {latestRemoteOperation.success
                                ? t('tasks.detail.success')
                                : t('tasks.detail.failed')}
                            </Tag>
                            <Typography.Text strong>{latestRemoteOperation.name}</Typography.Text>
                          </Space>
                        </Descriptions.Item>
                        <Descriptions.Item label={t('tasks.detail.server')}>
                          {latestRemoteOperation.connectionId ? (
                            <Button
                              type="link"
                              size="small"
                              style={{ paddingInline: 0 }}
                              onClick={() =>
                                navigate(
                                  `/servers?server_id=${encodeURIComponent(
                                    latestRemoteOperation.connectionId!,
                                  )}`,
                                )
                              }
                            >
                              {latestRemoteOperation.connectionName ||
                                latestRemoteOperation.connectionId}
                            </Button>
                          ) : (
                            latestRemoteOperation.connectionName || '-'
                          )}
                        </Descriptions.Item>
                        <Descriptions.Item label={t('tasks.detail.host')}>
                          {formatTaskRemoteEndpoint(
                            latestRemoteOperation.username,
                            latestRemoteOperation.host,
                            latestRemoteOperation.port,
                          ) || '-'}
                        </Descriptions.Item>
                        <Descriptions.Item label={t('tasks.detail.commandPath')}>
                          {latestRemoteOperation.command ||
                            latestRemoteOperation.path ||
                            latestRemoteOperation.summary ||
                            '-'}
                        </Descriptions.Item>
                        <Descriptions.Item label={t('tasks.detail.remoteHost')}>
                          {latestRemoteOperation.remoteHost || '-'}
                        </Descriptions.Item>
                        <Descriptions.Item label={t('tasks.detail.resultSummary')}>
                          {latestRemoteOperation.content || '-'}
                        </Descriptions.Item>
                      </Descriptions>
                    ) : null}

                    <List
                      bordered
                      dataSource={recentRemoteOperations}
                      renderItem={(operation) => (
                        <List.Item
                          actions={[
                            <Button
                              key="run"
                              size="small"
                              onClick={() => jumpToRunHistory(selectedTask.id, detailLastRunId)}
                            >
                              {t('tasks.detail.runDetails')}
                            </Button>,
                          ]}
                        >
                          <Space direction="vertical" size={4} style={{ width: '100%' }}>
                            <Space wrap>
                              <Tag color={operation.success ? 'success' : 'error'}>
                                {operation.success
                                  ? t('tasks.detail.success')
                                  : t('tasks.detail.failed')}
                              </Tag>
                              <Typography.Text strong>{operation.name}</Typography.Text>
                              {operation.connectionName ? (
                                <Typography.Text type="secondary">
                                  {operation.connectionName}
                                </Typography.Text>
                              ) : null}
                            </Space>
                            <Typography.Paragraph
                              type="secondary"
                              ellipsis={{ rows: 2 }}
                              style={{ marginBottom: 0 }}
                            >
                              {operation.command ||
                                operation.path ||
                                operation.summary ||
                                operation.content ||
                                t('tasks.detail.noSummary')}
                            </Typography.Paragraph>
                          </Space>
                        </List.Item>
                      )}
                    />
                  </Space>
                ) : detailLastRunEventsQuery.isLoading || detailLastRunQuery.isLoading ? null : (
                  <Empty
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    description={t('tasks.detail.noRemoteOperations')}
                  />
                )}
              </div>
            ) : null}

            <div>
              <Typography.Title level={5}>{t('tasks.detail.recentRuns')}</Typography.Title>
              {taskRecentRunsQuery.data?.length ? (
                <List
                  bordered
                  dataSource={taskRecentRunsQuery.data}
                  renderItem={(run: TaskRunRecord) => (
                    <List.Item
                      actions={[
                        <Button
                          key="open"
                          size="small"
                          onClick={() => jumpToRunHistory(selectedTask.id, run.id)}
                        >
                          {t('common.open')}
                        </Button>,
                      ]}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Typography.Text code>{run.id.slice(0, 12)}</Typography.Text>
                          <Tag color={runStatusColorMap[run.status]}>{run.status}</Tag>
                          <Typography.Text type="secondary">
                            {run.started_at
                              ? dayjs(run.started_at).format('YYYY-MM-DD HH:mm:ss')
                              : dayjs(run.created_at).format('YYYY-MM-DD HH:mm:ss')}
                          </Typography.Text>
                        </Space>
                        {run.result_summary ? (
                          <Typography.Paragraph
                            type="secondary"
                            ellipsis={{ rows: 2 }}
                            style={{ marginBottom: 0 }}
                          >
                            {run.result_summary}
                          </Typography.Paragraph>
                        ) : run.error_message ? (
                          <Typography.Text type="danger">{run.error_message}</Typography.Text>
                        ) : (
                          <Typography.Text type="secondary">{t('tasks.detail.noSummary')}</Typography.Text>
                        )}
                      </Space>
                    </List.Item>
                  )}
                />
              ) : taskRecentRunsQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noRunRecords')} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('tasks.detail.relatedPrompts')}</Typography.Title>
              {taskPromptsQuery.data?.items.length ? (
                <List
                  bordered
                  dataSource={taskPromptsQuery.data.items}
                  renderItem={(prompt: UiPromptRecord) => (
                    <List.Item
                      actions={[
                        <Button
                          key="open"
                          size="small"
                          onClick={() =>
                            navigate(
                              `/prompts?task_id=${encodeURIComponent(selectedTask.id)}&prompt_id=${encodeURIComponent(prompt.id)}`,
                            )
                          }
                        >
                          {t('common.open')}
                        </Button>,
                      ]}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Typography.Text strong>
                            {prompt.title || prompt.message || prompt.kind}
                          </Typography.Text>
                          <Tag color={promptStatusColorMap[prompt.status]}>
                            {prompt.status}
                          </Tag>
                          {prompt.run_id ? (
                            <Typography.Text code>{prompt.run_id.slice(0, 12)}</Typography.Text>
                          ) : null}
                        </Space>
                        {prompt.message ? (
                          <Typography.Paragraph
                            type="secondary"
                            ellipsis={{ rows: 2 }}
                            style={{ marginBottom: 0 }}
                          >
                            {prompt.message}
                          </Typography.Paragraph>
                        ) : null}
                        <Typography.Text type="secondary">
                          {dayjs(prompt.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                        </Typography.Text>
                      </Space>
                    </List.Item>
                  )}
                />
              ) : taskPromptsQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noPrompts')} />
              )}
              {taskPromptsQuery.data?.has_more ? (
                <Space style={{ marginTop: 12 }}>
                  <Typography.Text type="secondary">
                    {t('tasks.detail.promptVisibleCount', {
                      shown: taskPromptsQuery.data.items.length,
                      total: taskPromptsQuery.data.total,
                    })}
                  </Typography.Text>
                  <Button
                    size="small"
                    onClick={() =>
                      navigate(`/prompts?task_id=${encodeURIComponent(selectedTask.id)}`)
                    }
                  >
                    {t('tasks.detail.viewAll')}
                  </Button>
                </Space>
              ) : null}
            </div>

            <div>
              <Typography.Title level={5}>{t('tasks.detail.followUps')}</Typography.Title>
              {taskFollowUpQuery.data?.length ? (
                <List
                  bordered
                  dataSource={taskFollowUpQuery.data}
                  renderItem={(task) => (
                    <List.Item
                      actions={[
                        <Button key="detail" size="small" onClick={() => openDetailDrawer(task)}>
                          {t('tasks.action.detail')}
                        </Button>,
                        <Button key="history" size="small" onClick={() => jumpToRunHistory(task.id)}>
                          {t('tasks.action.history')}
                        </Button>,
                        <Button
                          key="run"
                          size="small"
                          type="primary"
                          disabled={task.status === 'running'}
                          onClick={() => openRunModal(task)}
                        >
                          {t('tasks.action.run')}
                        </Button>,
                      ]}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Typography.Text strong>{task.title}</Typography.Text>
                          <Tag color={statusColorMap[task.status]}>{taskStatusLabel(task.status)}</Tag>
                          {task.source_run_id ? (
                            <Typography.Text type="secondary">
                              source run: {task.source_run_id.slice(0, 12)}
                            </Typography.Text>
                          ) : null}
                        </Space>
                        <Typography.Paragraph
                          type="secondary"
                          ellipsis={{ rows: 2 }}
                          style={{ marginBottom: 0 }}
                        >
                          {task.objective}
                        </Typography.Paragraph>
                      </Space>
                    </List.Item>
                  )}
                />
              ) : taskFollowUpQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noFollowUps')} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('tasks.detail.runDerivedTasks')}</Typography.Title>
              {taskRunDerivedQuery.data?.length ? (
                <List
                  bordered
                  dataSource={taskRunDerivedQuery.data}
                  renderItem={(task) => (
                    <List.Item
                      actions={[
                        <Button key="detail" size="small" onClick={() => openDetailDrawer(task)}>
                          {t('tasks.action.detail')}
                        </Button>,
                        <Button key="history" size="small" onClick={() => jumpToRunHistory(task.id)}>
                          {t('tasks.action.history')}
                        </Button>,
                      ]}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Typography.Text strong>{task.title}</Typography.Text>
                          <Tag color={statusColorMap[task.status]}>{taskStatusLabel(task.status)}</Tag>
                          {task.parent_task_id ? (
                            <Typography.Text type="secondary">
                              parent: {task.parent_task_id.slice(0, 12)}
                            </Typography.Text>
                          ) : null}
                        </Space>
                        <Typography.Paragraph
                          type="secondary"
                          ellipsis={{ rows: 2 }}
                          style={{ marginBottom: 0 }}
                        >
                          {task.objective}
                        </Typography.Paragraph>
                      </Space>
                    </List.Item>
                  )}
                />
              ) : taskRunDerivedQuery.isLoading ? null : (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={t('tasks.detail.noDerivedTasks')}
                />
              )}
            </div>

            {selectedTask.input_payload ? (
              <JsonBlock title={t('tasks.detail.inputSnapshot')} value={selectedTask.input_payload} />
            ) : null}
          </Space>
        ) : selectedTaskQuery.isLoading ? null : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
        )}
      </Drawer>

      <Drawer
        title={editingTask ? t('tasks.drawer.edit') : t('tasks.drawer.create')}
        open={drawerOpen}
        width={820}
        destroyOnClose
        onClose={closeTaskDrawer}
        extra={
          <Space>
            <Button onClick={closeTaskDrawer}>{t('common.cancel')}</Button>
            <Button
              type="primary"
              loading={createTaskMutation.isPending || updateTaskMutation.isPending}
              onClick={() => form.submit()}
            >
              {t('common.save')}
            </Button>
          </Space>
        }
      >
        <Form<TaskFormValues> layout="vertical" form={form} onFinish={handleSubmit}>
          <Form.Item
            name="title"
            label={t('tasks.form.title')}
            rules={[{ required: true, message: t('tasks.form.titleRequired') }]}
          >
            <Input />
          </Form.Item>
          <Form.Item
            name="objective"
            label={t('tasks.form.objective')}
            rules={[{ required: true, message: t('tasks.form.objectiveRequired') }]}
          >
            <Input.TextArea rows={4} />
          </Form.Item>
          <Form.Item name="description" label={t('tasks.form.description')}>
            <Input.TextArea rows={3} />
          </Form.Item>

          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item
              name="status"
              label={t('common.status')}
              style={{ flex: '0 0 220px', minWidth: 220 }}
            >
              <Select
                style={{ width: '100%' }}
                options={taskStatusOptions}
              />
            </Form.Item>
            <Form.Item name="priority" label={t('tasks.column.priority')} style={{ width: 140 }}>
              <InputNumber style={{ width: '100%' }} />
            </Form.Item>
          </Space>

          <Form.Item name="default_model_config_id" label={t('tasks.form.defaultModel')}>
            <Select
              allowClear
              options={modelOptions}
              placeholder={t('tasks.form.modelPlaceholder')}
            />
          </Form.Item>
          <Form.Item name="prerequisite_task_ids" label="前置任务">
            <Select
              mode="multiple"
              allowClear
              showSearch
              options={prerequisiteTaskOptions}
              optionFilterProp="label"
              placeholder="选择必须先完成的任务"
            />
          </Form.Item>
          <Form.Item name="tagsText" label={t('tasks.form.tags')}>
            <Input placeholder={t('tasks.form.tagsPlaceholder')} />
          </Form.Item>

          <Typography.Title level={5} style={{ marginTop: 8 }}>
            {t('tasks.form.schedule')}
          </Typography.Title>

          <Form.Item
            name="scheduleMode"
            label={t('tasks.form.scheduleMode')}
            extra={scheduleModeDescriptions[effectiveScheduleMode]}
          >
            <Select options={scheduleModeOptions} />
          </Form.Item>

          {effectiveScheduleMode !== 'manual' ? (
            <Form.Item
              name="scheduleRunAt"
              label={
                effectiveScheduleMode === 'once'
                  ? t('tasks.form.runAt')
                  : t('tasks.form.firstRunAt')
              }
              rules={[
                {
                  required: true,
                  message:
                    effectiveScheduleMode === 'once'
                      ? t('tasks.form.runAtRequired')
                      : t('tasks.form.firstRunAtRequired'),
                },
              ]}
            >
              <Input type="datetime-local" />
            </Form.Item>
          ) : null}

          {effectiveScheduleMode === 'interval' ? (
            <Form.Item
              name="scheduleIntervalSeconds"
              label={t('tasks.form.intervalSeconds')}
              rules={[
                { required: true, message: t('tasks.form.intervalRequired') },
                {
                  validator: async (_, value) => {
                    if (value === undefined || value === null || value >= 60) {
                      return;
                    }
                    throw new Error(t('tasks.form.intervalMin'));
                  },
                },
              ]}
            >
              <InputNumber style={{ width: '100%' }} min={60} step={60} />
            </Form.Item>
          ) : null}

          <Typography.Title level={5} style={{ marginTop: 8 }}>
            {t('tasks.form.builtinMcp')}
          </Typography.Title>

          <Space style={{ marginBottom: 12 }}>
            <Button onClick={openDraftMcpPreviewModal}>{t('tasks.form.previewPrompt')}</Button>
          </Space>

          <Space size="middle" style={{ marginBottom: 16, width: '100%' }} align="start">
            <Form.Item
              name="mcpEnabled"
              label={t('tasks.form.enable')}
              valuePropName="checked"
              style={{ marginBottom: 0 }}
            >
              <Switch />
            </Form.Item>
            <Form.Item name="mcpInitMode" label={t('tasks.form.initMode')} style={{ marginBottom: 0 }}>
              <Select
                style={{ width: 180 }}
                disabled={!mcpEnabled}
                options={[
                  { label: 'builtin_only', value: 'builtin_only' },
                  { label: 'full', value: 'full' },
                  { label: 'disabled', value: 'disabled' },
                ]}
              />
            </Form.Item>
          </Space>

          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="builtinPromptMode" label={t('tasks.form.promptMode')} style={{ flex: 1 }}>
              <Select
                disabled={!mcpEnabled}
                options={[
                  { label: 'effective', value: 'effective' },
                  { label: 'configured', value: 'configured' },
                ]}
              />
            </Form.Item>
            <Form.Item name="builtinPromptLocale" label={t('mcp.promptLanguage.label')} style={{ width: 180 }}>
              <Select
                disabled={!mcpEnabled}
                options={[
                  { label: t('mcp.promptLanguage.zhCN'), value: 'zh-CN' },
                  { label: t('mcp.promptLanguage.enUS'), value: 'en-US' },
                ]}
              />
            </Form.Item>
          </Space>

          <Form.Item name="enabledBuiltinKinds" label={t('tasks.form.enabledKinds')}>
            <Checkbox.Group style={{ width: '100%' }}>
              <Space direction="vertical" style={{ width: '100%' }}>
                {mcpOptions.map((option) => (
                  <Checkbox
                    key={String(option.value)}
                    value={String(option.value)}
                    disabled={option.disabled || !mcpEnabled}
                  >
                    <Space direction="vertical" size={2}>
                      <Typography.Text>{option.label}</Typography.Text>
                      {option.description ? (
                        <Typography.Text type="secondary">{option.description}</Typography.Text>
                      ) : null}
                      {option.useCases.length || option.capabilities.length || option.message ? (
                        <Typography.Text type="secondary">
                          {[...option.useCases, ...option.capabilities].join(' / ')}
                          {option.message ? ` / ${option.message}` : ''}
                        </Typography.Text>
                      ) : null}
                    </Space>
                  </Checkbox>
                ))}
              </Space>
            </Checkbox.Group>
          </Form.Item>

          <Form.Item name="workspaceDir" label={t('tasks.form.workspaceDir')}>
            <Input
              disabled={!mcpEnabled}
              placeholder={t('tasks.form.workspacePlaceholder')}
            />
          </Form.Item>

          {remoteControllerEffectiveSelected ? (
            <Form.Item name="defaultRemoteServerId" label={t('tasks.form.defaultRemoteServer')}>
              <Select
                allowClear
                disabled={!mcpEnabled}
                options={remoteServerOptions}
                placeholder={t('tasks.form.defaultRemoteServerPlaceholder')}
              />
            </Form.Item>
          ) : null}

          {mcpCatalogQuery.data?.length ? (
            <Space direction="vertical" size={4} style={{ width: '100%' }}>
              {mcpCatalogQuery.data.map((entry: McpCatalogEntry) => (
                <Typography.Text
                  key={entry.kind}
                  type={entry.implemented ? 'secondary' : 'warning'}
                >
                  {entry.kind}: {t('tasks.mcpTools', { count: entry.available_tool_names.length })}
                  {entry.message ? `, ${entry.message}` : ''}
                </Typography.Text>
              ))}
            </Space>
          ) : null}

          {remoteControllerEffectiveSelected ? (
            <Space
              direction="vertical"
              size={4}
              style={{
                width: '100%',
                padding: 12,
                border: '1px solid #f0f0f0',
                borderRadius: 6,
                background: '#fafafa',
              }}
            >
              <Space wrap>
                <Tag color={enabledRemoteServerCount > 0 ? 'success' : 'warning'}>
                  RemoteConnectionController
                </Tag>
                <Typography.Text type="secondary">
                  {t('tasks.form.remoteServerCount', {
                    enabled: enabledRemoteServerCount,
                    total: remoteServerTotalCount,
                  })}
                </Typography.Text>
              </Space>
              <Typography.Text type="secondary">
                {defaultRemoteServerId
                  ? t('tasks.form.defaultRemoteServerBound', {
                      server: remoteServerMap.get(defaultRemoteServerId)?.name || defaultRemoteServerId,
                    })
                  : enabledRemoteServerCount > 0
                  ? t('tasks.form.defaultRemoteServerUnbound')
                  : t('tasks.form.noRemoteServers')}
              </Typography.Text>
              <Space>
                <Button size="small" onClick={() => navigate('/servers')}>
                  {t('tasks.form.manageServers')}
                </Button>
                <Button size="small" onClick={() => navigate('/mcp')}>
                  {t('tasks.form.viewMcpCatalog')}
                </Button>
              </Space>
            </Space>
          ) : null}
        </Form>
      </Drawer>

      <Modal
        title={mcpPreviewTask
          ? t('tasks.preview.titleWithName', { title: mcpPreviewTask.title })
          : t('tasks.preview.title')}
        open={Boolean(mcpPreviewTask)}
        width={860}
        footer={[
          <Button key="close" onClick={closeTaskMcpPreviewModal}>
            {t('common.close')}
          </Button>,
        ]}
        onCancel={closeTaskMcpPreviewModal}
      >
        {taskMcpPromptPreviewQuery.data ? (
          <McpPromptPreviewCard preview={taskMcpPromptPreviewQuery.data} />
        ) : taskMcpPromptPreviewQuery.isLoading ? (
          <Typography.Text type="secondary">{t('tasks.preview.loading')}</Typography.Text>
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.preview.unavailable')} />
        )}
      </Modal>

      <Modal
        title={t('tasks.preview.formTitle')}
        open={draftMcpPreviewOpen}
        width={860}
        footer={[
          <Button key="close" onClick={closeDraftMcpPreviewModal}>
            {t('common.close')}
          </Button>,
        ]}
        onCancel={closeDraftMcpPreviewModal}
      >
        {draftMcpPreviewMutation.data ? (
          <McpPromptPreviewCard preview={draftMcpPreviewMutation.data} />
        ) : draftMcpPreviewMutation.isPending ? (
          <Typography.Text type="secondary">{t('tasks.preview.loading')}</Typography.Text>
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.preview.unavailable')} />
        )}
      </Modal>

      <Drawer
        title={memoryTask
          ? t('tasks.memory.titleWithName', { title: memoryTask.title })
          : t('tasks.memory.title')}
        open={Boolean(memoryTask)}
        width={920}
        onClose={closeMemoryDrawer}
      >
        {memoryTask ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Segmented
                value={memoryRoleFilter}
                onChange={(value) =>
                  setMemoryRoleFilter(value as 'all' | 'user' | 'assistant' | 'tool' | 'system')
                }
                options={[
                  { label: t('tasks.memory.allRoles'), value: 'all' },
                  { label: 'user', value: 'user' },
                  { label: 'assistant', value: 'assistant' },
                  { label: 'tool', value: 'tool' },
                  { label: 'system', value: 'system' },
                ]}
              />
              <Segmented
                value={memorySummaryFilter}
                onChange={(value) => setMemorySummaryFilter(value as 'all' | 'pending' | 'done')}
                options={[
                  { label: t('tasks.memory.allSummaryStatuses'), value: 'all' },
                  { label: 'pending', value: 'pending' },
                  { label: 'done', value: 'done' },
                ]}
              />
              <Select
                value={memoryLimit}
                onChange={setMemoryLimit}
                style={{ width: 140 }}
                options={[
                  { label: t('tasks.memory.recentLimit', { count: 20 }), value: 20 },
                  { label: t('tasks.memory.recentLimit', { count: 50 }), value: 50 },
                  { label: t('tasks.memory.recentLimit', { count: 100 }), value: 100 },
                ]}
              />
              <Button
                onClick={() => {
                  void Promise.all([
                    taskMemoryContextQuery.refetch(),
                    taskMemoryRecordsQuery.refetch(),
                  ]);
                }}
              >
                {t('common.refresh')}
              </Button>
              <Button
                loading={summarizeTaskMemoryMutation.isPending}
                onClick={() => summarizeTaskMemoryMutation.mutate(memoryTask.id)}
              >
                {t('tasks.memory.triggerSummary')}
              </Button>
            </Space>

            {taskMemoryContextQuery.data?.thread ? (
              <>
                <Descriptions bordered column={1} size="small">
                  <Descriptions.Item label={t('tasks.detail.taskId')}>{memoryTask.id}</Descriptions.Item>
                  <Descriptions.Item label="Memory Thread">
                    <Typography.Text code>{taskMemoryContextQuery.data.memory_thread_id}</Typography.Text>
                  </Descriptions.Item>
                  <Descriptions.Item label="Tenant">
                    {taskMemoryContextQuery.data.tenant_id}
                  </Descriptions.Item>
                  <Descriptions.Item label="Subject">
                    {taskMemoryContextQuery.data.subject_id}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('tasks.memory.threadStatus')}>
                    <Tag color="processing">{taskMemoryContextQuery.data.thread.status}</Tag>
                  </Descriptions.Item>
                  <Descriptions.Item label={t('tasks.memory.summaryStatus')}>
                    <Tag color={memorySummaryColor(taskMemoryContextQuery.data.thread.summary_status)}>
                      {taskMemoryContextQuery.data.thread.summary_status}
                    </Tag>
                  </Descriptions.Item>
                  <Descriptions.Item label="Pending Records">
                    {taskMemoryContextQuery.data.thread.pending_record_count}
                  </Descriptions.Item>
                  <Descriptions.Item label="Pending Summary Tokens">
                    {taskMemoryContextQuery.data.thread.pending_summary_tokens}
                  </Descriptions.Item>
                  <Descriptions.Item label="Total Records">
                    {taskMemoryContextQuery.data.total_record_count}
                  </Descriptions.Item>
                  <Descriptions.Item label="Summary Job">
                    {taskMemoryContextQuery.data.thread.summary_job_run_id || '-'}
                  </Descriptions.Item>
                </Descriptions>

                {taskMemoryContextQuery.data.thread.metadata ? (
                  <JsonBlock title={t('tasks.memory.threadMetadata')} value={taskMemoryContextQuery.data.thread.metadata} />
                ) : null}
              </>
            ) : taskMemoryContextQuery.isLoading ? null : (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.memory.threadNotCreated')} />
            )}

            <div>
              <Typography.Title level={5}>{t('tasks.memory.contextPreview')}</Typography.Title>
              {taskMemoryContextQuery.data?.context ? (
                <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                  <Space wrap>
                    <Tag color="blue">
                      {t('tasks.memory.summaries', {
                        count: taskMemoryContextQuery.data.context.meta.summary_count,
                      })}
                    </Tag>
                    <Tag color="cyan">
                      {t('tasks.memory.recentRecords', {
                        count: taskMemoryContextQuery.data.context.meta.recent_record_count,
                      })}
                    </Tag>
                  </Space>
                  <List
                    bordered
                    dataSource={taskMemoryContextQuery.data.context.blocks}
                    renderItem={(block) => (
                      <List.Item>
                        <Space direction="vertical" size={8} style={{ width: '100%' }}>
                          <Tag color="processing" style={{ width: 'fit-content' }}>
                            {block.block_type}
                          </Tag>
                          <Typography.Paragraph
                            style={{ marginBottom: 0, whiteSpace: 'pre-wrap' }}
                          >
                            {block.text}
                          </Typography.Paragraph>
                        </Space>
                      </List.Item>
                    )}
                  />
                </Space>
              ) : taskMemoryContextQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.memory.noContext')} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('tasks.memory.records')}</Typography.Title>
              <Table<EngineRecord>
                rowKey="id"
                loading={taskMemoryRecordsQuery.isLoading}
                columns={memoryRecordColumns}
                dataSource={taskMemoryRecordsQuery.data?.items || []}
                pagination={false}
                scroll={{ x: 1180 }}
                expandable={{
                  expandedRowRender: (record) =>
                    record.structured_payload || record.metadata ? (
                      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                        {record.structured_payload ? (
                          <JsonBlock title="structured_payload" value={record.structured_payload} />
                        ) : null}
                        {record.metadata ? (
                          <JsonBlock title="metadata" value={record.metadata} />
                        ) : null}
                      </Space>
                    ) : (
                      <Typography.Text type="secondary">{t('tasks.memory.noExtraData')}</Typography.Text>
                    ),
                  rowExpandable: (record) => Boolean(record.structured_payload || record.metadata),
                }}
              />
              {!taskMemoryRecordsQuery.isLoading &&
              !taskMemoryRecordsQuery.data?.items.length ? (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={t('tasks.memory.noRecordsFilter')}
                  style={{ marginTop: 16 }}
                />
              ) : null}
            </div>
          </Space>
        ) : null}
      </Drawer>

      <Modal
        title={runningTask
          ? t('tasks.run.titleWithName', { title: runningTask.title })
          : t('tasks.run.title')}
        open={Boolean(runningTask)}
        onCancel={closeRunModal}
        onOk={() => runForm.submit()}
        confirmLoading={runTaskMutation.isPending}
        destroyOnClose
      >
        {runningTask ? (
          <Space direction="vertical" size="middle" style={{ width: '100%' }}>
            <Space direction="vertical" size={0}>
              <Typography.Text type="secondary">{t('tasks.run.objective')}</Typography.Text>
              <Typography.Paragraph style={{ marginBottom: 0 }}>
                {runningTask.objective}
              </Typography.Paragraph>
            </Space>

            <Form<RunTaskFormValues> layout="vertical" form={runForm} onFinish={handleRunTask}>
              <Form.Item name="model_config_id" label={t('tasks.run.modelConfig')}>
                <Select
                  allowClear
                  placeholder={t('tasks.run.modelPlaceholder')}
                  options={modelOptions}
                />
              </Form.Item>
              <Form.Item name="prompt_override" label="Prompt Override">
                <Input.TextArea
                  rows={5}
                  placeholder={t('tasks.run.promptPlaceholder')}
                />
              </Form.Item>
            </Form>
          </Space>
        ) : null}
      </Modal>

      <Modal
        title={batchRunTaskIds.length
          ? t('tasks.batchRun.titleWithCount', { count: batchRunTaskIds.length })
          : t('tasks.batchRun.title')}
        open={Boolean(batchRunTaskIds.length)}
        onCancel={closeBatchRunModal}
        onOk={() => batchRunForm.submit()}
        confirmLoading={batchStartTaskRunsMutation.isPending}
        destroyOnClose
      >
        {batchRunTaskIds.length ? (
          <Space direction="vertical" size="middle" style={{ width: '100%' }}>
            <Space direction="vertical" size={0}>
              <Typography.Text type="secondary">{t('tasks.batchRun.tasks')}</Typography.Text>
              <Typography.Paragraph style={{ marginBottom: 0 }}>
                {batchRunTasks.length
                  ? batchRunTasks.map((task) => task.title).join(' / ')
                  : t('tasks.batchRun.selectedFallback', { count: batchRunTaskIds.length })}
              </Typography.Paragraph>
            </Space>

            <Form<RunTaskFormValues>
              layout="vertical"
              form={batchRunForm}
              onFinish={handleBatchRunTask}
            >
              <Form.Item name="model_config_id" label={t('tasks.batchRun.overrideModel')}>
                <Select
                  allowClear
                  placeholder={t('tasks.batchRun.overrideModelPlaceholder')}
                  options={modelOptions}
                />
              </Form.Item>
              <Form.Item name="prompt_override" label={t('tasks.batchRun.overridePrompt')}>
                <Input.TextArea
                  rows={6}
                  placeholder={t('tasks.batchRun.overridePromptPlaceholder')}
                />
              </Form.Item>
            </Form>
          </Space>
        ) : null}
      </Modal>
    </>
  );
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
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
    </div>
  );
}

type TaskRemoteOperationView = {
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
  content?: string;
  summary?: string;
};

function collectTaskRemoteOperations(
  events: TaskRunEventRecord[],
  remoteServerMap: Map<string, RemoteServerRecord>,
): TaskRemoteOperationView[] {
  return events
    .filter((event) => event.event_type === 'tool_stream')
    .map((event) => taskPayloadAsRecord(event.payload))
    .filter((payload): payload is Record<string, unknown> => Boolean(payload))
    .filter((payload) => isTaskRemoteToolName(taskPayloadAsOptionalString(payload.name) || ''))
    .map((payload) => {
      const result = taskPayloadAsRecord(payload.result);
      const nestedResult = taskPayloadAsRecord(result?.result);
      const connectionId = taskPayloadAsOptionalString(result?.connection_id);
      const remoteServer = connectionId ? remoteServerMap.get(connectionId) : undefined;
      const command = taskPayloadAsOptionalString(result?.command);
      const path = taskPayloadAsOptionalString(result?.path);
      const connectionName =
        taskPayloadAsOptionalString(result?.name) || remoteServer?.name;

      return {
        name: taskPayloadAsOptionalString(payload.name) || 'unknown_tool',
        success: Boolean(payload.success) && !Boolean(payload.is_error),
        connectionId,
        connectionName,
        username:
          taskPayloadAsOptionalString(result?.username) || remoteServer?.username,
        host: taskPayloadAsOptionalString(result?.host) || remoteServer?.host,
        port: taskPayloadAsOptionalNumber(result?.port) || remoteServer?.port,
        command,
        path,
        remoteHost: taskPayloadAsOptionalString(nestedResult?.remote_host),
        content: taskPayloadAsOptionalString(payload.content),
        summary: command || path || taskPayloadAsOptionalString(payload.content),
      };
    });
}

function summarizeTaskRemoteOperations(items: TaskRemoteOperationView[]) {
  const serverIds = new Set(items.map((item) => item.connectionId).filter(Boolean));
  const successCount = items.filter((item) => item.success).length;
  return {
    total: items.length,
    serverCount: serverIds.size,
    successCount,
    failedCount: items.length - successCount,
  };
}

function isTaskRemoteToolName(name: string): boolean {
  return (
    name === 'list_connections' ||
    name === 'test_connection' ||
    name === 'run_command' ||
    name === 'list_directory' ||
    name === 'read_file'
  );
}

function taskPayloadAsRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function taskPayloadAsOptionalString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const text = value.trim();
  return text ? text : undefined;
}

function taskPayloadAsOptionalNumber(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  return undefined;
}

function formatTaskRemoteEndpoint(
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

function buildSchedulePayload(values: TaskFormValues): TaskScheduleConfig | null {
  if (values.scheduleMode === 'manual') {
    return {
      mode: 'manual',
    };
  }

  const runAtInput = values.scheduleRunAt?.trim();
  if (!runAtInput) {
    return null;
  }
  const runAt = dayjs(runAtInput);
  if (!runAt.isValid()) {
    return null;
  }

  if (values.scheduleMode === 'once') {
    return {
      mode: 'once',
      run_at: runAt.toISOString(),
    };
  }

  if (!values.scheduleIntervalSeconds || values.scheduleIntervalSeconds < 60) {
    return null;
  }

  return {
    mode: 'interval',
    run_at: runAt.toISOString(),
    interval_seconds: values.scheduleIntervalSeconds,
  };
}

function formatScheduleInput(value?: string | null): string | undefined {
  if (!value) {
    return undefined;
  }
  const parsed = dayjs(value);
  if (!parsed.isValid()) {
    return undefined;
  }
  return parsed.format('YYYY-MM-DDTHH:mm:ss');
}

function describeTaskSchedule(schedule: TaskScheduleConfig, t: TranslateFn): string {
  if (schedule.mode === 'manual') {
    return t(scheduleModeLabelKeys.manual);
  }

  const parts: string[] = [t(scheduleModeLabelKeys[schedule.mode])];
  if (schedule.next_run_at) {
    parts.push(t('tasks.schedule.nextAt', {
      time: dayjs(schedule.next_run_at).format('YYYY-MM-DD HH:mm:ss'),
    }));
  } else if (schedule.run_at) {
    parts.push(dayjs(schedule.run_at).format('YYYY-MM-DD HH:mm:ss'));
  }
  if (schedule.interval_seconds) {
    parts.push(t('tasks.schedule.everySeconds', { seconds: schedule.interval_seconds }));
  }
  return parts.join(' / ');
}

function memoryRoleColor(role: string): string {
  switch (role) {
    case 'assistant':
      return 'blue';
    case 'tool':
      return 'purple';
    case 'system':
      return 'gold';
    case 'user':
      return 'green';
    default:
      return 'default';
  }
}

function memorySummaryColor(status: string): string {
  switch (status) {
    case 'done':
      return 'success';
    case 'pending':
      return 'warning';
    case 'running':
      return 'processing';
    case 'failed':
      return 'error';
    default:
      return 'default';
  }
}

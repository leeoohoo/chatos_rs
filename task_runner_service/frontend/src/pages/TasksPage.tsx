// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Form,
  Modal,
  Space,
  message,
} from 'antd';

import { useI18n } from '../i18n/I18nProvider';
import {
  buildCreateTaskFormValues,
  buildEditTaskFormValues,
  buildTaskPayload,
  completeEnabledBuiltinKindDependencies,
  type TaskFormValues,
  type RunTaskFormValues,
} from './tasks/taskPageUtils';
import { buildTaskTableColumns } from './tasks/taskTableColumns';
import { TaskStatsCards } from './tasks/TaskStatsCards';
import {
  TaskMemoryDrawer,
  type TaskMemoryRoleFilter,
  type TaskMemorySummaryFilter,
} from './tasks/TaskMemoryDrawer';
import { TaskDetailDrawer } from './tasks/TaskDetailDrawer';
import { TaskEditorDrawer } from './tasks/TaskEditorDrawer';
import { BatchTaskRunModal, TaskRunModal } from './tasks/TaskRunModals';
import { TaskBatchActionsBar } from './tasks/TaskBatchActionsBar';
import { TaskListToolbar } from './tasks/TaskListToolbar';
import { TaskListTable } from './tasks/TaskListTable';
import { TaskMcpPromptPreviewModal } from './tasks/TaskMcpPromptPreviewModal';
import { TaskSubtasksDrawer } from './tasks/TaskSubtasksDrawer';
import { useTasksPageEffects } from './tasks/useTasksPageEffects';
import { useTaskMutations } from './tasks/useTaskMutations';
import { useTasksPageData } from './tasks/useTasksPageData';
import type {
  StartTaskRunPayload,
  TaskRecord,
  TaskStatus,
} from '../types';

export function TasksPage() {
  const { locale, t } = useI18n();
  const DEFAULT_PAGE_SIZE = 8;
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
  const [subtasksParentTask, setSubtasksParentTask] = useState<TaskRecord | null>(null);
  const [draftMcpPreviewOpen, setDraftMcpPreviewOpen] = useState(false);
  const [mcpPreviewTask, setMcpPreviewTask] = useState<TaskRecord | null>(null);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([]);
  const [statusFilter, setStatusFilter] = useState<'all' | TaskStatus>('all');
  const [keywordFilter, setKeywordFilter] = useState('');
  const [tagFilter, setTagFilter] = useState<string | undefined>(undefined);
  const [scheduledOnly, setScheduledOnly] = useState(false);
  const [taskPage, setTaskPage] = useState(1);
  const [taskPageSize, setTaskPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [memoryRoleFilter, setMemoryRoleFilter] = useState<TaskMemoryRoleFilter>('all');
  const [memorySummaryFilter, setMemorySummaryFilter] =
    useState<TaskMemorySummaryFilter>('all');
  const [memoryLimit, setMemoryLimit] = useState<number>(50);
  const [form] = Form.useForm<TaskFormValues>();
  const [runForm] = Form.useForm<RunTaskFormValues>();
  const [batchRunForm] = Form.useForm<RunTaskFormValues>();
  const routeTaskId = searchParams.get('task_id');
  const routeModelConfigId = searchParams.get('model_config_id') || undefined;
  const routeProjectId = searchParams.get('project_id') || undefined;

  const {
    tasksQuery,
    taskStatsQuery,
    selectedTaskQuery,
    taskRecentRunsQuery,
    detailLastRunId,
    detailLastRunQuery,
    detailLastRunEventsQuery,
    taskFollowUpQuery,
    taskRunDerivedQuery,
    taskPromptsQuery,
    mcpCatalogQuery,
    taskCapabilityCatalogQuery,
    remoteServersQuery,
    externalMcpConfigsQuery,
    taskMemoryContextQuery,
    taskMemoryRecordsQuery,
    taskMcpPromptPreviewQuery,
    taskMcpResolutionQuery,
    taskEditorMcpResolutionQuery,
    scheduleModeLabels,
    statusFilterOptions,
    taskStatusLabel,
    modelOptions,
    modelNameMap,
    modelLabelMap,
    projectNameMap,
    projectOptions,
    taskSummaryMap,
    prerequisiteTaskOptions,
    tagOptions,
    remoteServerMap,
    externalMcpConfigMap,
    selectedTask,
    detailResultSummary,
    detailRemoteOperations,
    detailRemoteOperationStats,
    latestRemoteOperation,
    recentRemoteOperations,
    taskRowRemoteActivityByTaskId,
    pendingPromptCountByTaskId,
    batchRunTasks,
  } = useTasksPageData({
    t,
    statusFilter,
    keywordFilter,
    tagFilter,
    routeModelConfigId,
    routeProjectId,
    scheduledOnly,
    taskPage,
    taskPageSize,
    detailTaskId,
    detailTaskPreview,
    memoryTask,
    memoryRoleFilter,
    memorySummaryFilter,
    memoryLimit,
    mcpPreviewTask,
    batchRunTaskIds,
    editingTaskId: editingTask?.id,
  });

  const { taskSubtasksQuery } = useTasksPageEffects({
    visibleTasks: tasksQuery.data?.items,
    routeTaskId,
    statusFilter,
    keywordFilter,
    tagFilter,
    routeModelConfigId,
    routeProjectId,
    scheduledOnly,
    drawerOpen,
    editingTask,
    form,
    taskEditorMcpResolution: taskEditorMcpResolutionQuery.data,
    subtasksParentTask,
    setSelectedTaskIds,
    setTaskPage,
    setDetailTaskId,
    setDetailTaskPreview,
  });

  const {
    createTaskMutation,
    updateTaskMutation,
    deleteTaskMutation,
    runTaskMutation,
    batchUpdateTaskStatusMutation,
    batchDeleteTasksMutation,
    batchStartTaskRunsMutation,
    summarizeTaskMemoryMutation,
    draftMcpPreviewMutation,
  } = useTaskMutations({
    t,
    messageApi,
    onTaskSaved: closeTaskDrawer,
    onRunStarted: closeRunModal,
    onBatchRunStarted: closeBatchRunModal,
    onClearSelectedTasks: () => setSelectedTaskIds([]),
  });

  const hasSelectedTasks = selectedTaskIds.length > 0;
  const batchActionPending =
    batchUpdateTaskStatusMutation.isPending ||
    batchDeleteTasksMutation.isPending ||
    batchStartTaskRunsMutation.isPending;

  const columns = buildTaskTableColumns({
    t,
    navigate,
    modelNameMap,
    projectNameMap,
    externalMcpConfigMap,
    pendingPromptCountByTaskId,
    scheduleModeLabels,
    taskRowRemoteActivityByTaskId,
    onOpenDetail: openDetailDrawer,
    onOpenEdit: openEditDrawer,
    onOpenMemory: openMemoryDrawer,
    onOpenSubtasks: openSubtasksDrawer,
    onOpenRun: openRunModal,
    onConfirmDelete: confirmDelete,
  });
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

  function closeSubtasksDrawer() {
    setSubtasksParentTask(null);
  }

  function closeTaskMcpPreviewModal() {
    setMcpPreviewTask(null);
  }

  function closeDraftMcpPreviewModal() {
    setDraftMcpPreviewOpen(false);
  }

  function openCreateDrawer() {
    setEditingTask(null);
    form.setFieldsValue(buildCreateTaskFormValues(locale));
    setDrawerOpen(true);
  }

  function openEditDrawer(task: TaskRecord) {
    setEditingTask(task);
    form.setFieldsValue(buildEditTaskFormValues(task));
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

  function openSubtasksDrawer(task: TaskRecord) {
    setSubtasksParentTask(task);
  }

  function openTaskMcpPreviewModal(task: TaskRecord) {
    setMcpPreviewTask(task);
  }

  function openDraftMcpPreviewModal() {
    const values = form.getFieldsValue([
      'mcpEnabled',
      'builtinPromptMode',
      'builtinPromptLocale',
      'enabledBuiltinKinds',
      'workspaceDir',
      'defaultRemoteServerId',
    ]) as Partial<TaskFormValues>;
    setDraftMcpPreviewOpen(true);
    draftMcpPreviewMutation.mutate({
      enabled: values.mcpEnabled ?? true,
      init_mode: 'full',
      builtin_prompt_mode: values.builtinPromptMode ?? 'effective',
      builtin_prompt_locale: values.builtinPromptLocale || locale,
      enabled_builtin_kinds: completeEnabledBuiltinKindDependencies(values.enabledBuiltinKinds),
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

  function handleSubmit(values: TaskFormValues) {
    const payload = buildTaskPayload(values, { editingTask, routeProjectId });
    if (!payload) {
      messageApi.error(t('tasks.scheduleInvalid'));
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
        <TaskListToolbar
          t={t}
          keywordFilter={keywordFilter}
          tagFilter={tagFilter}
          modelConfigId={routeModelConfigId}
          projectId={routeProjectId}
          statusFilter={statusFilter}
          scheduledOnly={scheduledOnly}
          tagOptions={tagOptions}
          modelOptions={modelOptions}
          projectOptions={projectOptions}
          statusFilterOptions={statusFilterOptions}
          onKeywordFilterChange={setKeywordFilter}
          onTagFilterChange={setTagFilter}
          onModelFilterChange={(value) => {
            const next = new URLSearchParams(searchParams);
            if (value) {
              next.set('model_config_id', value);
            } else {
              next.delete('model_config_id');
            }
            setSearchParams(next);
          }}
          onProjectFilterChange={(value) => {
            const next = new URLSearchParams(searchParams);
            if (value) {
              next.set('project_id', value);
            } else {
              next.delete('project_id');
            }
            setSearchParams(next);
          }}
          onStatusFilterChange={setStatusFilter}
          onScheduledOnlyChange={setScheduledOnly}
          onRefresh={() => {
            void Promise.all([tasksQuery.refetch(), taskStatsQuery.refetch()]);
          }}
          onCreateTask={openCreateDrawer}
        />

        <TaskStatsCards
          t={t}
          stats={taskStatsQuery.data}
          loading={taskStatsQuery.isLoading}
        />

        <TaskBatchActionsBar
          t={t}
          selectedCount={selectedTaskIds.length}
          hasSelectedTasks={hasSelectedTasks}
          pending={batchActionPending}
          batchRunLoading={batchStartTaskRunsMutation.isPending}
          batchUpdateLoading={batchUpdateTaskStatusMutation.isPending}
          batchDeleteLoading={batchDeleteTasksMutation.isPending}
          onOpenBatchRun={openBatchRunModal}
          onSetReady={() =>
            batchUpdateTaskStatusMutation.mutate({
              task_ids: selectedTaskIds,
              status: 'ready',
            })
          }
          onArchive={() =>
            batchUpdateTaskStatusMutation.mutate({
              task_ids: selectedTaskIds,
              status: 'archived',
            })
          }
          onDelete={confirmBatchDelete}
        />

        <TaskListTable
          t={t}
          selectedTaskIds={selectedTaskIds}
          loading={tasksQuery.isLoading}
          columns={columns}
          tasks={tasksQuery.data?.items || []}
          page={taskPage}
          pageSize={taskPageSize}
          total={tasksQuery.data?.total || 0}
          onSelectedTaskIdsChange={setSelectedTaskIds}
          onPageChange={(page, pageSize) => {
            setTaskPage(page);
            setTaskPageSize(pageSize);
          }}
        />
      </Space>

      <TaskDetailDrawer
        t={t}
        open={Boolean(detailTaskId)}
        task={selectedTask}
        loading={selectedTaskQuery.isLoading}
        detailLastRunId={detailLastRunId}
        detailResultSummary={detailResultSummary}
        remoteOperations={detailRemoteOperations}
        remoteOperationStats={detailRemoteOperationStats}
        latestRemoteOperation={latestRemoteOperation}
        recentRemoteOperations={recentRemoteOperations}
        remoteOperationsLoading={detailLastRunEventsQuery.isLoading || detailLastRunQuery.isLoading}
        recentRuns={taskRecentRunsQuery.data}
        recentRunsLoading={taskRecentRunsQuery.isLoading}
        prompts={taskPromptsQuery.data}
        promptsLoading={taskPromptsQuery.isLoading}
        mcpResolution={taskMcpResolutionQuery.data}
        mcpResolutionLoading={taskMcpResolutionQuery.isLoading}
        followUps={taskFollowUpQuery.data}
        followUpsLoading={taskFollowUpQuery.isLoading}
        runDerivedTasks={taskRunDerivedQuery.data}
        runDerivedTasksLoading={taskRunDerivedQuery.isLoading}
        modelLabelMap={modelLabelMap}
        taskSummaryMap={taskSummaryMap}
        remoteServerMap={remoteServerMap}
        externalMcpConfigMap={externalMcpConfigMap}
        taskStatusLabel={taskStatusLabel}
        onClose={closeDetailDrawer}
        onEditTask={openEditDrawer}
        onRunTask={openRunModal}
        onOpenMemory={openMemoryDrawer}
        onPreviewMcpPrompt={openTaskMcpPreviewModal}
        onOpenRunHistory={jumpToRunHistory}
        onOpenPrompts={(taskId, promptId) => {
          const search = new URLSearchParams();
          search.set('task_id', taskId);
          if (promptId) {
            search.set('prompt_id', promptId);
          }
          navigate(`/prompts?${search.toString()}`);
        }}
        onOpenModel={(modelId) =>
          navigate(`/models?model_id=${encodeURIComponent(modelId)}`)
        }
        onOpenServers={(serverId) => {
          if (serverId) {
            navigate(`/servers?server_id=${encodeURIComponent(serverId)}`);
            return;
          }
          navigate('/servers');
        }}
        onOpenDetail={openDetailDrawer}
      />

      <TaskEditorDrawer
        t={t}
        open={drawerOpen}
        editingTask={editingTask}
        form={form}
        saving={createTaskMutation.isPending || updateTaskMutation.isPending}
        modelOptions={modelOptions}
        prerequisiteTaskOptions={prerequisiteTaskOptions}
        mcpCatalogEntries={mcpCatalogQuery.data}
        selectableSkills={taskCapabilityCatalogQuery.data?.selectable_skills}
        remoteServers={remoteServersQuery.data}
        externalMcpConfigs={externalMcpConfigsQuery.data}
        onClose={closeTaskDrawer}
        onSubmit={handleSubmit}
        onPreviewPrompt={openDraftMcpPreviewModal}
        onManageServers={() => navigate('/servers')}
        onViewMcpCatalog={() => navigate('/mcp')}
      />

      <TaskMcpPromptPreviewModal
        t={t}
        title={mcpPreviewTask
          ? t('tasks.preview.titleWithName', { title: mcpPreviewTask.title })
          : t('tasks.preview.title')}
        open={Boolean(mcpPreviewTask)}
        preview={taskMcpPromptPreviewQuery.data}
        loading={taskMcpPromptPreviewQuery.isLoading}
        onClose={closeTaskMcpPreviewModal}
      />

      <TaskMcpPromptPreviewModal
        t={t}
        title={t('tasks.preview.formTitle')}
        open={draftMcpPreviewOpen}
        preview={draftMcpPreviewMutation.data}
        loading={draftMcpPreviewMutation.isPending}
        onClose={closeDraftMcpPreviewModal}
      />

      <TaskMemoryDrawer
        t={t}
        task={memoryTask}
        roleFilter={memoryRoleFilter}
        summaryFilter={memorySummaryFilter}
        limit={memoryLimit}
        context={taskMemoryContextQuery.data}
        contextLoading={taskMemoryContextQuery.isLoading}
        records={taskMemoryRecordsQuery.data}
        recordsLoading={taskMemoryRecordsQuery.isLoading}
        summarizeLoading={summarizeTaskMemoryMutation.isPending}
        onClose={closeMemoryDrawer}
        onRoleFilterChange={setMemoryRoleFilter}
        onSummaryFilterChange={setMemorySummaryFilter}
        onLimitChange={setMemoryLimit}
        onRefresh={() => {
          void Promise.all([
            taskMemoryContextQuery.refetch(),
            taskMemoryRecordsQuery.refetch(),
          ]);
        }}
        onSummarize={(taskId) => summarizeTaskMemoryMutation.mutate(taskId)}
      />

      <TaskSubtasksDrawer
        t={t}
        open={Boolean(subtasksParentTask)}
        parentTask={subtasksParentTask}
        tasks={taskSubtasksQuery.data}
        loading={taskSubtasksQuery.isLoading}
        taskStatusLabel={taskStatusLabel}
        onClose={closeSubtasksDrawer}
        onOpenDetail={openDetailDrawer}
        onOpenRunHistory={jumpToRunHistory}
      />

      <TaskRunModal
        t={t}
        task={runningTask}
        form={runForm}
        modelOptions={modelOptions}
        loading={runTaskMutation.isPending}
        onClose={closeRunModal}
        onSubmit={handleRunTask}
      />

      <BatchTaskRunModal
        t={t}
        taskIds={batchRunTaskIds}
        tasks={batchRunTasks}
        form={batchRunForm}
        modelOptions={modelOptions}
        loading={batchStartTaskRunsMutation.isPending}
        onClose={closeBatchRunModal}
        onSubmit={handleBatchRunTask}
      />
    </>
  );
}

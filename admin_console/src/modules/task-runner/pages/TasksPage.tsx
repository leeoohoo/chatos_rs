// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { lazy, Suspense, useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Form,
  Modal,
  Space,
  message,
} from 'antd';

import { useI18n } from '../i18n/I18nProvider';
import {
  buildEditTaskFormValues,
  buildTaskUpdatePayload,
  type TaskFormValues,
  type RunTaskFormValues,
} from './tasks/taskPageUtils';
import { buildTaskTableColumns } from './tasks/taskTableColumns';
import { TaskStatsCards } from './tasks/TaskStatsCards';
import type {
  TaskMemoryRoleFilter,
  TaskMemorySummaryFilter,
} from './tasks/TaskMemoryDrawer';
import { TaskBatchActionsBar } from './tasks/TaskBatchActionsBar';
import { TaskListToolbar } from './tasks/TaskListToolbar';
import { TaskListTable } from './tasks/TaskListTable';
import { useTasksPageEffects } from './tasks/useTasksPageEffects';
import { useTaskMutations } from './tasks/useTaskMutations';
import { useTasksPageData } from './tasks/useTasksPageData';
import type {
  StartTaskRunPayload,
  TaskRecord,
  TaskStatus,
} from '../types';

const TaskDetailDrawer = lazy(() => import('./tasks/TaskDetailDrawer').then((module) => ({ default: module.TaskDetailDrawer })));
const TaskEditorDrawer = lazy(() => import('./tasks/TaskEditorDrawer').then((module) => ({ default: module.TaskEditorDrawer })));
const TaskMcpPromptPreviewModal = lazy(() => import('./tasks/TaskMcpPromptPreviewModal').then((module) => ({ default: module.TaskMcpPromptPreviewModal })));
const TaskMemoryDrawer = lazy(() => import('./tasks/TaskMemoryDrawer').then((module) => ({ default: module.TaskMemoryDrawer })));
const TaskSubtasksDrawer = lazy(() => import('./tasks/TaskSubtasksDrawer').then((module) => ({ default: module.TaskSubtasksDrawer })));
const TaskRunModal = lazy(() => import('./tasks/TaskRunModals').then((module) => ({ default: module.TaskRunModal })));
const BatchTaskRunModal = lazy(() => import('./tasks/TaskRunModals').then((module) => ({ default: module.BatchTaskRunModal })));

export function TasksPage() {
  const { t } = useI18n();
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
  const [mcpPreviewTask, setMcpPreviewTask] = useState<TaskRecord | null>(null);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([]);
  const [statusFilter, setStatusFilter] = useState<'all' | TaskStatus>('all');
  const [keywordInput, setKeywordInput] = useState('');
  const [keywordFilter, setKeywordFilter] = useState('');
  const [tagFilter, setTagFilter] = useState<string | undefined>(undefined);
  const [scheduledOnly, setScheduledOnly] = useState(false);
  const [taskIndexEnabled, setTaskIndexEnabled] = useState(false);
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

  useEffect(() => {
    const timer = window.setTimeout(() => setKeywordFilter(keywordInput.trim()), 300);
    return () => window.clearTimeout(timer);
  }, [keywordInput]);

  const {
    tasksQuery,
    taskStatsQuery,
    selectedTaskQuery,
    taskRecentRunsQuery,
    detailLastRunId,
    detailLastRunQuery,
    taskFollowUpQuery,
    taskRunDerivedQuery,
    taskPromptsQuery,
    taskMemoryContextQuery,
    taskMemoryRecordsQuery,
    taskMcpPromptPreviewQuery,
    taskMcpResolutionQuery,
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
    selectedTask,
    detailResultSummary,
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
    taskIndexEnabled,
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
    subtasksParentTask,
    setSelectedTaskIds,
    setTaskPage,
    setDetailTaskId,
    setDetailTaskPreview,
  });

  const {
    updateTaskMutation,
    deleteTaskMutation,
    runTaskMutation,
    batchUpdateTaskStatusMutation,
    batchDeleteTasksMutation,
    batchStartTaskRunsMutation,
    summarizeTaskMemoryMutation,
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
    pendingPromptCountByTaskId,
    scheduleModeLabels,
    onOpenDetail: openDetailDrawer,
    onOpenEdit: openEditDrawer,
    onOpenMemory: openMemoryDrawer,
    onOpenSubtasks: openSubtasksDrawer,
    onOpenRun: openRunModal,
    onConfirmDelete: confirmDelete,
  });
  function closeTaskDrawer() {
    setDrawerOpen(false);
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

  function openEditDrawer(task: TaskRecord) {
    setTaskIndexEnabled(true);
    setEditingTask(task);
    form.setFieldsValue(buildEditTaskFormValues(task));
    setDrawerOpen(true);
  }

  function openDetailDrawer(task: TaskRecord) {
    setTaskIndexEnabled(true);
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

  function jumpToRunHistory(taskId: string, runId?: string) {
    const search = new URLSearchParams();
    search.set('task_id', taskId);
    if (runId) {
      search.set('run_id', runId);
    }
    navigate(`/task-runner/runs?${search.toString()}`);
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
    if (!editingTask) {
      return;
    }
    const payload = buildTaskUpdatePayload(values);
    if (!payload) {
      messageApi.error(t('tasks.scheduleInvalid'));
      return;
    }

    updateTaskMutation.mutate({ id: editingTask.id, payload });
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
          keywordFilter={keywordInput}
          tagFilter={tagFilter}
          modelConfigId={routeModelConfigId}
          projectId={routeProjectId}
          statusFilter={statusFilter}
          scheduledOnly={scheduledOnly}
          tagOptions={tagOptions}
          modelOptions={modelOptions}
          projectOptions={projectOptions}
          statusFilterOptions={statusFilterOptions}
          onKeywordFilterChange={setKeywordInput}
          onTagFilterChange={setTagFilter}
          onTagDropdownOpen={() => setTaskIndexEnabled(true)}
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

      <Suspense fallback={null}>
      {detailTaskId ? <TaskDetailDrawer
        t={t}
        open={Boolean(detailTaskId)}
        task={selectedTask}
        loading={selectedTaskQuery.isLoading}
        detailLastRunId={detailLastRunId}
        detailLastRunLoading={detailLastRunQuery.isLoading}
        detailResultSummary={detailResultSummary}
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
        projectNameMap={projectNameMap}
        taskSummaryMap={taskSummaryMap}
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
          navigate(`/task-runner/prompts?${search.toString()}`);
        }}
        onOpenDetail={openDetailDrawer}
      /> : null}

      {drawerOpen ? <TaskEditorDrawer
        t={t}
        open={drawerOpen}
        editingTask={editingTask}
        form={form}
        saving={updateTaskMutation.isPending}
        modelOptions={modelOptions}
        projectOptions={projectOptions}
        prerequisiteTaskOptions={prerequisiteTaskOptions}
        onClose={closeTaskDrawer}
        onSubmit={handleSubmit}
      /> : null}

      {mcpPreviewTask ? <TaskMcpPromptPreviewModal
        t={t}
        title={mcpPreviewTask
          ? t('tasks.preview.titleWithName', { title: mcpPreviewTask.title })
          : t('tasks.preview.title')}
        open={Boolean(mcpPreviewTask)}
        preview={taskMcpPromptPreviewQuery.data}
        loading={taskMcpPromptPreviewQuery.isLoading}
        onClose={closeTaskMcpPreviewModal}
      /> : null}

      {memoryTask ? <TaskMemoryDrawer
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
      /> : null}

      {subtasksParentTask ? <TaskSubtasksDrawer
        t={t}
        open={Boolean(subtasksParentTask)}
        parentTask={subtasksParentTask}
        tasks={taskSubtasksQuery.data}
        loading={taskSubtasksQuery.isLoading}
        taskStatusLabel={taskStatusLabel}
        onClose={closeSubtasksDrawer}
        onOpenDetail={openDetailDrawer}
        onOpenRunHistory={jumpToRunHistory}
      /> : null}

      {runningTask ? <TaskRunModal
        t={t}
        task={runningTask}
        form={runForm}
        modelOptions={modelOptions}
        loading={runTaskMutation.isPending}
        onClose={closeRunModal}
        onSubmit={handleRunTask}
      /> : null}

      {batchRunTaskIds.length ? <BatchTaskRunModal
        t={t}
        taskIds={batchRunTaskIds}
        tasks={batchRunTasks}
        form={batchRunForm}
        modelOptions={modelOptions}
        loading={batchStartTaskRunsMutation.isPending}
        onClose={closeBatchRunModal}
        onSubmit={handleBatchRunTask}
      /> : null}
      </Suspense>
    </>
  );
}

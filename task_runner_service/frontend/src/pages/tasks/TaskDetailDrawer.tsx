// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  List,
  Space,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  ExternalMcpConfigRecord,
  PaginatedResponse,
  RemoteServerRecord,
  TaskRecord,
  TaskRunRecord,
  TaskStatus,
  AskUserPromptRecord,
  TaskMcpResolutionResponse,
} from '../../types';
import {
  describeTaskSchedule,
  isSchedulerOnlyTask,
  JsonBlock,
  statusColorMap,
  taskCreatorLabel,
  taskProfileColorMap,
  taskProfileLabel,
  type TaskRemoteOperationStats,
  type TaskRemoteOperationView,
} from './taskPageUtils';
import {
  RecentRunsSection,
  RelatedPromptsSection,
  RelatedTasksSection,
  RemoteOperationsSection,
  TextSection,
} from './TaskDetailSections';

type TaskDetailDrawerProps = {
  t: TranslateFn;
  open: boolean;
  task: TaskRecord | null;
  loading: boolean;
  detailLastRunId?: string | null;
  detailResultSummary?: string | null;
  remoteOperations: TaskRemoteOperationView[];
  remoteOperationStats: TaskRemoteOperationStats;
  latestRemoteOperation: TaskRemoteOperationView | null;
  recentRemoteOperations: TaskRemoteOperationView[];
  remoteOperationsLoading: boolean;
  recentRuns?: TaskRunRecord[];
  recentRunsLoading: boolean;
  prompts?: PaginatedResponse<AskUserPromptRecord>;
  promptsLoading: boolean;
  mcpResolution?: TaskMcpResolutionResponse;
  mcpResolutionLoading: boolean;
  followUps?: TaskRecord[];
  followUpsLoading: boolean;
  runDerivedTasks?: TaskRecord[];
  runDerivedTasksLoading: boolean;
  modelLabelMap: Map<string, string>;
  projectNameMap: Map<string, string>;
  taskSummaryMap: Map<string, string>;
  remoteServerMap: Map<string, RemoteServerRecord>;
  externalMcpConfigMap: Map<string, ExternalMcpConfigRecord>;
  taskStatusLabel: (status: TaskStatus) => string;
  onClose: () => void;
  onEditTask: (task: TaskRecord) => void;
  onRunTask: (task: TaskRecord) => void;
  onOpenMemory: (task: TaskRecord) => void;
  onPreviewMcpPrompt: (task: TaskRecord) => void;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
  onOpenPrompts: (taskId: string, promptId?: string) => void;
  onOpenModel: (modelId: string) => void;
  onOpenServers: (serverId?: string) => void;
  onOpenDetail: (task: TaskRecord) => void;
};

export function TaskDetailDrawer({
  t,
  open,
  task,
  loading,
  detailLastRunId,
  detailResultSummary,
  remoteOperations,
  remoteOperationStats,
  latestRemoteOperation,
  recentRemoteOperations,
  remoteOperationsLoading,
  recentRuns,
  recentRunsLoading,
  prompts,
  promptsLoading,
  mcpResolution,
  mcpResolutionLoading,
  followUps,
  followUpsLoading,
  runDerivedTasks,
  runDerivedTasksLoading,
  modelLabelMap,
  projectNameMap,
  taskSummaryMap,
  remoteServerMap,
  externalMcpConfigMap,
  taskStatusLabel,
  onClose,
  onEditTask,
  onRunTask,
  onOpenMemory,
  onPreviewMcpPrompt,
  onOpenRunHistory,
  onOpenPrompts,
  onOpenModel,
  onOpenServers,
  onOpenDetail,
}: TaskDetailDrawerProps) {
  return (
    <Drawer
      title={task ? t('tasks.detail.titleWithName', { title: task.title }) : t('tasks.detail.title')}
      open={open}
      width={760}
      onClose={onClose}
    >
      {task ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space wrap>
            <Button
              onClick={() => {
                onClose();
                onEditTask(task);
              }}
            >
              {t('tasks.detail.editTask')}
            </Button>
            <Button
              type="primary"
              disabled={
                task.status === 'queued' ||
                task.status === 'running' ||
                isSchedulerOnlyTask(task)
              }
              onClick={() => {
                onClose();
                onRunTask(task);
              }}
            >
              {t('tasks.detail.runNow')}
            </Button>
            <Button onClick={() => onOpenRunHistory(task.id)}>
              {t('tasks.detail.allRunHistory')}
            </Button>
            <Button
              onClick={() => {
                onClose();
                onOpenMemory(task);
              }}
            >
              {t('tasks.detail.openMemory')}
            </Button>
            <Button onClick={() => onPreviewMcpPrompt(task)}>
              {t('tasks.detail.previewMcpPrompt')}
            </Button>
            <Button onClick={() => onOpenPrompts(task.id)}>
              {t('tasks.detail.relatedPrompts')}
            </Button>
          </Space>

          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('tasks.detail.taskId')}>{task.id}</Descriptions.Item>
            <Descriptions.Item label={t('common.status')}>
              <Tag color={statusColorMap[task.status]}>{taskStatusLabel(task.status)}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.taskProfile')}>
              <Tag color={taskProfileColorMap[task.task_profile] || 'default'}>
                {taskProfileLabel(task.task_profile, t)}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.column.creator')}>
              {taskCreatorLabel(task)}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.column.project')}>
              {task.project_id === '-1'
                ? t('projects.public')
                : projectNameMap.get(task.project_id) || task.project_id}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.defaultModel')}>
              {task.default_model_config_id ? (
                <Button
                  type="link"
                  size="small"
                  style={{ paddingInline: 0 }}
                  onClick={() => onOpenModel(task.default_model_config_id!)}
                >
                  {modelLabelMap.get(task.default_model_config_id) ||
                    task.default_model_config_id}
                </Button>
              ) : (
                t('tasks.modelUnbound')
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.column.priority')}>
              {task.priority}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.form.requiresExecution')}>
              {task.mcp_config.requires_execution === false ? t('common.no') : t('common.yes')}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.column.schedule')}>
              {describeTaskSchedule(task.schedule, t)}
            </Descriptions.Item>
            <Descriptions.Item label="前置任务">
              {task.prerequisite_task_ids.length ? (
                <Space wrap>
                  {task.prerequisite_task_ids.map((taskId) => (
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
              <Typography.Text code>{task.memory_thread_id}</Typography.Text>
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.recentRun')}>
              {task.last_run_id || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.defaultServer')}>
              {task.mcp_config.default_remote_server_id
                ? remoteServerMap.get(task.mcp_config.default_remote_server_id)?.name ||
                  task.mcp_config.default_remote_server_id
                : t('tasks.modelUnbound')}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.externalMcpConfigs')}>
              {task.mcp_config.external_mcp_config_ids?.length ? (
                <Space wrap>
                  {task.mcp_config.external_mcp_config_ids.map((configId) => {
                    const config = externalMcpConfigMap.get(configId);
                    return (
                      <Tag key={configId} color={config?.enabled === false ? 'default' : 'blue'}>
                        {config?.name || configId}
                      </Tag>
                    );
                  })}
                </Space>
              ) : (
                t('common.noData')
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.skills')}>
              {task.mcp_config.selected_skill_ids?.length ? (
                <Space wrap>
                  {task.mcp_config.selected_skill_ids.map((skillId) => (
                    <Tag key={skillId} color="purple">
                      {skillId}
                    </Tag>
                  ))}
                </Space>
              ) : (
                t('common.noData')
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.mcpRequestedCapabilities')}>
              {renderStringTags(
                mcpResolution?.requested_builtin_kinds,
                'blue',
                mcpResolutionLoading ? t('common.loading') : t('common.noData'),
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.mcpRequiredCapabilities')}>
              {mcpResolutionLoading ? (
                t('common.loading')
              ) : mcpResolution?.required_builtin_kinds.length ? (
                <Space wrap>
                  {mcpResolution.required_builtin_kinds.map((item) => (
                    <Tag key={`${item.source}:${item.kind}`} color="geekblue">
                      {item.kind} / {item.source}
                    </Tag>
                  ))}
                </Space>
              ) : (
                t('common.noData')
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.mcpHostedRoutes')}>
              {mcpResolutionLoading ? (
                t('common.loading')
              ) : mcpResolution?.hosted_builtin_routes.length ? (
                <Space direction="vertical" size={4}>
                  {mcpResolution.hosted_builtin_routes.map((route) => (
                    <Space key={route.host} wrap>
                      <Tag color="purple">{route.host}</Tag>
                      {route.public_server_names.map((serverName) => (
                        <Tag key={`${route.host}:${serverName}`} color="cyan">
                          {serverName}
                        </Tag>
                      ))}
                      {route.builtin_kinds.map((kind) => (
                        <Tag key={`${route.host}:${kind}`}>{kind}</Tag>
                      ))}
                    </Space>
                  ))}
                </Space>
              ) : (
                t('common.noData')
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.mcpServerLocalCapabilities')}>
              {renderStringTags(
                mcpResolution?.server_local_builtin_kinds,
                'cyan',
                mcpResolutionLoading ? t('common.loading') : t('common.noData'),
              )}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.detail.createdAt')}>
              {dayjs(task.created_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
            <Descriptions.Item label={t('tasks.column.updatedAt')}>
              {dayjs(task.updated_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
          </Descriptions>

          <TextSection title={t('tasks.detail.objective')} value={task.objective} />

          {task.description ? (
            <TextSection title={t('tasks.detail.description')} value={task.description} />
          ) : null}

          {task.process_log ? (
            <TextSection title={t('tasks.detail.processLog')} value={task.process_log} />
          ) : null}

          {detailResultSummary ? (
            <TextSection title={t('tasks.detail.latestSummary')} value={detailResultSummary} />
          ) : null}

          {task.task_tool_state.outcome_items.length ? (
            <div>
              <Typography.Title level={5}>{t('tasks.detail.outcomes')}</Typography.Title>
              <List
                bordered
                dataSource={task.task_tool_state.outcome_items}
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
            <RemoteOperationsSection
              t={t}
              task={task}
              detailLastRunId={detailLastRunId}
              operations={remoteOperations}
              stats={remoteOperationStats}
              latest={latestRemoteOperation}
              recent={recentRemoteOperations}
              loading={remoteOperationsLoading}
              onOpenRunHistory={onOpenRunHistory}
              onOpenServers={onOpenServers}
            />
          ) : null}

          <RecentRunsSection
            t={t}
            task={task}
            runs={recentRuns}
            loading={recentRunsLoading}
            onOpenRunHistory={onOpenRunHistory}
          />

          <RelatedPromptsSection
            t={t}
            task={task}
            prompts={prompts}
            loading={promptsLoading}
            onOpenPrompts={onOpenPrompts}
          />

          <RelatedTasksSection
            t={t}
            title={t('tasks.detail.followUps')}
            tasks={followUps}
            loading={followUpsLoading}
            emptyDescription={t('tasks.detail.noFollowUps')}
            taskStatusLabel={taskStatusLabel}
            onOpenDetail={onOpenDetail}
            onOpenRunHistory={onOpenRunHistory}
            onRunTask={onRunTask}
            showRunAction
            sourceLabel="source run"
          />

          <RelatedTasksSection
            t={t}
            title={t('tasks.detail.runDerivedTasks')}
            tasks={runDerivedTasks}
            loading={runDerivedTasksLoading}
            emptyDescription={t('tasks.detail.noDerivedTasks')}
            taskStatusLabel={taskStatusLabel}
            onOpenDetail={onOpenDetail}
            onOpenRunHistory={onOpenRunHistory}
            onRunTask={onRunTask}
            sourceLabel="parent"
          />

          {task.input_payload ? (
            <JsonBlock title={t('tasks.detail.inputSnapshot')} value={task.input_payload} />
          ) : null}
        </Space>
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </Drawer>
  );
}

function renderStringTags(values: string[] | undefined, color: string, emptyText: string) {
  if (!values?.length) {
    return emptyText;
  }
  return (
    <Space wrap>
      {values.map((value) => (
        <Tag key={value} color={color}>
          {value}
        </Tag>
      ))}
    </Space>
  );
}

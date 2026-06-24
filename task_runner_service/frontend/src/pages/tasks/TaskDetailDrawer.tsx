import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  List,
  Space,
  Statistic,
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
} from '../../types';
import {
  describeTaskSchedule,
  formatTaskRemoteEndpoint,
  isSchedulerOnlyTask,
  JsonBlock,
  promptStatusColorMap,
  runStatusColorMap,
  statusColorMap,
  taskCreatorLabel,
  type TaskRemoteOperationStats,
  type TaskRemoteOperationView,
} from './taskPageUtils';

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
  followUps?: TaskRecord[];
  followUpsLoading: boolean;
  runDerivedTasks?: TaskRecord[];
  runDerivedTasksLoading: boolean;
  modelLabelMap: Map<string, string>;
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
  followUps,
  followUpsLoading,
  runDerivedTasks,
  runDerivedTasksLoading,
  modelLabelMap,
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
            <Descriptions.Item label={t('tasks.column.creator')}>
              {taskCreatorLabel(task)}
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
            <Descriptions.Item label={t('tasks.detail.mcpWorkspace')}>
              {task.mcp_config.workspace_dir || t('tasks.detail.workspaceNotConfigured')}
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

function TextSection({ title, value }: { title: string; value: string }) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
        {value}
      </Typography.Paragraph>
    </div>
  );
}

function RemoteOperationsSection({
  t,
  task,
  detailLastRunId,
  operations,
  stats,
  latest,
  recent,
  loading,
  onOpenRunHistory,
  onOpenServers,
}: {
  t: TranslateFn;
  task: TaskRecord;
  detailLastRunId: string;
  operations: TaskRemoteOperationView[];
  stats: TaskRemoteOperationStats;
  latest: TaskRemoteOperationView | null;
  recent: TaskRemoteOperationView[];
  loading: boolean;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
  onOpenServers: (serverId?: string) => void;
}) {
  return (
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
            onClick={() => onOpenRunHistory(task.id, detailLastRunId)}
          >
            {t('tasks.detail.openRecentRun')}
          </Button>
          <Button size="small" onClick={() => onOpenServers()}>
            {t('tasks.detail.servers')}
          </Button>
        </Space>
      </Space>

      {operations.length ? (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Space size="large" wrap>
            <Statistic title={t('tasks.detail.remoteOperationCount')} value={stats.total} />
            <Statistic title={t('tasks.detail.involvedServers')} value={stats.serverCount} />
            <Statistic title={t('tasks.detail.success')} value={stats.successCount} />
            <Statistic title={t('tasks.detail.failed')} value={stats.failedCount} />
          </Space>

          {latest ? (
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('tasks.detail.latestOperation')}>
                <Space wrap>
                  <Tag color={latest.success ? 'success' : 'error'}>
                    {latest.success ? t('tasks.detail.success') : t('tasks.detail.failed')}
                  </Tag>
                  <Typography.Text strong>{latest.name}</Typography.Text>
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.server')}>
                {latest.connectionId ? (
                  <Button
                    type="link"
                    size="small"
                    style={{ paddingInline: 0 }}
                    onClick={() => onOpenServers(latest.connectionId)}
                  >
                    {latest.connectionName || latest.connectionId}
                  </Button>
                ) : (
                  latest.connectionName || '-'
                )}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.host')}>
                {formatTaskRemoteEndpoint(latest.username, latest.host, latest.port) || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.commandPath')}>
                {latest.command || latest.path || latest.summary || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.remoteHost')}>
                {latest.remoteHost || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.resultSummary')}>
                {latest.content || '-'}
              </Descriptions.Item>
            </Descriptions>
          ) : null}

          <List
            bordered
            dataSource={recent}
            renderItem={(operation) => (
              <List.Item
                actions={[
                  <Button
                    key="run"
                    size="small"
                    onClick={() => onOpenRunHistory(task.id, detailLastRunId)}
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
      ) : loading ? null : (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={t('tasks.detail.noRemoteOperations')}
        />
      )}
    </div>
  );
}

function RecentRunsSection({
  t,
  task,
  runs,
  loading,
  onOpenRunHistory,
}: {
  t: TranslateFn;
  task: TaskRecord;
  runs?: TaskRunRecord[];
  loading: boolean;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('tasks.detail.recentRuns')}</Typography.Title>
      {runs?.length ? (
        <List
          bordered
          dataSource={runs}
          renderItem={(run) => (
            <List.Item
              actions={[
                <Button
                  key="open"
                  size="small"
                  onClick={() => onOpenRunHistory(task.id, run.id)}
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
                  <Typography.Text type="secondary">
                    {t('tasks.detail.noSummary')}
                  </Typography.Text>
                )}
              </Space>
            </List.Item>
          )}
        />
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noRunRecords')} />
      )}
    </div>
  );
}

function RelatedPromptsSection({
  t,
  task,
  prompts,
  loading,
  onOpenPrompts,
}: {
  t: TranslateFn;
  task: TaskRecord;
  prompts?: PaginatedResponse<AskUserPromptRecord>;
  loading: boolean;
  onOpenPrompts: (taskId: string, promptId?: string) => void;
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('tasks.detail.relatedPrompts')}</Typography.Title>
      {prompts?.items.length ? (
        <List
          bordered
          dataSource={prompts.items}
          renderItem={(prompt) => (
            <List.Item
              actions={[
                <Button
                  key="open"
                  size="small"
                  onClick={() => onOpenPrompts(task.id, prompt.id)}
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
                  <Tag color={promptStatusColorMap[prompt.status]}>{prompt.status}</Tag>
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
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noPrompts')} />
      )}
      {prompts?.has_more ? (
        <Space style={{ marginTop: 12 }}>
          <Typography.Text type="secondary">
            {t('tasks.detail.promptVisibleCount', {
              shown: prompts.items.length,
              total: prompts.total,
            })}
          </Typography.Text>
          <Button size="small" onClick={() => onOpenPrompts(task.id)}>
            {t('tasks.detail.viewAll')}
          </Button>
        </Space>
      ) : null}
    </div>
  );
}

function RelatedTasksSection({
  t,
  title,
  tasks,
  loading,
  emptyDescription,
  taskStatusLabel,
  onOpenDetail,
  onOpenRunHistory,
  onRunTask,
  showRunAction = false,
  sourceLabel,
}: {
  t: TranslateFn;
  title: string;
  tasks?: TaskRecord[];
  loading: boolean;
  emptyDescription: string;
  taskStatusLabel: (status: TaskStatus) => string;
  onOpenDetail: (task: TaskRecord) => void;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
  onRunTask: (task: TaskRecord) => void;
  showRunAction?: boolean;
  sourceLabel: 'source run' | 'parent';
}) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      {tasks?.length ? (
        <List
          bordered
          dataSource={tasks}
          renderItem={(relatedTask) => (
            <List.Item
              actions={[
                <Button key="detail" size="small" onClick={() => onOpenDetail(relatedTask)}>
                  {t('tasks.action.detail')}
                </Button>,
                <Button
                  key="history"
                  size="small"
                  onClick={() => onOpenRunHistory(relatedTask.id)}
                >
                  {t('tasks.action.history')}
                </Button>,
                showRunAction ? (
                  <Button
                    key="run"
                    size="small"
                    type="primary"
                    disabled={
                      relatedTask.status === 'queued' ||
                      relatedTask.status === 'running' ||
                      isSchedulerOnlyTask(relatedTask)
                    }
                    onClick={() => onRunTask(relatedTask)}
                  >
                    {t('tasks.action.run')}
                  </Button>
                ) : null,
              ]}
            >
              <Space direction="vertical" size={4} style={{ width: '100%' }}>
                <Space wrap>
                  <Typography.Text strong>{relatedTask.title}</Typography.Text>
                  <Tag color={statusColorMap[relatedTask.status]}>
                    {taskStatusLabel(relatedTask.status)}
                  </Tag>
                  <RelatedTaskSourceLabel task={relatedTask} sourceLabel={sourceLabel} />
                </Space>
                <Typography.Paragraph
                  type="secondary"
                  ellipsis={{ rows: 2 }}
                  style={{ marginBottom: 0 }}
                >
                  {relatedTask.objective}
                </Typography.Paragraph>
              </Space>
            </List.Item>
          )}
        />
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={emptyDescription} />
      )}
    </div>
  );
}

function RelatedTaskSourceLabel({
  task,
  sourceLabel,
}: {
  task: TaskRecord;
  sourceLabel: 'source run' | 'parent';
}) {
  if (sourceLabel === 'source run' && task.source_run_id) {
    return (
      <Typography.Text type="secondary">
        source run: {task.source_run_id.slice(0, 12)}
      </Typography.Text>
    );
  }

  if (sourceLabel === 'parent' && task.parent_task_id) {
    return (
      <Typography.Text type="secondary">
        parent: {task.parent_task_id.slice(0, 12)}
      </Typography.Text>
    );
  }

  return null;
}

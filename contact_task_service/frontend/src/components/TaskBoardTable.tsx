import { Space, Table, Tag, Typography } from 'antd';
import { useMemo } from 'react';

import { formatBlockedReason, formatTaskStatusColor } from '../appHelpers';
import type { ContactTask, TaskExecutionMessage, TaskResultBrief } from '../types';
import { TaskExpandedRowContent } from './TaskExpandedRowContent';

const { Text } = Typography;

type TaskBoardTableProps = {
  loading: boolean;
  visibleTasks: ContactTask[];
  expandedTaskIds: string[];
  formatRelatedTask: (taskId: string) => string;
  getContactDisplayName: (record: ContactTask) => string;
  getProjectDisplayName: (record: ContactTask) => string;
  executionMessagesByTaskId: Record<string, TaskExecutionMessage[]>;
  executionLoadingByTaskId: Record<string, boolean>;
  executionErrorByTaskId: Record<string, string | null>;
  executionSectionExpandedByTaskId: Record<string, boolean>;
  executionPageByTaskId: Record<string, number>;
  resultBriefByTaskId: Record<string, TaskResultBrief | null | undefined>;
  resultBriefLoadingByTaskId: Record<string, boolean>;
  resultBriefErrorByTaskId: Record<string, string | null>;
  onExpandedTaskIdsChange: (taskIds: string[]) => void;
  onLoadExecutionMessages: (taskId: string, force?: boolean) => Promise<void> | void;
  onLoadTaskResultBrief: (taskId: string, force?: boolean) => Promise<void> | void;
  onExecutionSectionExpandedChange: (taskId: string, expanded: boolean) => void;
  onExecutionPageChange: (taskId: string, page: number) => void;
};

export function TaskBoardTable({
  loading,
  visibleTasks,
  expandedTaskIds,
  formatRelatedTask,
  getContactDisplayName,
  getProjectDisplayName,
  executionMessagesByTaskId,
  executionLoadingByTaskId,
  executionErrorByTaskId,
  executionSectionExpandedByTaskId,
  executionPageByTaskId,
  resultBriefByTaskId,
  resultBriefLoadingByTaskId,
  resultBriefErrorByTaskId,
  onExpandedTaskIdsChange,
  onLoadExecutionMessages,
  onLoadTaskResultBrief,
  onExecutionSectionExpandedChange,
  onExecutionPageChange,
}: TaskBoardTableProps) {
  const columns = useMemo(
    () => [
      {
        title: '任务',
        dataIndex: 'title',
        key: 'title',
        render: (value: string, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text strong>{value}</Text>
            {typeof record.queue_position === 'number' ? (
              <Text type="secondary">{`队列 ${record.queue_position}`}</Text>
            ) : null}
          </Space>
        ),
      },
      {
        title: '用户 / 联系人 / 项目',
        key: 'scope',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>用户: {record.user_id}</Text>
            <Text>
              联系人:
              {' '}
              {getContactDisplayName(record)}
            </Text>
            <Text type="secondary">
              项目:
              {' '}
              {getProjectDisplayName(record)}
            </Text>
          </Space>
        ),
      },
      {
        title: '计划 / 图谱',
        key: 'plan',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>{record.task_plan_id ? '已归属计划' : '未归属计划'}</Text>
            <Text type="secondary">
              {record.task_ref || '-'}
              {record.task_kind ? ` · ${record.task_kind}` : ''}
            </Text>
            <Text type="secondary">
              依赖:
              {' '}
              {record.depends_on_task_ids?.length ?? 0}
              {' / '}
              验证:
              {' '}
              {record.verification_of_task_ids?.length ?? 0}
            </Text>
          </Space>
        ),
      },
      {
        title: '优先级',
        dataIndex: 'priority',
        key: 'priority',
        render: (value: string) => (
          <Tag color={value === 'high' ? 'red' : value === 'low' ? 'default' : 'blue'}>
            {value}
          </Tag>
        ),
      },
      {
        title: '状态',
        dataIndex: 'status',
        key: 'status',
        render: (value: string, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Tag color={formatTaskStatusColor(value)}>{value}</Tag>
            {record.blocked_reason ? (
              <Text type="secondary">{formatBlockedReason(record.blocked_reason)}</Text>
            ) : null}
          </Space>
        ),
      },
      {
        title: '执行模型',
        dataIndex: 'model_config_id',
        key: 'model_config_id',
        render: (value?: string | null) => value || '-',
      },
      {
        title: '计划资源',
        key: 'planned_resources',
        render: (_: unknown, record: ContactTask) => (
          <Space direction="vertical" size={2}>
            <Text>{record.planned_builtin_mcp_ids?.length ?? 0} 个 MCP</Text>
            <Text type="secondary">{record.planned_context_assets?.length ?? 0} 个上下文资产</Text>
          </Space>
        ),
      },
      {
        title: '更新时间',
        dataIndex: 'updated_at',
        key: 'updated_at',
        render: (value: string) => new Date(value).toLocaleString(),
      },
    ],
    [getContactDisplayName, getProjectDisplayName],
  );

  return (
    <Table<ContactTask>
      rowKey="id"
      loading={loading}
      columns={columns}
      dataSource={visibleTasks}
      pagination={{ pageSize: 20, showSizeChanger: false }}
      expandable={{
        expandedRowKeys: expandedTaskIds,
        expandRowByClick: true,
        onExpand: (expanded, record) => {
          const nextExpandedTaskIds = expanded
            ? Array.from(new Set([...expandedTaskIds, record.id]))
            : expandedTaskIds.filter((item) => item !== record.id);
          onExpandedTaskIdsChange(nextExpandedTaskIds);
          if (expanded) {
            void onLoadExecutionMessages(record.id, true);
            void onLoadTaskResultBrief(record.id, true);
          }
        },
        expandedRowRender: (record) => (
          <TaskExpandedRowContent
            record={record}
            formatRelatedTask={formatRelatedTask}
            executionMessages={executionMessagesByTaskId[record.id] || []}
            executionLoading={Boolean(executionLoadingByTaskId[record.id])}
            executionError={executionErrorByTaskId[record.id]}
            executionSectionExpanded={Boolean(executionSectionExpandedByTaskId[record.id])}
            executionPage={executionPageByTaskId[record.id] || 1}
            onExecutionExpandedChange={(expanded) => {
              onExecutionSectionExpandedChange(record.id, expanded);
            }}
            onExecutionRefresh={() => onLoadExecutionMessages(record.id, true)}
            onExecutionPageChange={(page) => {
              onExecutionPageChange(record.id, page);
            }}
            resultBrief={resultBriefByTaskId[record.id]}
            resultBriefLoading={Boolean(resultBriefLoadingByTaskId[record.id])}
            resultBriefError={resultBriefErrorByTaskId[record.id]}
            onResultBriefRefresh={() => onLoadTaskResultBrief(record.id, true)}
          />
        ),
      }}
    />
  );
}

import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  List,
  Segmented,
  Select,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  EngineRecord,
  TaskMemoryContextResponse,
  TaskMemoryRecordsResponse,
  TaskRecord,
} from '../../types';
import {
  JsonBlock,
  memoryRoleColor,
  memorySummaryColor,
} from './taskPageUtils';

export type TaskMemoryRoleFilter = 'all' | 'user' | 'assistant' | 'tool' | 'system';
export type TaskMemorySummaryFilter = 'all' | 'pending' | 'done';

type TaskMemoryDrawerProps = {
  t: TranslateFn;
  task: TaskRecord | null;
  roleFilter: TaskMemoryRoleFilter;
  summaryFilter: TaskMemorySummaryFilter;
  limit: number;
  context?: TaskMemoryContextResponse;
  contextLoading: boolean;
  records?: TaskMemoryRecordsResponse;
  recordsLoading: boolean;
  summarizeLoading: boolean;
  onClose: () => void;
  onRoleFilterChange: (value: TaskMemoryRoleFilter) => void;
  onSummaryFilterChange: (value: TaskMemorySummaryFilter) => void;
  onLimitChange: (value: number) => void;
  onRefresh: () => void;
  onSummarize: (taskId: string) => void;
};

export function TaskMemoryDrawer({
  t,
  task,
  roleFilter,
  summaryFilter,
  limit,
  context,
  contextLoading,
  records,
  recordsLoading,
  summarizeLoading,
  onClose,
  onRoleFilterChange,
  onSummaryFilterChange,
  onLimitChange,
  onRefresh,
  onSummarize,
}: TaskMemoryDrawerProps) {
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

  return (
    <Drawer
      title={task
        ? t('tasks.memory.titleWithName', { title: task.title })
        : t('tasks.memory.title')}
      open={Boolean(task)}
      width={920}
      onClose={onClose}
    >
      {task ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space wrap>
            <Segmented
              value={roleFilter}
              onChange={(value) => onRoleFilterChange(value as TaskMemoryRoleFilter)}
              options={[
                { label: t('tasks.memory.allRoles'), value: 'all' },
                { label: 'user', value: 'user' },
                { label: 'assistant', value: 'assistant' },
                { label: 'tool', value: 'tool' },
                { label: 'system', value: 'system' },
              ]}
            />
            <Segmented
              value={summaryFilter}
              onChange={(value) => onSummaryFilterChange(value as TaskMemorySummaryFilter)}
              options={[
                { label: t('tasks.memory.allSummaryStatuses'), value: 'all' },
                { label: 'pending', value: 'pending' },
                { label: 'done', value: 'done' },
              ]}
            />
            <Select
              value={limit}
              onChange={onLimitChange}
              style={{ width: 140 }}
              options={[
                { label: t('tasks.memory.recentLimit', { count: 20 }), value: 20 },
                { label: t('tasks.memory.recentLimit', { count: 50 }), value: 50 },
                { label: t('tasks.memory.recentLimit', { count: 100 }), value: 100 },
              ]}
            />
            <Button onClick={onRefresh}>{t('common.refresh')}</Button>
            <Button
              loading={summarizeLoading}
              onClick={() => onSummarize(task.id)}
            >
              {t('tasks.memory.triggerSummary')}
            </Button>
          </Space>

          {context?.thread ? (
            <>
              <Descriptions bordered column={1} size="small">
                <Descriptions.Item label={t('tasks.detail.taskId')}>{task.id}</Descriptions.Item>
                <Descriptions.Item label="Memory Thread">
                  <Typography.Text code>{context.memory_thread_id}</Typography.Text>
                </Descriptions.Item>
                <Descriptions.Item label="Tenant">{context.tenant_id}</Descriptions.Item>
                <Descriptions.Item label="Subject">{context.subject_id}</Descriptions.Item>
                <Descriptions.Item label={t('tasks.memory.threadStatus')}>
                  <Tag color="processing">{context.thread.status}</Tag>
                </Descriptions.Item>
                <Descriptions.Item label={t('tasks.memory.summaryStatus')}>
                  <Tag color={memorySummaryColor(context.thread.summary_status)}>
                    {context.thread.summary_status}
                  </Tag>
                </Descriptions.Item>
                <Descriptions.Item label="Pending Records">
                  {context.thread.pending_record_count}
                </Descriptions.Item>
                <Descriptions.Item label="Pending Summary Tokens">
                  {context.thread.pending_summary_tokens}
                </Descriptions.Item>
                <Descriptions.Item label="Total Records">
                  {context.total_record_count}
                </Descriptions.Item>
                <Descriptions.Item label="Summary Job">
                  {context.thread.summary_job_run_id || '-'}
                </Descriptions.Item>
              </Descriptions>

              {context.thread.metadata ? (
                <JsonBlock title={t('tasks.memory.threadMetadata')} value={context.thread.metadata} />
              ) : null}
            </>
          ) : contextLoading ? null : (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.memory.threadNotCreated')} />
          )}

          <div>
            <Typography.Title level={5}>{t('tasks.memory.contextPreview')}</Typography.Title>
            {context?.context ? (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <Space wrap>
                  <Tag color="blue">
                    {t('tasks.memory.summaries', {
                      count: context.context.meta.summary_count,
                    })}
                  </Tag>
                  <Tag color="cyan">
                    {t('tasks.memory.recentRecords', {
                      count: context.context.meta.recent_record_count,
                    })}
                  </Tag>
                </Space>
                <List
                  bordered
                  dataSource={context.context.blocks}
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
            ) : contextLoading ? null : (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.memory.noContext')} />
            )}
          </div>

          <div>
            <Typography.Title level={5}>{t('tasks.memory.records')}</Typography.Title>
            <Table<EngineRecord>
              rowKey="id"
              loading={recordsLoading}
              columns={memoryRecordColumns}
              dataSource={records?.items || []}
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
            {!recordsLoading && !records?.items.length ? (
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
  );
}

import { Space, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';

import type { EngineJobRun } from '../../../types';
import {
  fallbackThreadDisplayName,
  statusColor,
  toLocal,
} from '../../utils';

const { Paragraph, Text } = Typography;

export function runColumns(): TableColumnsType<EngineJobRun> {
  return [
    {
      title: 'ID',
      dataIndex: 'id',
      key: 'id',
      width: 90,
      render: (value: string) => value.slice(0, 8),
    },
    {
      title: '任务类型',
      dataIndex: 'job_type',
      key: 'job_type',
      width: 130,
      render: (value: string) => <Tag>{value}</Tag>,
    },
    {
      title: '触发来源',
      dataIndex: 'trigger_type',
      key: 'trigger_type',
      width: 160,
      render: (value: string) =>
        (
          {
            thread_direct: '线程级直触发',
            subject_direct: '记忆直接触发',
            scheduler: '系统调度',
          }[value] ?? value
        ),
    },
    {
      title: 'Tenant',
      dataIndex: 'tenant_id',
      key: 'tenant_id',
      width: 140,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Source',
      dataIndex: 'source_id',
      key: 'source_id',
      width: 140,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Thread',
      dataIndex: 'thread_id',
      key: 'thread_id',
      width: 220,
      render: (_value: string | null | undefined, record) => {
        if (!record.thread_id) {
          return (
            <Text type="secondary">
              {record.trigger_type === 'scheduler'
                ? '系统调度任务'
                : record.subject_id
                  ? `subject:${record.subject_id}`
                  : '-'}
            </Text>
          );
        }
        const displayName = record.thread_display_name ?? fallbackThreadDisplayName(record);
        return (
          <Space direction="vertical" size={2}>
            <Text strong>{displayName}</Text>
            {displayName !== record.thread_id ? (
              <Text type="secondary">{record.thread_id}</Text>
            ) : null}
          </Space>
        );
      },
    },
    {
      title: 'Subject',
      dataIndex: 'subject_id',
      key: 'subject_id',
      width: 160,
      render: (value?: string | null) => value || '-',
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (value: string) => <Tag color={statusColor(value)}>{value}</Tag>,
    },
    { title: '输入', dataIndex: 'input_count', key: 'input_count', width: 80 },
    { title: '输出', dataIndex: 'output_count', key: 'output_count', width: 80 },
    { title: '处理', dataIndex: 'processed_count', key: 'processed_count', width: 80 },
    { title: '成功', dataIndex: 'success_count', key: 'success_count', width: 80 },
    { title: '失败数', dataIndex: 'error_count', key: 'error_count', width: 80 },
    {
      title: '开始时间',
      dataIndex: 'started_at',
      key: 'started_at',
      width: 180,
      render: toLocal,
    },
    {
      title: '结束时间',
      dataIndex: 'finished_at',
      key: 'finished_at',
      width: 180,
      render: toLocal,
    },
    {
      title: '元数据',
      dataIndex: 'metadata',
      key: 'metadata',
      width: 320,
      render: (value?: Record<string, unknown> | null) =>
        value ? (
          <Paragraph className="engine-pre" ellipsis={{ rows: 6, expandable: true, symbol: '展开' }}>
            {JSON.stringify(value, null, 2)}
          </Paragraph>
        ) : '-',
    },
  ];
}

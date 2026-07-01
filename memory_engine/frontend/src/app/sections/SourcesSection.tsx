// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EditOutlined, KeyOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Space, Table, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';

import type { EngineSource } from '../../types';
import { toLocal } from '../utils';

const { Paragraph, Text } = Typography;

type SourcesSectionProps = {
  sources: EngineSource[];
  loading: boolean;
  onReload: () => void;
  onCreate: () => void;
  onEdit: (source: EngineSource) => void;
  onRotateSecret: (source: EngineSource) => void;
};

export function SourcesSection(props: SourcesSectionProps) {
  const { sources, loading, onReload, onCreate, onEdit, onRotateSecret } = props;

  const columns: TableColumnsType<EngineSource> = [
    { title: '系统标识', dataIndex: 'source_id', key: 'source_id', width: 180 },
    { title: '名称', dataIndex: 'name', key: 'name', width: 180 },
    {
      title: '接入方式',
      dataIndex: 'source_type',
      key: 'source_type',
      width: 140,
      render: () => <Tag color="blue">SDK</Tag>,
    },
    {
      title: '启用',
      dataIndex: 'status',
      key: 'sdk_enabled',
      width: 90,
      render: (_value: unknown, record) => (
        <Tag color={record.status === 'active' ? 'success' : 'default'}>
          {record.status === 'active' ? '是' : '否'}
        </Tag>
      ),
    },
    {
      title: '接入密钥',
      dataIndex: 'secret_key_hint',
      key: 'secret_key_hint',
      width: 140,
      render: (value?: string | null) => (
        <Text type={value ? undefined : 'secondary'}>{value || '未生成'}</Text>
      ),
    },
    {
      title: '最近轮换',
      dataIndex: 'key_last_rotated_at',
      key: 'key_last_rotated_at',
      width: 180,
      render: toLocal,
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
    {
      title: '操作',
      key: 'actions',
      width: 180,
      fixed: 'right',
      render: (_value, record) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => onEdit(record)}>
            编辑
          </Button>
          <Button size="small" icon={<KeyOutlined />} onClick={() => onRotateSecret(record)}>
            轮换密钥
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Card
        title="接入系统"
        extra={
          <Space>
            <Button icon={<ReloadOutlined />} loading={loading} onClick={onReload}>
              刷新
            </Button>
            <Button type="primary" icon={<PlusOutlined />} onClick={onCreate}>
              新增系统
            </Button>
          </Space>
        }
      >
        <Paragraph type="secondary">
          在这里维护接入记忆平台的下游系统。每个系统只需要一个稳定的系统标识和展示名称，平台会为它生成独立的接入密钥。
        </Paragraph>
        <Table
          rowKey="id"
          dataSource={sources}
          loading={loading}
          pagination={{ pageSize: 10 }}
          scroll={{ x: 1600 }}
          columns={columns}
        />
      </Card>
    </Space>
  );
}

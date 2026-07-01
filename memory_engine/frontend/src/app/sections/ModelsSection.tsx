// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Popconfirm, Space, Table, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';

import type { EngineModelProfile } from '../../types';
import { toLocal } from '../utils';

const { Text } = Typography;

export type OwnerLabelMap = Record<
  string,
  {
    username: string;
    display_name: string;
  }
>;

type ModelsSectionProps = {
  models: EngineModelProfile[];
  ownerLabelsById?: OwnerLabelMap;
  loading: boolean;
  onReload: () => void;
  onCreate: () => void;
  onEdit: (model: EngineModelProfile) => void;
  onDelete: (model: EngineModelProfile) => void;
};

export function ModelsSection(props: ModelsSectionProps) {
  const { models, ownerLabelsById = {}, loading, onReload, onCreate, onEdit, onDelete } = props;

  function renderOwnerScope(record: EngineModelProfile) {
    const ownerUserId = record.owner_user_id?.trim();
    if (!ownerUserId) {
      return <Tag>Global</Tag>;
    }
    const owner = ownerLabelsById[ownerUserId];
    const username = owner?.username?.trim() || record.owner_username?.trim();
    const displayName = owner?.display_name?.trim();
    const label = displayName || username || ownerUserId;
    return (
      <Tag color="geekblue">
        {label}
        {username && username !== label ? ` (${username})` : ''}
      </Tag>
    );
  }

  const columns: TableColumnsType<EngineModelProfile> = [
    { title: 'Name', dataIndex: 'name', key: 'name', width: 180 },
    { title: 'Provider', dataIndex: 'provider', key: 'provider', width: 140 },
    { title: 'Model', dataIndex: 'model', key: 'model', width: 220 },
    {
      title: 'Scope',
      key: 'scope',
      width: 160,
      render: (_value, record) => renderOwnerScope(record),
    },
    {
      title: 'Default',
      dataIndex: 'is_default',
      key: 'is_default',
      width: 90,
      render: (value: boolean) =>
        value ? <Tag color="blue">Default</Tag> : <Text type="secondary">-</Text>,
    },
    {
      title: 'Capabilities',
      key: 'capabilities',
      width: 220,
      render: (_value, record) => (
        <Space size={[4, 4]} wrap>
          {record.supports_images ? <Tag color="purple">images</Tag> : null}
          {record.supports_reasoning ? <Tag color="gold">reasoning</Tag> : null}
          {record.supports_responses ? <Tag color="cyan">responses</Tag> : null}
          {!record.supports_images &&
          !record.supports_reasoning &&
          !record.supports_responses ? (
            <Text type="secondary">-</Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: 'Enabled',
      dataIndex: 'enabled',
      key: 'enabled',
      width: 90,
      render: (value: boolean) => (
        <Tag color={value ? 'success' : 'default'}>{value ? 'Yes' : 'No'}</Tag>
      ),
    },
    {
      title: 'Temperature',
      dataIndex: 'temperature',
      key: 'temperature',
      width: 110,
      render: (value?: number | null) => value ?? '-',
    },
    {
      title: 'Updated At',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: toLocal,
    },
    {
      title: 'Actions',
      key: 'actions',
      fixed: 'right',
      width: 160,
      render: (_value, record) => (
        <Space>
          <Button icon={<EditOutlined />} size="small" onClick={() => onEdit(record)}>
            Edit
          </Button>
          <Popconfirm
            title="Delete model profile"
            description={`Delete ${record.name}?`}
            okText="Delete"
            cancelText="Cancel"
            onConfirm={() => onDelete(record)}
          >
            <Button danger icon={<DeleteOutlined />} size="small">
              Delete
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <Card
      title="Model Profiles"
      extra={
        <Space>
          <Button icon={<ReloadOutlined />} loading={loading} onClick={onReload}>
            Reload
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={onCreate}>
            New Model
          </Button>
        </Space>
      }
    >
      <Table
        rowKey="id"
        dataSource={models}
        loading={loading}
        pagination={{ pageSize: 10 }}
        scroll={{ x: 1440 }}
        columns={columns}
      />
    </Card>
  );
}

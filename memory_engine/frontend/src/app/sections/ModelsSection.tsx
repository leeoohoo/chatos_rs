import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { Button, Card, Popconfirm, Space, Table, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';

import type { EngineModelProfile } from '../../types';
import { toLocal } from '../utils';

const { Text } = Typography;

type ModelsSectionProps = {
  models: EngineModelProfile[];
  loading: boolean;
  onReload: () => void;
  onCreate: () => void;
  onEdit: (model: EngineModelProfile) => void;
  onDelete: (model: EngineModelProfile) => void;
};

export function ModelsSection(props: ModelsSectionProps) {
  const { models, loading, onReload, onCreate, onEdit, onDelete } = props;

  const columns: TableColumnsType<EngineModelProfile> = [
    { title: 'Name', dataIndex: 'name', key: 'name', width: 180 },
    { title: 'Provider', dataIndex: 'provider', key: 'provider', width: 140 },
    { title: 'Model', dataIndex: 'model', key: 'model', width: 220 },
    {
      title: 'Scope',
      key: 'scope',
      width: 160,
      render: (_value, record) =>
        record.owner_username || record.owner_user_id ? (
          <Tag color="geekblue">{record.owner_username ?? record.owner_user_id}</Tag>
        ) : (
          <Tag>Global</Tag>
        ),
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

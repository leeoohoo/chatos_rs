// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Button, Popconfirm, Space, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { DeleteOutlined, EditOutlined, ReloadOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';

import type {
  AgentPromptVendor,
  CreateUserModelProviderPayload,
  UpdateUserModelProviderPayload,
  UserModelConfigRecord,
  UserModelProviderRecord,
  UserSummaryRecord,
} from '../../types';

export type ProviderFormValues = {
  owner_user_id?: string;
  name: string;
  provider: string;
  prompt_vendor: AgentPromptVendor;
  api_key?: string;
  clear_api_key?: boolean;
  base_url?: string;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
};

export const PROVIDER_OPTIONS = [
  { label: 'GPT / OpenAI', value: 'gpt' },
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'Kimi', value: 'kimi' },
  { label: 'GLM', value: 'glm' },
];

export const PROMPT_VENDOR_OPTIONS = [
  { label: 'GLM', value: 'glm' },
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'GPT / OpenAI', value: 'gpt' },
  { label: 'Kimi / Moonshot', value: 'kimi' },
];

export function defaultPromptVendor(provider: string): AgentPromptVendor {
  switch (provider.trim().toLowerCase()) {
    case 'glm':
    case 'zhipu':
    case 'zhipuai':
      return 'glm';
    case 'deepseek':
      return 'deepseek';
    case 'kimi':
    case 'moonshot':
      return 'kimi';
    default:
      return 'gpt';
  }
}

export const ALL_USERS_SCOPE = '__all_users__';

export function providerCatalogStatusLabel(status?: string | null) {
  switch (status) {
    case 'ok':
      return 'loaded';
    case 'error':
      return 'failed';
    case 'empty':
      return 'empty';
    default:
      return 'not fetched';
  }
}

export function userLabel(users: UserSummaryRecord[] | undefined, ownerUserId: string): string {
  const owner = (users || []).find((item) => item.id === ownerUserId);
  return owner ? `${owner.display_name || owner.username} (${owner.username})` : ownerUserId;
}

export function buildCreateProviderPayload({
  values,
  isSuperAdmin,
  selectedUserId,
}: {
  values: ProviderFormValues;
  isSuperAdmin: boolean;
  selectedUserId?: string;
}): CreateUserModelProviderPayload {
  const normalized = normalizeProviderValues(values);
  return {
    owner_user_id: isSuperAdmin ? values.owner_user_id : selectedUserId,
    ...normalized,
    base_url: normalized.base_url || undefined,
  };
}

export function buildUpdateProviderPayload(
  values: ProviderFormValues,
): UpdateUserModelProviderPayload {
  const normalized = normalizeProviderValues(values);
  return {
    ...normalized,
    clear_api_key: values.clear_api_key === true,
    base_url: normalized.base_url ?? '',
  };
}

function normalizeProviderValues(values: ProviderFormValues) {
  return {
    name: values.name.trim(),
    provider: values.provider,
    prompt_vendor: values.prompt_vendor,
    api_key: values.api_key?.trim() || undefined,
    base_url: values.base_url?.trim() || undefined,
    enabled: values.enabled,
    supports_images: values.supports_images,
    supports_reasoning: values.supports_reasoning,
    supports_responses: values.supports_responses,
  };
}

export function buildProviderColumns({
  users,
  onRefresh,
  onEdit,
  onDelete,
  deleteLoading,
}: {
  users?: UserSummaryRecord[];
  onRefresh: (record: UserModelProviderRecord) => void;
  onEdit: (record: UserModelProviderRecord) => void;
  onDelete: (id: string) => void;
  deleteLoading: boolean;
}): ColumnsType<UserModelProviderRecord> {
  return [
    {
      title: 'Provider',
      dataIndex: 'name',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8} wrap>
            <Typography.Text strong>{record.name}</Typography.Text>
            {record.has_api_key ? <Tag color="blue">Key Saved</Tag> : <Tag>Missing Key</Tag>}
          </Space>
          <Typography.Text type="secondary">
            {record.provider}
            {record.base_url ? ` | ${record.base_url}` : ''}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: 'Owner',
      dataIndex: 'owner_user_id',
      width: 220,
      render: (ownerUserId: string) => userLabel(users, ownerUserId),
    },
    {
      title: 'Catalog',
      key: 'catalog',
      width: 260,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8} wrap>
            <Tag color={record.last_sync_status === 'ok' ? 'success' : 'default'}>
              {providerCatalogStatusLabel(record.last_sync_status)}
            </Tag>
            <Tag>{record.imported_model_count || 0} models</Tag>
          </Space>
          {record.last_sync_error ? (
            <Typography.Text type="danger" ellipsis={{ tooltip: record.last_sync_error }}>
              {record.last_sync_error}
            </Typography.Text>
          ) : record.last_synced_at ? (
            <Typography.Text type="secondary">
              {dayjs(record.last_synced_at).format('YYYY-MM-DD HH:mm:ss')}
            </Typography.Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: 'Flags',
      key: 'flags',
      width: 220,
      render: (_, record) => <ModelCapabilityTags record={record} />,
    },
    {
      title: 'Updated',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: 'Actions',
      key: 'actions',
      width: 240,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<ReloadOutlined />} onClick={() => onRefresh(record)}>
            Refresh
          </Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => onEdit(record)}>
            Edit
          </Button>
          <Popconfirm
            title="Delete this provider and its imported models?"
            onConfirm={() => onDelete(record.id)}
            okButtonProps={{ loading: deleteLoading }}
          >
            <Button size="small" danger icon={<DeleteOutlined />}>
              Delete
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];
}

export function buildModelColumns(
  users?: UserSummaryRecord[],
): ColumnsType<UserModelConfigRecord> {
  return [
    {
      title: 'Concrete Model',
      dataIndex: 'name',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{record.name}</Typography.Text>
          <Typography.Text type="secondary">
            {record.provider} | {record.model_name}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: 'Owner',
      dataIndex: 'owner_user_id',
      width: 220,
      render: (ownerUserId: string) => userLabel(users, ownerUserId),
    },
    {
      title: 'Task Usage',
      dataIndex: 'task_usage_scenario',
      width: 220,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Task Thinking',
      dataIndex: 'task_thinking_level',
      width: 160,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Flags',
      key: 'flags',
      width: 220,
      render: (_, record) => <ModelCapabilityTags record={record} />,
    },
    {
      title: 'Updated',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
  ];
}

function ModelCapabilityTags({
  record,
}: {
  record: Pick<
    UserModelConfigRecord | UserModelProviderRecord,
    'enabled' | 'supports_images' | 'supports_reasoning' | 'supports_responses'
  >;
}) {
  return (
    <Space wrap>
      <Tag color={record.enabled ? 'success' : 'default'}>
        {record.enabled ? 'Enabled' : 'Disabled'}
      </Tag>
      {record.supports_images ? <Tag>Image</Tag> : null}
      {record.supports_reasoning ? <Tag>Reasoning</Tag> : null}
      {record.supports_responses ? <Tag>Responses</Tag> : null}
    </Space>
  );
}

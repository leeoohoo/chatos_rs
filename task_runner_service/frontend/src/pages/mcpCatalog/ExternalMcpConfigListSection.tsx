import {
  Alert,
  Button,
  Empty,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ExternalMcpConfigRecord } from '../../types';
import { MCP_CARD_STYLE } from './mcpCatalogPageUtils';

type ExternalMcpConfigListSectionProps = {
  t: TranslateFn;
  configs: ExternalMcpConfigRecord[];
  loading: boolean;
  onCreate: () => void;
  onEdit: (config: ExternalMcpConfigRecord) => void;
  onDelete: (config: ExternalMcpConfigRecord) => void;
};

export function ExternalMcpConfigListSection({
  t,
  configs,
  loading,
  onCreate,
  onEdit,
  onDelete,
}: ExternalMcpConfigListSectionProps) {
  const columns: ColumnsType<ExternalMcpConfigRecord> = [
    {
      title: t('common.name'),
      dataIndex: 'name',
      width: 220,
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{value}</Typography.Text>
          <Typography.Text type="secondary">{record.id}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('mcpCatalog.externalConfigTransport'),
      dataIndex: 'transport',
      width: 120,
      render: (value: string) => <Tag color={value === 'http' ? 'blue' : 'geekblue'}>{value}</Tag>,
    },
    {
      title: t('mcpCatalog.externalConfigEndpoint'),
      key: 'endpoint',
      render: (_, record) => (
        <Typography.Text code>
          {record.transport === 'http'
            ? record.url || '-'
            : [record.command, ...(record.args || [])].filter(Boolean).join(' ') || '-'}
        </Typography.Text>
      ),
    },
    {
      title: t('servers.detail.creator'),
      key: 'creator',
      width: 160,
      render: (_, record) =>
        record.creator_display_name || record.creator_username || record.creator_user_id || '-',
    },
    {
      title: t('common.status'),
      dataIndex: 'enabled',
      width: 120,
      render: (enabled: boolean) => (
        <Tag color={enabled ? 'success' : 'default'}>
          {enabled ? t('common.enabled') : t('common.disabled')}
        </Tag>
      ),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 180,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => onEdit(record)}>
            {t('common.edit')}
          </Button>
          <Button size="small" danger onClick={() => onDelete(record)}>
            {t('common.delete')}
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Space direction="vertical" size="middle" style={MCP_CARD_STYLE}>
      <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            {t('mcpCatalog.externalConfigTitle')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('mcpCatalog.externalConfigSubtitle')}
          </Typography.Text>
        </Space>
        <Button type="primary" onClick={onCreate}>
          {t('mcpCatalog.addExternalConfig')}
        </Button>
      </Space>

      <Alert
        showIcon
        type="info"
        message={t('mcpCatalog.externalConfigReadyTitle')}
        description={t('mcpCatalog.externalConfigReadyDescription')}
      />

      <Table<ExternalMcpConfigRecord>
        rowKey="id"
        columns={columns}
        dataSource={configs}
        loading={loading}
        pagination={false}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={t('mcpCatalog.externalConfigEmpty')}
            />
          ),
        }}
      />
    </Space>
  );
}

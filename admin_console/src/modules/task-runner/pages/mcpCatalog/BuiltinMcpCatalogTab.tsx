// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useQuery } from '@tanstack/react-query';
import {
  Collapse,
  Empty,
  List,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import type { McpCatalogEntry } from '../../types';

export function BuiltinMcpCatalogTab() {
  const { t } = useI18n();
  const catalogQuery = useQuery({
    queryKey: ['task-runner', 'mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });

  const columns: ColumnsType<McpCatalogEntry> = [
    {
      title: 'Builtin Kind',
      dataIndex: 'kind',
      width: 220,
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{value}</Typography.Text>
          <Typography.Text type="secondary">{record.server_name}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('mcpCatalog.column.status'),
      dataIndex: 'implemented',
      width: 140,
      render: (implemented: boolean) => (
        <Tag color={implemented ? 'success' : 'warning'}>
          {implemented ? 'implemented' : 'planned'}
        </Tag>
      ),
    },
    {
      title: 'Runtime Default',
      dataIndex: 'runtime_default',
      width: 140,
      render: (runtimeDefault: boolean) =>
        runtimeDefault ? <Tag color="blue">default</Tag> : <Tag>optional</Tag>,
    },
    {
      title: t('mcpCatalog.column.writes'),
      dataIndex: 'default_allow_writes',
      width: 120,
      render: (allowWrites: boolean) =>
        allowWrites ? <Tag color="volcano">write</Tag> : <Tag color="default">read-only</Tag>,
    },
    {
      title: t('mcpCatalog.column.toolCount'),
      key: 'tool_count',
      width: 140,
      render: (_, record) => record.available_tool_names.length,
    },
    {
      title: t('common.description'),
      dataIndex: 'description',
      render: (_: string, record) => (
        <Space direction="vertical" size={4}>
          <Typography.Text>{record.description || '-'}</Typography.Text>
          {record.use_cases.length ? (
            <Space size={4} wrap>
              {record.use_cases.map((item) => (
                <Tag key={item}>{item}</Tag>
              ))}
            </Space>
          ) : null}
          {record.capabilities.length ? (
            <Typography.Text type="secondary">
              {record.capabilities.join(' / ')}
            </Typography.Text>
          ) : null}
          {record.message ? (
            <Typography.Text type="secondary">{record.message}</Typography.Text>
          ) : null}
        </Space>
      ),
    },
  ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Table<McpCatalogEntry>
        rowKey="kind"
        loading={catalogQuery.isLoading}
        columns={columns}
        dataSource={catalogQuery.data || []}
        pagination={false}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={t('mcpCatalog.emptyCatalog')}
            />
          ),
        }}
        expandable={{
          expandedRowRender: (record) => (
            <Collapse
              ghost
              items={[
                {
                  key: 'available-tools',
                  label: t('mcpCatalog.availableTools', {
                    count: record.available_tool_names.length,
                  }),
                  children: record.available_tool_names.length ? (
                    <List
                      size="small"
                      dataSource={record.available_tool_names}
                      renderItem={(item) => <List.Item>{item}</List.Item>}
                    />
                  ) : (
                    <Typography.Text type="secondary">{t('common.noData')}</Typography.Text>
                  ),
                },
                {
                  key: 'unavailable-tools',
                  label: t('mcpCatalog.unavailableTools', {
                    count: record.unavailable_tools.length,
                  }),
                  children: record.unavailable_tools.length ? (
                    <List
                      size="small"
                      dataSource={record.unavailable_tools}
                      renderItem={(item) => (
                        <List.Item>
                          <Space direction="vertical" size={0}>
                            <Typography.Text>{item.name}</Typography.Text>
                            <Typography.Text type="secondary">
                              {item.reason}
                            </Typography.Text>
                          </Space>
                        </List.Item>
                      )}
                    />
                  ) : (
                    <Typography.Text type="secondary">{t('common.noData')}</Typography.Text>
                  ),
                },
              ]}
            />
          ),
        }}
      />
    </Space>
  );
}

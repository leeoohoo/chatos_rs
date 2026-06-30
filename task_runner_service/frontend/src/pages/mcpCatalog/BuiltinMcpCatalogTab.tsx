import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import {
  Button,
  Collapse,
  Descriptions,
  Empty,
  List,
  Segmented,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../../api/client';
import { McpPromptPreviewCard } from '../../components/McpPromptPreviewCard';
import { useI18n } from '../../i18n/I18nProvider';
import type { McpCatalogEntry, TaskBuiltinPromptMode } from '../../types';
import { MCP_CARD_STYLE } from './mcpCatalogPageUtils';

export function BuiltinMcpCatalogTab() {
  const { locale, t } = useI18n();
  const navigate = useNavigate();
  const [mcpEnabled, setMcpEnabled] = useState(true);
  const [promptMode, setPromptMode] = useState<TaskBuiltinPromptMode>('effective');
  const [promptLocale, setPromptLocale] = useState(locale);
  const [selectedKinds, setSelectedKinds] = useState<string[]>([]);
  const catalogQuery = useQuery({
    queryKey: ['mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });
  const remoteServersQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const promptPreviewQuery = useQuery({
    queryKey: ['mcp-prompt-preview', mcpEnabled, promptMode, promptLocale, selectedKinds],
    queryFn: () =>
      api.previewMcpPrompt({
        enabled: mcpEnabled,
        init_mode: 'full',
        builtin_prompt_mode: promptMode,
        builtin_prompt_locale: promptLocale,
        enabled_builtin_kinds: selectedKinds.length ? selectedKinds : undefined,
      }),
  });
  const kindOptions = useMemo(
    () =>
      (catalogQuery.data || []).map((entry) => ({
        label: entry.kind,
        value: entry.kind,
        disabled: !entry.implemented,
      })),
    [catalogQuery.data],
  );
  const remoteControllerEntry = useMemo(
    () =>
      (catalogQuery.data || []).find((entry) => entry.kind === 'RemoteConnectionController') ||
      null,
    [catalogQuery.data],
  );
  const remoteServerSummary = useMemo(() => {
    const items = remoteServersQuery.data || [];
    return {
      total: items.length,
      enabled: items.filter((item) => item.enabled).length,
      testedSuccess: items.filter((item) => item.last_test_status === 'success').length,
      strict: items.filter((item) => item.host_key_policy === 'strict').length,
    };
  }, [remoteServersQuery.data]);

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
      <Space direction="vertical" size="middle" style={MCP_CARD_STYLE}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
          <Space direction="vertical" size={0}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              RemoteConnectionController
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('mcpCatalog.remoteSubtitle')}
            </Typography.Text>
          </Space>
          <Space>
            <Button onClick={() => remoteServersQuery.refetch()}>
              {t('mcpCatalog.refreshServers')}
            </Button>
            <Button type="primary" onClick={() => navigate('/servers')}>
              {t('mcpCatalog.manageServers')}
            </Button>
          </Space>
        </Space>

        <Space size="large" wrap>
          <Statistic title={t('mcpCatalog.serverTotal')} value={remoteServerSummary.total} />
          <Statistic title={t('mcpCatalog.serverEnabled')} value={remoteServerSummary.enabled} />
          <Statistic title={t('mcpCatalog.testSuccess')} value={remoteServerSummary.testedSuccess} />
          <Statistic title={t('mcpCatalog.strictCheck')} value={remoteServerSummary.strict} />
        </Space>

        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label={t('mcpCatalog.builtinStatus')}>
            {remoteControllerEntry ? (
              <Tag color={remoteControllerEntry.implemented ? 'success' : 'warning'}>
                {remoteControllerEntry.implemented ? 'implemented' : 'planned'}
              </Tag>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label={t('mcpCatalog.availableToolCount')}>
            {remoteControllerEntry?.available_tool_names.length ?? 0}
          </Descriptions.Item>
          <Descriptions.Item label={t('common.description')}>
            {remoteControllerEntry?.message ||
              t('mcpCatalog.remoteDescriptionFallback')}
          </Descriptions.Item>
        </Descriptions>
      </Space>

      <Space direction="vertical" size="middle" style={MCP_CARD_STYLE}>
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            {t('mcpCatalog.promptPreviewTitle')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('mcpCatalog.promptPreviewSubtitle')}
          </Typography.Text>
        </Space>

        <Space wrap>
          <Space size={8}>
            <Typography.Text type="secondary">{t('mcpCatalog.enableMcp')}</Typography.Text>
            <Switch checked={mcpEnabled} onChange={setMcpEnabled} />
          </Space>
          <Segmented
            value={promptMode}
            onChange={(value) => setPromptMode(value as TaskBuiltinPromptMode)}
            options={[
              { label: 'effective', value: 'effective' },
              { label: 'configured', value: 'configured' },
            ]}
          />
          <Select
            style={{ width: 140 }}
            value={promptLocale}
            onChange={setPromptLocale}
            options={[
              { label: t('mcp.promptLanguage.zhCN'), value: 'zh-CN' },
              { label: t('mcp.promptLanguage.enUS'), value: 'en-US' },
            ]}
          />
          <Select
            mode="multiple"
            allowClear
            placeholder={t('mcpCatalog.kindsPlaceholder')}
            style={{ minWidth: 320 }}
            value={selectedKinds}
            options={kindOptions}
            onChange={(value) => setSelectedKinds(value)}
          />
        </Space>

        {promptPreviewQuery.data ? (
          <McpPromptPreviewCard preview={promptPreviewQuery.data} />
        ) : null}
      </Space>

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

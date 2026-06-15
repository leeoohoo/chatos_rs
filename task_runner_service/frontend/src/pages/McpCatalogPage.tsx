import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Alert,
  Button,
  Collapse,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  List,
  Modal,
  Segmented,
  Select,
  Space,
  Statistic,
  Switch,
  Table,
  Tabs,
  Tag,
  Typography,
  message,
} from 'antd';
import { useMemo, useState } from 'react';
import type { ColumnsType } from 'antd/es/table';
import { useNavigate } from 'react-router-dom';

import { api, buildApiUrl } from '../api/client';
import { McpPromptPreviewCard } from '../components/McpPromptPreviewCard';
import { useI18n } from '../i18n/I18nProvider';
import type {
  CreateExternalMcpConfigPayload,
  ExternalMcpConfigRecord,
  ExternalMcpTransport,
  McpCatalogEntry,
  McpServerInfo,
  McpServerToolProfileInfo,
  TaskBuiltinPromptMode,
  TaskMcpInitMode,
  UpdateExternalMcpConfigPayload,
} from '../types';

const { TextArea } = Input;

type ExternalMcpConfigFormValues = {
  name: string;
  transport: ExternalMcpTransport;
  command?: string;
  argsText?: string;
  url?: string;
  headersText?: string;
  envText?: string;
  cwd?: string;
  enabled?: boolean;
};

const CARD_STYLE = {
  width: '100%',
  padding: 16,
  borderRadius: 6,
  background: '#fff',
  border: '1px solid #f0f0f0',
};

const TOOL_PROFILE_COLORS: Record<string, string> = {
  admin_full: 'volcano',
  agent_default: 'blue',
  chatos_async_planner: 'geekblue',
};

export function McpCatalogPage() {
  const { locale, t } = useI18n();
  const navigate = useNavigate();
  const [mcpEnabled, setMcpEnabled] = useState(true);
  const [initMode, setInitMode] = useState<TaskMcpInitMode>('builtin_only');
  const [promptMode, setPromptMode] = useState<TaskBuiltinPromptMode>('effective');
  const [promptLocale, setPromptLocale] = useState(locale);
  const [selectedKinds, setSelectedKinds] = useState<string[]>([]);
  const serverInfoQuery = useQuery({
    queryKey: ['mcp-server-info'],
    queryFn: api.getMcpServerInfo,
  });
  const catalogQuery = useQuery({
    queryKey: ['mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });
  const remoteServersQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const promptPreviewQuery = useQuery({
    queryKey: ['mcp-prompt-preview', mcpEnabled, initMode, promptMode, promptLocale, selectedKinds],
    queryFn: () =>
      api.previewMcpPrompt({
        enabled: mcpEnabled,
        init_mode: initMode,
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
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('mcpCatalog.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('mcpCatalog.subtitle')}
        </Typography.Text>
      </Space>

      <Tabs
        items={[
          {
            key: 'external-server',
            label: t('mcpCatalog.tab.externalServer'),
            children: serverInfoQuery.data ? (
              <ExternalMcpServerCard
                info={serverInfoQuery.data}
                onRefresh={() => serverInfoQuery.refetch()}
              />
            ) : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('common.noData')}
              />
            ),
          },
          {
            key: 'builtin',
            label: t('mcpCatalog.tab.builtin'),
            children: (
              <Space direction="vertical" size="large" style={{ width: '100%' }}>
                <Space direction="vertical" size="middle" style={CARD_STYLE}>
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

                <Space direction="vertical" size="middle" style={CARD_STYLE}>
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
                    <Select
                      style={{ width: 160 }}
                      value={initMode}
                      onChange={(value) => setInitMode(value)}
                      options={[
                        { label: 'builtin_only', value: 'builtin_only' },
                        { label: 'full', value: 'full' },
                        { label: 'disabled', value: 'disabled' },
                      ]}
                    />
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
            ),
          },
          {
            key: 'external-configs',
            label: t('mcpCatalog.tab.externalConfigs'),
            children: <ExternalMcpConfigTab />,
          },
        ]}
      />
    </Space>
  );
}

function ExternalMcpServerCard({
  info,
  onRefresh,
}: {
  info: McpServerInfo;
  onRefresh: () => void;
}) {
  const { t } = useI18n();
  const profiles =
    info.tool_profiles && info.tool_profiles.length
      ? info.tool_profiles
      : [
          {
            key: 'admin_full',
            label: 'Admin / full metadata',
            description: 'Complete server metadata list before user/profile access filtering.',
            tool_names: info.tool_names,
          },
        ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size="middle" style={CARD_STYLE}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
          <Space direction="vertical" size={0}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              {t('mcpCatalog.externalServerTitle')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('mcpCatalog.externalServerSubtitle')}
            </Typography.Text>
          </Space>
          <Button onClick={onRefresh}>{t('common.refresh')}</Button>
        </Space>

        <Space wrap>
          {info.transports.map((transport) => (
            <Tag key={transport} color="blue">
              {transport}
            </Tag>
          ))}
        </Space>

        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label="HTTP Endpoint">
            {info.http_endpoint_path ? (
              <Typography.Text code>{buildApiUrl(info.http_endpoint_path)}</Typography.Text>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label="stdio Command">
            {info.stdio_command ? (
              <Typography.Text code>
                {[info.stdio_command, ...info.stdio_args].join(' ')}
              </Typography.Text>
            ) : (
              '-'
            )}
          </Descriptions.Item>
          <Descriptions.Item label={t('mcpCatalog.metadataToolCount')}>
            {info.tool_names.length}
          </Descriptions.Item>
        </Descriptions>
      </Space>

      <Space direction="vertical" size="middle" style={CARD_STYLE}>
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            {t('mcpCatalog.profileToolsTitle')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('mcpCatalog.profileToolsSubtitle')}
          </Typography.Text>
        </Space>

        <Collapse
          items={profiles.map((profile) => ({
            key: profile.key,
            label: (
              <Space wrap>
                <Typography.Text strong>{profileLabel(profile, t)}</Typography.Text>
                <Tag color={TOOL_PROFILE_COLORS[profile.key] || 'default'}>
                  {t('mcpCatalog.toolCount', { count: profile.tool_names.length })}
                </Tag>
                <Typography.Text type="secondary">
                  {profileDescription(profile, t)}
                </Typography.Text>
              </Space>
            ),
            children: <ToolNameList names={profile.tool_names} />,
          }))}
        />
      </Space>
    </Space>
  );
}

function ExternalMcpConfigTab() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [form] = Form.useForm<ExternalMcpConfigFormValues>();
  const transport = Form.useWatch('transport', form) || 'stdio';
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<ExternalMcpConfigRecord | null>(null);
  const configsQuery = useQuery({
    queryKey: ['external-mcp-configs'],
    queryFn: api.listExternalMcpConfigs,
  });
  const createMutation = useMutation({
    mutationFn: api.createExternalMcpConfig,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['external-mcp-configs'] });
      messageApi.success(t('mcpCatalog.externalConfigCreated'));
      closeDrawer();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const updateMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateExternalMcpConfigPayload }) =>
      api.updateExternalMcpConfig(id, payload),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['external-mcp-configs'] });
      messageApi.success(t('mcpCatalog.externalConfigUpdated'));
      closeDrawer();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const deleteMutation = useMutation({
    mutationFn: api.deleteExternalMcpConfig,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['external-mcp-configs'] });
      messageApi.success(t('mcpCatalog.externalConfigDeleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

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
          <Button size="small" onClick={() => openEditDrawer(record)}>
            {t('common.edit')}
          </Button>
          <Button size="small" danger onClick={() => confirmDelete(record)}>
            {t('common.delete')}
          </Button>
        </Space>
      ),
    },
  ];

  function openCreateDrawer() {
    setEditingConfig(null);
    form.setFieldsValue({
      name: '',
      transport: 'stdio',
      command: '',
      argsText: '',
      url: '',
      headersText: '{}',
      envText: '{}',
      cwd: '',
      enabled: true,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(config: ExternalMcpConfigRecord) {
    setEditingConfig(config);
    form.setFieldsValue({
      name: config.name,
      transport: config.transport === 'http' ? 'http' : 'stdio',
      command: config.command || '',
      argsText: (config.args || []).join('\n'),
      url: config.url || '',
      headersText: JSON.stringify(config.headers || {}, null, 2),
      envText: JSON.stringify(config.env || {}, null, 2),
      cwd: config.cwd || '',
      enabled: config.enabled,
    });
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingConfig(null);
    form.resetFields();
  }

  function confirmDelete(config: ExternalMcpConfigRecord) {
    Modal.confirm({
      title: t('mcpCatalog.externalConfigDeleteTitle', { name: config.name }),
      content: t('mcpCatalog.externalConfigDeleteContent'),
      okButtonProps: { danger: true },
      onOk: () => deleteMutation.mutate(config.id),
    });
  }

  function handleSubmit(values: ExternalMcpConfigFormValues) {
    let payload: CreateExternalMcpConfigPayload;
    try {
      payload = buildExternalMcpConfigPayload(values);
    } catch (error) {
      messageApi.error(error instanceof Error ? error.message : String(error));
      return;
    }

    if (editingConfig) {
      updateMutation.mutate({ id: editingConfig.id, payload });
    } else {
      createMutation.mutate(payload);
    }
  }

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      {contextHolder}

      <Space direction="vertical" size="middle" style={CARD_STYLE}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
          <Space direction="vertical" size={0}>
            <Typography.Title level={5} style={{ margin: 0 }}>
              {t('mcpCatalog.externalConfigTitle')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('mcpCatalog.externalConfigSubtitle')}
            </Typography.Text>
          </Space>
          <Button type="primary" onClick={openCreateDrawer}>
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
          dataSource={configsQuery.data || []}
          loading={configsQuery.isLoading}
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

      <Space direction="vertical" size="middle" style={CARD_STYLE}>
        <Typography.Title level={5} style={{ margin: 0 }}>
          {t('mcpCatalog.externalConfigRoadmapTitle')}
        </Typography.Title>
        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label={t('mcpCatalog.externalConfigRuntime')}>
            <Tag color="success">{t('mcpCatalog.externalConfigRuntimeReady')}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label={t('mcpCatalog.externalConfigStorage')}>
            <Tag color="success">{t('mcpCatalog.externalConfigStorageReady')}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label={t('mcpCatalog.externalConfigTaskBinding')}>
            <Tag color="success">{t('mcpCatalog.externalConfigTaskBindingReady')}</Tag>
          </Descriptions.Item>
        </Descriptions>
      </Space>

      <Drawer
        title={
          editingConfig
            ? t('mcpCatalog.externalConfigEditTitle')
            : t('mcpCatalog.externalConfigCreateTitle')
        }
        open={drawerOpen}
        onClose={closeDrawer}
        width={640}
        destroyOnClose
        extra={
          <Space>
            <Button onClick={closeDrawer}>{t('common.cancel')}</Button>
            <Button
              type="primary"
              loading={createMutation.isPending || updateMutation.isPending}
              onClick={() => form.submit()}
            >
              {t('common.save')}
            </Button>
          </Space>
        }
      >
        <Form<ExternalMcpConfigFormValues>
          layout="vertical"
          form={form}
          onFinish={handleSubmit}
        >
          <Form.Item
            name="name"
            label={t('common.name')}
            rules={[{ required: true, message: t('mcpCatalog.externalConfigNameRequired') }]}
          >
            <Input placeholder="filesystem / jira / internal-search" />
          </Form.Item>

          <Space align="start" style={{ width: '100%' }}>
            <Form.Item
              name="transport"
              label={t('mcpCatalog.externalConfigTransport')}
              rules={[{ required: true }]}
            >
              <Select
                style={{ width: 160 }}
                options={[
                  { label: 'stdio', value: 'stdio' },
                  { label: 'http', value: 'http' },
                ]}
              />
            </Form.Item>
            <Form.Item
              name="enabled"
              label={t('common.status')}
              valuePropName="checked"
            >
              <Switch checkedChildren={t('common.enabled')} unCheckedChildren={t('common.disabled')} />
            </Form.Item>
          </Space>

          {transport === 'http' ? (
            <>
              <Form.Item
                name="url"
                label="URL"
                rules={[{ required: true, message: t('mcpCatalog.externalConfigUrlRequired') }]}
              >
                <Input placeholder="http://127.0.0.1:3001/mcp" />
              </Form.Item>
              <Form.Item name="headersText" label="Headers JSON">
                <TextArea rows={5} placeholder='{"Authorization": "Bearer ..."}' />
              </Form.Item>
            </>
          ) : (
            <>
              <Form.Item
                name="command"
                label={t('mcpCatalog.externalConfigCommand')}
                rules={[{ required: true, message: t('mcpCatalog.externalConfigCommandRequired') }]}
              >
                <Input placeholder="npx / node / python" />
              </Form.Item>
              <Form.Item name="argsText" label={t('mcpCatalog.externalConfigArgs')}>
                <TextArea rows={4} placeholder={'-y\n@modelcontextprotocol/server-filesystem\n/Users/me/project'} />
              </Form.Item>
              <Form.Item name="cwd" label="cwd">
                <Input placeholder="/Users/me/project" />
              </Form.Item>
              <Form.Item name="envText" label="Env JSON">
                <TextArea rows={5} placeholder='{"TOKEN": "..."}' />
              </Form.Item>
            </>
          )}
        </Form>
      </Drawer>
    </Space>
  );
}

function buildExternalMcpConfigPayload(
  values: ExternalMcpConfigFormValues,
): CreateExternalMcpConfigPayload {
  const transport = values.transport || 'stdio';
  const command = values.command?.trim() || '';
  const url = values.url?.trim() || '';
  const cwd = values.cwd?.trim() || '';
  const base = {
    name: values.name?.trim() || '',
    transport,
    enabled: values.enabled ?? true,
  };
  if (transport === 'http') {
    return {
      ...base,
      command: '',
      args: [],
      url,
      headers: parseStringMapJson(values.headersText, 'Headers JSON'),
      env: {},
      cwd: '',
    };
  }
  return {
    ...base,
    command,
    args: parseLines(values.argsText),
    url: '',
    headers: {},
    cwd,
    env: parseStringMapJson(values.envText, 'Env JSON'),
  };
}

function parseLines(value?: string): string[] {
  return (value || '')
    .split('\n')
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseStringMapJson(value: string | undefined, label: string): Record<string, string> {
  const trimmed = (value || '').trim();
  if (!trimmed) {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    throw new Error(`${label} must be valid JSON`);
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error(`${label} must be a JSON object`);
  }
  return Object.fromEntries(
    Object.entries(parsed as Record<string, unknown>)
      .map(([key, item]) => [key.trim(), String(item).trim()])
      .filter(([key]) => key.length > 0),
  );
}

function ToolNameList({ names }: { names: string[] }) {
  const { t } = useI18n();

  if (!names.length) {
    return <Typography.Text type="secondary">{t('common.noData')}</Typography.Text>;
  }

  return (
    <Space wrap size={[6, 6]}>
      {names.map((name) => (
        <Tag key={name}>{name}</Tag>
      ))}
    </Space>
  );
}

function profileLabel(
  profile: McpServerToolProfileInfo,
  t: ReturnType<typeof useI18n>['t'],
): string {
  if (profile.key === 'admin_full') {
    return t('mcpCatalog.profile.adminFull');
  }
  if (profile.key === 'agent_default') {
    return t('mcpCatalog.profile.agentDefault');
  }
  if (profile.key === 'chatos_async_planner') {
    return t('mcpCatalog.profile.chatosAsyncPlanner');
  }
  return profile.label;
}

function profileDescription(
  profile: McpServerToolProfileInfo,
  t: ReturnType<typeof useI18n>['t'],
): string {
  if (profile.key === 'admin_full') {
    return t('mcpCatalog.profile.adminFullDescription');
  }
  if (profile.key === 'agent_default') {
    return t('mcpCatalog.profile.agentDefaultDescription');
  }
  if (profile.key === 'chatos_async_planner') {
    return t('mcpCatalog.profile.chatosAsyncPlannerDescription');
  }
  return profile.description;
}

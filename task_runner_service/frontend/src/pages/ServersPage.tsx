import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';
import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Segmented,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n, type TranslateFn } from '../i18n/I18nProvider';
import type {
  CreateRemoteServerPayload,
  RemoteServerAuthType,
  RemoteServerRecord,
  RemoteServerTestResponse,
  TestRemoteServerPayload,
  UpdateRemoteServerPayload,
} from '../types';

type RemoteServerFormValues = {
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_type: RemoteServerAuthType;
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy: 'accept_new' | 'strict';
  enabled: boolean;
};

const authTypeLabelKeys: Record<RemoteServerAuthType, string> = {
  password: 'servers.auth.password',
  private_key: 'servers.auth.privateKey',
  private_key_cert: 'servers.auth.privateKeyCert',
};

const HOST_KEY_POLICY_OPTIONS = [
  { label: 'accept_new', value: 'accept_new' },
  { label: 'strict', value: 'strict' },
] as const;

function serverCreatorLabel(server: RemoteServerRecord): string {
  return server.creator_display_name || server.creator_username || server.creator_user_id || '-';
}

export function ServersPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<RemoteServerRecord | null>(null);
  const [testResult, setTestResult] = useState<RemoteServerTestResponse | null>(null);
  const [keywordFilter, setKeywordFilter] = useState('');
  const [authTypeFilter, setAuthTypeFilter] = useState<'all' | RemoteServerAuthType>('all');
  const [enabledFilter, setEnabledFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [testingServerId, setTestingServerId] = useState<string | null>(null);
  const [form] = Form.useForm<RemoteServerFormValues>();
  const routeServerId = searchParams.get('server_id') || undefined;
  const authType = Form.useWatch('auth_type', form) || 'password';
  const authTypeOptions = useMemo(
    () => (Object.keys(authTypeLabelKeys) as RemoteServerAuthType[]).map((value) => ({
      label: t(authTypeLabelKeys[value]),
      value,
    })),
    [t],
  );
  const authTypeFilterOptions = useMemo(
    () => [
      { label: t('servers.auth.all'), value: 'all' },
      ...authTypeOptions,
    ],
    [authTypeOptions, t],
  );
  const enabledFilterOptions = useMemo(
    () => [
      { label: t('servers.filter.all'), value: 'all' },
      { label: t('servers.filter.enabled'), value: 'enabled' },
      { label: t('servers.filter.disabled'), value: 'disabled' },
    ],
    [t],
  );
  const getAuthTypeLabel = (value: string) => (
    value in authTypeLabelKeys
      ? t(authTypeLabelKeys[value as RemoteServerAuthType])
      : value
  );

  const serversQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const selectedServerQuery = useQuery({
    queryKey: ['remote-server', routeServerId],
    queryFn: () => api.getRemoteServer(routeServerId!),
    enabled: Boolean(routeServerId),
  });

  const createServerMutation = useMutation({
    mutationFn: api.createRemoteServer,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['mcp-catalog'] }),
      ]);
      messageApi.success(t('servers.created'));
      closeEditor();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const updateServerMutation = useMutation({
    mutationFn: ({
      id,
      payload,
    }: {
      id: string;
      payload: UpdateRemoteServerPayload;
    }) => api.updateRemoteServer(id, payload),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['remote-server'] }),
        queryClient.invalidateQueries({ queryKey: ['mcp-catalog'] }),
      ]);
      messageApi.success(t('servers.updated'));
      closeEditor();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const deleteServerMutation = useMutation({
    mutationFn: api.deleteRemoteServer,
    onSuccess: async (_, id) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['remote-server'] }),
        queryClient.invalidateQueries({ queryKey: ['mcp-catalog'] }),
      ]);
      if (routeServerId === id) {
        closeDetailDrawer();
      }
      messageApi.success(t('servers.deleted'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testDraftMutation = useMutation({
    mutationFn: api.testRemoteServerDraft,
    onSuccess: async (result) => {
      setTestResult(result);
      await queryClient.invalidateQueries({ queryKey: ['remote-servers'] });
      if (result.ok) {
        messageApi.success(t('servers.draftTestSuccess'));
      } else {
        messageApi.warning(t('servers.draftTestFailed'));
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testSavedMutation = useMutation({
    mutationFn: (id: string) => api.testRemoteServer(id),
    onMutate: (id: string) => {
      setTestingServerId(id);
    },
    onSuccess: async (result) => {
      setTestResult(result);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['remote-servers'] }),
        queryClient.invalidateQueries({ queryKey: ['remote-server'] }),
      ]);
      if (result.ok) {
        messageApi.success(t('servers.testSuccess'));
      } else {
        messageApi.warning(t('servers.testFailed'));
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
    onSettled: () => {
      setTestingServerId(null);
    },
  });

  const selectedServer = useMemo(() => {
    if (!routeServerId) {
      return null;
    }
    return (
      selectedServerQuery.data ||
      (serversQuery.data || []).find((server) => server.id === routeServerId) ||
      null
    );
  }, [routeServerId, selectedServerQuery.data, serversQuery.data]);

  const filteredServers = useMemo(() => {
    const keyword = keywordFilter.trim().toLowerCase();
    return (serversQuery.data || []).filter((server) => {
      if (authTypeFilter !== 'all' && server.auth_type !== authTypeFilter) {
        return false;
      }
      if (enabledFilter === 'enabled' && !server.enabled) {
        return false;
      }
      if (enabledFilter === 'disabled' && server.enabled) {
        return false;
      }
      if (!keyword) {
        return true;
      }
      return [
        server.name,
        server.host,
        server.username,
        server.default_remote_path || '',
        server.last_test_message || '',
      ]
        .join(' ')
        .toLowerCase()
        .includes(keyword);
    });
  }, [authTypeFilter, enabledFilter, keywordFilter, serversQuery.data]);

  const columns: ColumnsType<RemoteServerRecord> = [
    {
      title: t('servers.column.server'),
      dataIndex: 'name',
      width: 260,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Button type="link" style={{ padding: 0 }} onClick={() => openDetailDrawer(record.id)}>
            <Typography.Text strong>{record.name}</Typography.Text>
          </Button>
          <Typography.Text type="secondary">
            {record.username}@{record.host}:{record.port}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: t('servers.column.authType'),
      dataIndex: 'auth_type',
      width: 140,
      render: (value: string) => getAuthTypeLabel(value),
    },
    {
      title: t('servers.column.defaultDir'),
      dataIndex: 'default_remote_path',
      width: 220,
      render: (value?: string | null) => value || '-',
      ellipsis: true,
    },
    {
      title: t('servers.column.source'),
      key: 'source',
      width: 190,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text>{serverCreatorLabel(record)}</Typography.Text>
          {record.task_id ? (
            <Typography.Text type="secondary" code>
              {record.task_id.slice(0, 8)}
            </Typography.Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: t('servers.column.hostKeyPolicy'),
      dataIndex: 'host_key_policy',
      width: 120,
      render: (value: string) => (
        <Tag color={value === 'strict' ? 'blue' : 'default'}>{value}</Tag>
      ),
    },
    {
      title: t('servers.column.lastTest'),
      key: 'last_test',
      width: 240,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          {renderTestStatus(record.last_test_status, t)}
          <Typography.Text type="secondary">
            {record.last_tested_at
              ? dayjs(record.last_tested_at).format('YYYY-MM-DD HH:mm:ss')
              : t('servers.untested')}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: t('common.status'),
      dataIndex: 'enabled',
      width: 120,
      render: (value: boolean) => (
        <Tag color={value ? 'success' : 'default'}>
          {value ? t('common.enabled') : t('common.disabled')}
        </Tag>
      ),
    },
    {
      title: t('common.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 300,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => openDetailDrawer(record.id)}>
            {t('common.detail')}
          </Button>
          <Button size="small" onClick={() => openEditDrawer(record)}>
            {t('common.edit')}
          </Button>
          <Button
            size="small"
            loading={testSavedMutation.isPending && testingServerId === record.id}
            onClick={() => testSavedMutation.mutate(record.id)}
          >
            {t('common.test')}
          </Button>
          <Button size="small" danger onClick={() => confirmDelete(record)}>
            {t('common.delete')}
          </Button>
        </Space>
      ),
    },
  ];

  function openCreateDrawer() {
    setEditingServer(null);
    form.setFieldsValue({
      name: '',
      host: '',
      port: 22,
      username: '',
      auth_type: 'password',
      password: '',
      private_key_path: '',
      certificate_path: '',
      default_remote_path: '',
      host_key_policy: 'accept_new',
      enabled: true,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(server: RemoteServerRecord) {
    setEditingServer(server);
    form.setFieldsValue({
      name: server.name,
      host: server.host,
      port: server.port,
      username: server.username,
      auth_type: normalizeAuthType(server.auth_type),
      password: server.password || '',
      private_key_path: server.private_key_path || '',
      certificate_path: server.certificate_path || '',
      default_remote_path: server.default_remote_path || '',
      host_key_policy: normalizeHostKeyPolicy(server.host_key_policy),
      enabled: server.enabled,
    });
    setDrawerOpen(true);
  }

  function closeEditor() {
    setDrawerOpen(false);
    setEditingServer(null);
    form.resetFields();
  }

  function openDetailDrawer(serverId: string) {
    const next = new URLSearchParams(searchParams);
    next.set('server_id', serverId);
    setSearchParams(next);
  }

  function closeDetailDrawer() {
    const next = new URLSearchParams(searchParams);
    next.delete('server_id');
    setSearchParams(next);
  }

  function confirmDelete(server: RemoteServerRecord) {
    Modal.confirm({
      title: t('servers.deleteConfirmTitle', { name: server.name }),
      content: t('servers.deleteConfirmContent'),
      okButtonProps: { danger: true },
      onOk: () => deleteServerMutation.mutate(server.id),
    });
  }

  function handleSubmit(values: RemoteServerFormValues) {
    const payload = buildRemoteServerPayload(values);
    if (editingServer) {
      updateServerMutation.mutate({ id: editingServer.id, payload });
      return;
    }
    createServerMutation.mutate(payload);
  }

  async function handleDraftTest() {
    const values = await form.validateFields();
    const payload = buildRemoteServerTestPayload(values);
    testDraftMutation.mutate(payload);
  }

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }} align="start">
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              {t('servers.title')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('servers.subtitle')}
            </Typography.Text>
          </Space>
          <Space wrap>
            <Input
              allowClear
              placeholder={t('servers.searchPlaceholder')}
              style={{ width: 260 }}
              value={keywordFilter}
              onChange={(event) => setKeywordFilter(event.target.value)}
            />
            <Select
              style={{ width: 180 }}
              value={authTypeFilter}
              options={authTypeFilterOptions}
              onChange={(value) => setAuthTypeFilter(value as 'all' | RemoteServerAuthType)}
            />
            <Segmented
              value={enabledFilter}
              onChange={(value) =>
                setEnabledFilter(value as 'all' | 'enabled' | 'disabled')
              }
              options={enabledFilterOptions}
            />
            <Button
              onClick={() => {
                setKeywordFilter('');
                setAuthTypeFilter('all');
                setEnabledFilter('all');
              }}
            >
              {t('common.clearFilters')}
            </Button>
            <Button onClick={() => serversQuery.refetch()}>{t('common.refresh')}</Button>
            <Button type="primary" onClick={openCreateDrawer}>
              {t('servers.new')}
            </Button>
          </Space>
        </Space>

        <Space size="large" wrap>
          <Statistic title={t('servers.visible')} value={filteredServers.length} />
          <Statistic
            title={t('servers.enabledCount')}
            value={filteredServers.filter((server) => server.enabled).length}
          />
          <Statistic
            title={t('servers.testPassed')}
            value={
              filteredServers.filter((server) => server.last_test_status === 'success').length
            }
          />
          <Statistic
            title={t('servers.strictCheck')}
            value={filteredServers.filter((server) => server.host_key_policy === 'strict').length}
          />
        </Space>

        <Table<RemoteServerRecord>
          rowKey="id"
          columns={columns}
          dataSource={filteredServers}
          loading={serversQuery.isLoading}
          pagination={{ pageSize: 8 }}
          scroll={{ x: 1400 }}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('servers.empty')}
              />
            ),
          }}
        />
      </Space>

      <Drawer
        title={editingServer ? t('servers.drawer.edit') : t('servers.drawer.create')}
        open={drawerOpen}
        width={560}
        destroyOnClose
        onClose={closeEditor}
        extra={
          <Space>
            <Button onClick={closeEditor}>{t('common.cancel')}</Button>
            <Button loading={testDraftMutation.isPending} onClick={handleDraftTest}>
              {t('servers.testDraft')}
            </Button>
            <Button
              type="primary"
              loading={createServerMutation.isPending || updateServerMutation.isPending}
              onClick={() => form.submit()}
            >
              {t('common.save')}
            </Button>
          </Space>
        }
      >
        <Form<RemoteServerFormValues> layout="vertical" form={form} onFinish={handleSubmit}>
          <Form.Item
            name="name"
            label={t('servers.form.name')}
            rules={[{ required: true, message: t('servers.form.nameRequired') }]}
          >
            <Input />
          </Form.Item>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item
              name="host"
              label="Host"
              style={{ flex: 1 }}
              rules={[{ required: true, message: t('servers.form.hostRequired') }]}
            >
              <Input placeholder={t('servers.form.hostPlaceholder')} />
            </Form.Item>
            <Form.Item name="port" label="Port" style={{ width: 140 }}>
              <InputNumber min={1} max={65535} style={{ width: '100%' }} />
            </Form.Item>
          </Space>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item
              name="username"
              label="Username"
              style={{ flex: 1 }}
              rules={[{ required: true, message: t('servers.form.usernameRequired') }]}
            >
              <Input />
            </Form.Item>
            <Form.Item name="auth_type" label={t('servers.form.authType')} style={{ width: 220 }} rules={[{ required: true }]}>
              <Select options={authTypeOptions} />
            </Form.Item>
          </Space>

          {authType === 'password' ? (
            <Form.Item name="password" label="Password" rules={[{ required: true, message: t('servers.form.passwordRequired') }]}>
              <Input.Password />
            </Form.Item>
          ) : null}

          {authType === 'private_key' || authType === 'private_key_cert' ? (
            <Form.Item
              name="private_key_path"
              label="Private Key Path"
              rules={[{ required: true, message: t('servers.form.privateKeyRequired') }]}
            >
              <Input placeholder="~/.ssh/id_rsa" />
            </Form.Item>
          ) : null}

          {authType === 'private_key_cert' ? (
            <Form.Item
              name="certificate_path"
              label="Certificate Path"
              rules={[{ required: true, message: t('servers.form.certificateRequired') }]}
            >
              <Input placeholder="~/.ssh/id_rsa-cert.pub" />
            </Form.Item>
          ) : null}

          <Form.Item name="default_remote_path" label={t('servers.form.defaultRemotePath')}>
            <Input placeholder="/srv/app" />
          </Form.Item>

          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="host_key_policy" label="Host Key Policy" style={{ flex: 1 }} rules={[{ required: true }]}>
              <Select
                options={
                  HOST_KEY_POLICY_OPTIONS as unknown as { label: string; value: string }[]
                }
              />
            </Form.Item>
            <Form.Item
              name="enabled"
              label="Enabled"
              valuePropName="checked"
              style={{ marginBottom: 0 }}
            >
              <Switch />
            </Form.Item>
          </Space>
        </Form>
      </Drawer>

      <Drawer
        title={selectedServer
          ? t('servers.detail.titleWithName', { name: selectedServer.name })
          : t('servers.detail.title')}
        open={Boolean(routeServerId)}
        width={760}
        onClose={closeDetailDrawer}
      >
        {selectedServer ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Button
                loading={testSavedMutation.isPending && testingServerId === selectedServer.id}
                onClick={() => testSavedMutation.mutate(selectedServer.id)}
              >
                {t('servers.detail.testConnection')}
              </Button>
              <Button
                onClick={() => {
                  closeDetailDrawer();
                  openEditDrawer(selectedServer);
                }}
              >
                {t('servers.detail.editConfig')}
              </Button>
              <Button danger onClick={() => confirmDelete(selectedServer)}>
                {t('servers.detail.deleteServer')}
              </Button>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('servers.detail.serverId')}>{selectedServer.id}</Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.name')}>{selectedServer.name}</Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.creator')}>
                {serverCreatorLabel(selectedServer)}
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.taskId')}>
                {selectedServer.task_id || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Host">
                {selectedServer.host}:{selectedServer.port}
              </Descriptions.Item>
              <Descriptions.Item label="Username">{selectedServer.username}</Descriptions.Item>
              <Descriptions.Item label={t('servers.column.authType')}>
                {getAuthTypeLabel(selectedServer.auth_type)}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <Tag color={selectedServer.enabled ? 'success' : 'default'}>
                  {selectedServer.enabled ? t('common.enabled') : t('common.disabled')}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Host Key Policy">
                <Tag color={selectedServer.host_key_policy === 'strict' ? 'blue' : 'default'}>
                  {selectedServer.host_key_policy}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.form.defaultRemotePath')}>
                {selectedServer.default_remote_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Password">
                {selectedServer.password ? t('servers.detail.passwordSaved') : '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Private Key Path">
                {selectedServer.private_key_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Certificate Path">
                {selectedServer.certificate_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.lastTestStatus')}>
                {renderTestStatus(selectedServer.last_test_status, t)}
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.lastTestedAt')}>
                {selectedServer.last_tested_at
                  ? dayjs(selectedServer.last_tested_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.lastTestMessage')}>
                {selectedServer.last_test_message || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.lastActiveAt')}>
                {selectedServer.last_active_at
                  ? dayjs(selectedServer.last_active_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('servers.detail.createdAt')}>
                {dayjs(selectedServer.created_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.updatedAt')}>
                {dayjs(selectedServer.updated_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
            </Descriptions>

            <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>
              {t('servers.detail.hint')}
            </Typography.Paragraph>
          </Space>
        ) : selectedServerQuery.isLoading ? null : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
        )}
      </Drawer>

      <Modal
        title={t('servers.testResult.title')}
        open={Boolean(testResult)}
        width={680}
        footer={[
          <Button key="close" onClick={() => setTestResult(null)}>
            {t('common.close')}
          </Button>,
        ]}
        onCancel={() => setTestResult(null)}
      >
        {testResult ? (
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('servers.testResult.result')}>
              <Tag color={testResult.ok ? 'success' : 'error'}>
                {testResult.ok ? t('common.success') : t('common.failed')}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.column.server')}>
              {testResult.name} ({testResult.username}@{testResult.host}:{testResult.port})
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.column.authType')}>
              {getAuthTypeLabel(testResult.auth_type)}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.testResult.remoteHost')}>
              {testResult.remote_host || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.testResult.error')}>
              {testResult.error || '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('servers.testResult.testedAt')}>
              {dayjs(testResult.tested_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
          </Descriptions>
        ) : null}
      </Modal>
    </>
  );
}

function buildRemoteServerPayload(
  values: RemoteServerFormValues,
): CreateRemoteServerPayload {
  const base = {
    name: values.name,
    host: values.host,
    port: values.port,
    username: values.username,
    auth_type: values.auth_type,
    default_remote_path: values.default_remote_path || '',
    host_key_policy: values.host_key_policy,
    enabled: values.enabled,
  };

  if (values.auth_type === 'password') {
    return {
      ...base,
      password: values.password || '',
      private_key_path: '',
      certificate_path: '',
    };
  }

  if (values.auth_type === 'private_key') {
    return {
      ...base,
      password: '',
      private_key_path: values.private_key_path || '',
      certificate_path: '',
    };
  }

  return {
    ...base,
    password: '',
    private_key_path: values.private_key_path || '',
    certificate_path: values.certificate_path || '',
  };
}

function buildRemoteServerTestPayload(values: RemoteServerFormValues): TestRemoteServerPayload {
  const payload = buildRemoteServerPayload(values);
  return {
    ...payload,
  };
}

function normalizeAuthType(value: string): RemoteServerAuthType {
  if (value === 'private_key' || value === 'private_key_cert') {
    return value;
  }
  return 'password';
}

function normalizeHostKeyPolicy(value: string): 'accept_new' | 'strict' {
  return value === 'strict' ? 'strict' : 'accept_new';
}

function renderTestStatus(value: string | null | undefined, t: TranslateFn) {
  if (value === 'success') {
    return <Tag color="success">{t('common.success')}</Tag>;
  }
  if (value === 'failed') {
    return <Tag color="error">{t('common.failed')}</Tag>;
  }
  return <Tag>{t('servers.untested')}</Tag>;
}

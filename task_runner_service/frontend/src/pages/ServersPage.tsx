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

const AUTH_TYPE_OPTIONS = [
  { label: '密码', value: 'password' },
  { label: '私钥', value: 'private_key' },
  { label: '私钥 + 证书', value: 'private_key_cert' },
] as const;

const HOST_KEY_POLICY_OPTIONS = [
  { label: 'accept_new', value: 'accept_new' },
  { label: 'strict', value: 'strict' },
] as const;

export function ServersPage() {
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
      messageApi.success('服务器已创建');
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
      messageApi.success('服务器已更新');
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
      messageApi.success('服务器已删除');
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testDraftMutation = useMutation({
    mutationFn: api.testRemoteServerDraft,
    onSuccess: async (result) => {
      setTestResult(result);
      await queryClient.invalidateQueries({ queryKey: ['remote-servers'] });
      if (result.ok) {
        messageApi.success('草稿连通性测试成功');
      } else {
        messageApi.warning('草稿连通性测试失败');
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
        messageApi.success('服务器连通性测试成功');
      } else {
        messageApi.warning('服务器连通性测试失败');
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
      title: '服务器',
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
      title: '认证方式',
      dataIndex: 'auth_type',
      width: 140,
      render: (value: string) => authTypeLabel(value),
    },
    {
      title: '默认目录',
      dataIndex: 'default_remote_path',
      width: 220,
      render: (value?: string | null) => value || '-',
      ellipsis: true,
    },
    {
      title: '主机校验',
      dataIndex: 'host_key_policy',
      width: 120,
      render: (value: string) => (
        <Tag color={value === 'strict' ? 'blue' : 'default'}>{value}</Tag>
      ),
    },
    {
      title: '最近测试',
      key: 'last_test',
      width: 240,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          {renderTestStatus(record.last_test_status)}
          <Typography.Text type="secondary">
            {record.last_tested_at
              ? dayjs(record.last_tested_at).format('YYYY-MM-DD HH:mm:ss')
              : '未测试'}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      width: 120,
      render: (value: boolean) => (
        <Tag color={value ? 'success' : 'default'}>{value ? 'enabled' : 'disabled'}</Tag>
      ),
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      key: 'actions',
      width: 300,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => openDetailDrawer(record.id)}>
            详情
          </Button>
          <Button size="small" onClick={() => openEditDrawer(record)}>
            编辑
          </Button>
          <Button
            size="small"
            loading={testSavedMutation.isPending && testingServerId === record.id}
            onClick={() => testSavedMutation.mutate(record.id)}
          >
            测试
          </Button>
          <Button size="small" danger onClick={() => confirmDelete(record)}>
            删除
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
      title: `删除服务器: ${server.name}`,
      content: '删除后，共享的 RemoteConnectionController 将不再能访问这台服务器。',
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
              服务器
            </Typography.Title>
            <Typography.Text type="secondary">
              维护 Task Runner 可复用的远程服务器清单，供共享 RemoteConnectionController builtin MCP 调用。
            </Typography.Text>
          </Space>
          <Space wrap>
            <Input
              allowClear
              placeholder="搜索名称 / Host / 用户 / 路径"
              style={{ width: 260 }}
              value={keywordFilter}
              onChange={(event) => setKeywordFilter(event.target.value)}
            />
            <Select
              style={{ width: 180 }}
              value={authTypeFilter}
              options={[
                { label: '全部认证方式', value: 'all' },
                ...AUTH_TYPE_OPTIONS,
              ]}
              onChange={(value) => setAuthTypeFilter(value as 'all' | RemoteServerAuthType)}
            />
            <Segmented
              value={enabledFilter}
              onChange={(value) =>
                setEnabledFilter(value as 'all' | 'enabled' | 'disabled')
              }
              options={[
                { label: '全部', value: 'all' },
                { label: '启用中', value: 'enabled' },
                { label: '已停用', value: 'disabled' },
              ]}
            />
            <Button
              onClick={() => {
                setKeywordFilter('');
                setAuthTypeFilter('all');
                setEnabledFilter('all');
              }}
            >
              清空筛选
            </Button>
            <Button onClick={() => serversQuery.refetch()}>刷新</Button>
            <Button type="primary" onClick={openCreateDrawer}>
              新建服务器
            </Button>
          </Space>
        </Space>

        <Space size="large" wrap>
          <Statistic title="当前可见服务器" value={filteredServers.length} />
          <Statistic
            title="启用中"
            value={filteredServers.filter((server) => server.enabled).length}
          />
          <Statistic
            title="测试成功"
            value={
              filteredServers.filter((server) => server.last_test_status === 'success').length
            }
          />
          <Statistic
            title="严格校验"
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
                description="暂无远程服务器，请先创建服务器配置"
              />
            ),
          }}
        />
      </Space>

      <Drawer
        title={editingServer ? '编辑服务器' : '新建服务器'}
        open={drawerOpen}
        width={560}
        destroyOnClose
        onClose={closeEditor}
        extra={
          <Space>
            <Button onClick={closeEditor}>取消</Button>
            <Button loading={testDraftMutation.isPending} onClick={handleDraftTest}>
              测试草稿
            </Button>
            <Button
              type="primary"
              loading={createServerMutation.isPending || updateServerMutation.isPending}
              onClick={() => form.submit()}
            >
              保存
            </Button>
          </Space>
        }
      >
        <Form<RemoteServerFormValues> layout="vertical" form={form} onFinish={handleSubmit}>
          <Form.Item name="name" label="服务器名称" rules={[{ required: true, message: '请输入服务器名称' }]}>
            <Input />
          </Form.Item>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="host" label="Host" style={{ flex: 1 }} rules={[{ required: true, message: '请输入主机地址' }]}>
              <Input placeholder="127.0.0.1 或 server.example.com" />
            </Form.Item>
            <Form.Item name="port" label="Port" style={{ width: 140 }}>
              <InputNumber min={1} max={65535} style={{ width: '100%' }} />
            </Form.Item>
          </Space>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="username" label="Username" style={{ flex: 1 }} rules={[{ required: true, message: '请输入用户名' }]}>
              <Input />
            </Form.Item>
            <Form.Item name="auth_type" label="认证方式" style={{ width: 220 }} rules={[{ required: true }]}>
              <Select options={AUTH_TYPE_OPTIONS as unknown as { label: string; value: string }[]} />
            </Form.Item>
          </Space>

          {authType === 'password' ? (
            <Form.Item name="password" label="Password" rules={[{ required: true, message: '请输入密码' }]}>
              <Input.Password />
            </Form.Item>
          ) : null}

          {authType === 'private_key' || authType === 'private_key_cert' ? (
            <Form.Item
              name="private_key_path"
              label="Private Key Path"
              rules={[{ required: true, message: '请输入私钥路径' }]}
            >
              <Input placeholder="~/.ssh/id_rsa" />
            </Form.Item>
          ) : null}

          {authType === 'private_key_cert' ? (
            <Form.Item
              name="certificate_path"
              label="Certificate Path"
              rules={[{ required: true, message: '请输入证书路径' }]}
            >
              <Input placeholder="~/.ssh/id_rsa-cert.pub" />
            </Form.Item>
          ) : null}

          <Form.Item name="default_remote_path" label="默认远程目录">
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
        title={selectedServer ? `服务器详情 - ${selectedServer.name}` : '服务器详情'}
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
                测试连通性
              </Button>
              <Button
                onClick={() => {
                  closeDetailDrawer();
                  openEditDrawer(selectedServer);
                }}
              >
                编辑配置
              </Button>
              <Button danger onClick={() => confirmDelete(selectedServer)}>
                删除服务器
              </Button>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="服务器 ID">{selectedServer.id}</Descriptions.Item>
              <Descriptions.Item label="名称">{selectedServer.name}</Descriptions.Item>
              <Descriptions.Item label="Host">
                {selectedServer.host}:{selectedServer.port}
              </Descriptions.Item>
              <Descriptions.Item label="Username">{selectedServer.username}</Descriptions.Item>
              <Descriptions.Item label="认证方式">
                {authTypeLabel(selectedServer.auth_type)}
              </Descriptions.Item>
              <Descriptions.Item label="状态">
                <Tag color={selectedServer.enabled ? 'success' : 'default'}>
                  {selectedServer.enabled ? 'enabled' : 'disabled'}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Host Key Policy">
                <Tag color={selectedServer.host_key_policy === 'strict' ? 'blue' : 'default'}>
                  {selectedServer.host_key_policy}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="默认远程目录">
                {selectedServer.default_remote_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Password">
                {selectedServer.password ? '已保存' : '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Private Key Path">
                {selectedServer.private_key_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Certificate Path">
                {selectedServer.certificate_path || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="最近测试状态">
                {renderTestStatus(selectedServer.last_test_status)}
              </Descriptions.Item>
              <Descriptions.Item label="最近测试时间">
                {selectedServer.last_tested_at
                  ? dayjs(selectedServer.last_tested_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
              <Descriptions.Item label="最近测试信息">
                {selectedServer.last_test_message || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="最近使用时间">
                {selectedServer.last_active_at
                  ? dayjs(selectedServer.last_active_at).format('YYYY-MM-DD HH:mm:ss')
                  : '-'}
              </Descriptions.Item>
              <Descriptions.Item label="创建时间">
                {dayjs(selectedServer.created_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label="更新时间">
                {dayjs(selectedServer.updated_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
            </Descriptions>

            <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>
              保存后，Task Runner 侧共享的 RemoteConnectionController builtin MCP 会直接复用这里的服务器列表。
            </Typography.Paragraph>
          </Space>
        ) : selectedServerQuery.isLoading ? null : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
        )}
      </Drawer>

      <Modal
        title="服务器测试结果"
        open={Boolean(testResult)}
        width={680}
        footer={[
          <Button key="close" onClick={() => setTestResult(null)}>
            关闭
          </Button>,
        ]}
        onCancel={() => setTestResult(null)}
      >
        {testResult ? (
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label="结果">
              <Tag color={testResult.ok ? 'success' : 'error'}>
                {testResult.ok ? 'success' : 'failed'}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label="服务器">
              {testResult.name} ({testResult.username}@{testResult.host}:{testResult.port})
            </Descriptions.Item>
            <Descriptions.Item label="认证方式">
              {authTypeLabel(testResult.auth_type)}
            </Descriptions.Item>
            <Descriptions.Item label="远端主机名">
              {testResult.remote_host || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="错误信息">
              {testResult.error || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="测试时间">
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

function authTypeLabel(value: string): string {
  return AUTH_TYPE_OPTIONS.find((item) => item.value === value)?.label || value;
}

function renderTestStatus(value?: string | null) {
  if (value === 'success') {
    return <Tag color="success">success</Tag>;
  }
  if (value === 'failed') {
    return <Tag color="error">failed</Tag>;
  }
  return <Tag>untested</Tag>;
}

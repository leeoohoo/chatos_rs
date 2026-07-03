// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  CopyOutlined,
  DeleteOutlined,
  EditOutlined,
  KeyOutlined,
  PlusOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  App,
  Button,
  Collapse,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';
import { useState } from 'react';

import { accessClientsApi } from '../api/accessClients';
import { useI18n } from '../i18n';
import type {
  SandboxAccessClient,
  SandboxAccessClientPayload,
  SandboxAccessClientSecretResponse,
  SandboxAccessClientUpdatePayload,
} from '../types';

const scopeOptions = [
  'sandbox.admin',
  'sandbox.pool.read',
  'sandbox.images.read',
  'sandbox.images.write',
  'sandbox.lease.create',
  'sandbox.lease.read',
  'sandbox.lease.release',
  'sandbox.lease.destroy',
  'sandbox.mcp.tools',
  'sandbox.mcp.call',
];

interface AccessClientFormValues {
  name: string;
  client_id?: string;
  scopes: string[];
  allowed_tenant_ids?: string;
  allowed_project_ids?: string;
  allowed_tools?: string;
  max_lease_ttl_seconds?: number;
}

export function AccessClientsPage() {
  const { modal, message } = App.useApp();
  const queryClient = useQueryClient();
  const { t } = useI18n();
  const [form] = Form.useForm<AccessClientFormValues>();
  const [editingClient, setEditingClient] = useState<SandboxAccessClient | null>(null);
  const [modalOpen, setModalOpen] = useState(false);

  const clientsQuery = useQuery({
    queryKey: ['sandbox-access-clients'],
    queryFn: accessClientsApi.list,
  });

  const invalidateClients = () => queryClient.invalidateQueries({ queryKey: ['sandbox-access-clients'] });

  const createMutation = useMutation({
    mutationFn: accessClientsApi.create,
    onSuccess: (response) => {
      setModalOpen(false);
      void invalidateClients();
      showClientKey(modal, t, response);
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: SandboxAccessClientUpdatePayload }) =>
      accessClientsApi.update(id, payload),
    onSuccess: () => {
      setModalOpen(false);
      void invalidateClients();
      message.success(t('access.updated'));
    },
  });

  const toggleMutation = useMutation({
    mutationFn: ({ client, enabled }: { client: SandboxAccessClient; enabled: boolean }) =>
      accessClientsApi.update(client.id, { enabled }),
    onSuccess: () => {
      void invalidateClients();
    },
  });

  const rotateMutation = useMutation({
    mutationFn: accessClientsApi.rotateKey,
    onSuccess: (response) => {
      void invalidateClients();
      showClientKey(modal, t, response);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: accessClientsApi.remove,
    onSuccess: () => {
      void invalidateClients();
      message.success(t('access.deleted'));
    },
  });

  const openCreate = () => {
    setEditingClient(null);
    form.setFieldsValue({
      name: '',
      client_id: '',
      scopes: ['sandbox.lease.create', 'sandbox.lease.read', 'sandbox.lease.release', 'sandbox.mcp.tools', 'sandbox.mcp.call'],
      allowed_tenant_ids: '*',
      allowed_project_ids: '*',
      allowed_tools: '*',
      max_lease_ttl_seconds: 7200,
    });
    setModalOpen(true);
  };

  const openEdit = (client: SandboxAccessClient) => {
    setEditingClient(client);
    form.setFieldsValue({
      name: client.name,
      client_id: client.client_id,
      scopes: client.scopes,
      allowed_tenant_ids: client.allowed_tenant_ids.join(', '),
      allowed_project_ids: client.allowed_project_ids.join(', '),
      allowed_tools: client.allowed_tools.join(', '),
      max_lease_ttl_seconds: client.max_lease_ttl_seconds,
    });
    setModalOpen(true);
  };

  const submit = async () => {
    const values = await form.validateFields();
    const basePayload = {
      name: values.name,
      scopes: values.scopes,
      allowed_tenant_ids: parseCsv(values.allowed_tenant_ids),
      allowed_project_ids: parseCsv(values.allowed_project_ids),
      allowed_tools: parseCsv(values.allowed_tools),
      max_lease_ttl_seconds: values.max_lease_ttl_seconds,
    };
    if (editingClient) {
      updateMutation.mutate({ id: editingClient.id, payload: basePayload });
    } else {
      createMutation.mutate({ ...basePayload, client_id: values.client_id });
    }
  };

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('access.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('access.subtitle')}</Typography.Text>
        </div>
        <Space>
          <Button icon={<ReloadOutlined />} onClick={() => void clientsQuery.refetch()}>
            {t('common.refresh')}
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            {t('access.create')}
          </Button>
        </Space>
      </div>

      <div className="surface">
        <Table<SandboxAccessClient>
          rowKey="id"
          loading={clientsQuery.isLoading}
          dataSource={clientsQuery.data ?? []}
          pagination={{ pageSize: 10 }}
          scroll={{ x: 1220 }}
          columns={[
            {
              title: t('access.enabled'),
              dataIndex: 'enabled',
              width: 110,
              render: (enabled, record) => (
                <Switch
                  checked={enabled}
                  loading={toggleMutation.isPending}
                  onChange={(checked) => toggleMutation.mutate({ client: record, enabled: checked })}
                />
              ),
            },
            { title: t('access.name'), dataIndex: 'name', width: 180 },
            {
              title: t('access.clientId'),
              dataIndex: 'client_id',
              width: 240,
              render: (value) => <Typography.Text copyable>{value}</Typography.Text>,
            },
            {
              title: t('access.scopes'),
              dataIndex: 'scopes',
              width: 260,
              render: (scopes: string[]) => scopes.map((scope) => <Tag key={scope}>{scope}</Tag>),
            },
            {
              title: t('access.tenantScope'),
              dataIndex: 'allowed_tenant_ids',
              width: 180,
              render: renderListTags,
            },
            {
              title: t('access.projectScope'),
              dataIndex: 'allowed_project_ids',
              width: 180,
              render: renderListTags,
            },
            {
              title: t('access.lastUsedAt'),
              dataIndex: 'last_used_at',
              width: 160,
              render: (value) => value ? dayjs(value).format('MM-DD HH:mm:ss') : '-',
            },
            {
              title: t('common.actions'),
              fixed: 'right',
              width: 260,
              render: (_, record) => (
                <Space>
                  <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(record)}>
                    {t('access.edit')}
                  </Button>
                  <Button
                    size="small"
                    icon={<KeyOutlined />}
                    loading={rotateMutation.isPending}
                    onClick={() => rotateMutation.mutate(record.id)}
                  >
                    {t('access.rotate')}
                  </Button>
                  <Popconfirm
                    title={t('access.deleteTitle')}
                    description={t('access.deleteDescription')}
                    onConfirm={() => deleteMutation.mutate(record.id)}
                  >
                    <Button danger size="small" icon={<DeleteOutlined />} loading={deleteMutation.isPending}>
                      {t('access.delete')}
                    </Button>
                  </Popconfirm>
                </Space>
              ),
            },
          ]}
        />
      </div>

      <Modal
        title={editingClient ? t('access.editTitle') : t('access.createTitle')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={() => void submit()}
        confirmLoading={createMutation.isPending || updateMutation.isPending}
        width={720}
        destroyOnClose
      >
        <Form form={form} layout="vertical">
          <Form.Item name="name" label={t('access.name')} rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="client_id" label={t('access.clientId')}>
            <Input disabled={Boolean(editingClient)} placeholder="sandbox_client_xxx" />
          </Form.Item>
          <Form.Item name="scopes" label={t('access.scopes')} rules={[{ required: true }]}>
            <Select mode="tags" options={scopeOptions.map((scope) => ({ label: scope, value: scope }))} />
          </Form.Item>
          <Collapse
            ghost
            size="small"
            items={[
              {
                key: 'policy',
                label: t('access.advancedPolicy'),
                children: (
                  <Space direction="vertical" size={0} style={{ width: '100%' }}>
                    <Typography.Text type="secondary">{t('access.advancedPolicyHint')}</Typography.Text>
                    <Form.Item name="allowed_tenant_ids" label={t('access.tenantScope')}>
                      <Input placeholder="*, tenant-1, tenant-2" />
                    </Form.Item>
                    <Form.Item name="allowed_project_ids" label={t('access.projectScope')}>
                      <Input placeholder="*, project-1, project-2" />
                    </Form.Item>
                    <Form.Item name="allowed_tools" label={t('access.toolScope')}>
                      <Input placeholder="*, filesystem, terminal" />
                    </Form.Item>
                  </Space>
                ),
              },
            ]}
          />
          <Form.Item name="max_lease_ttl_seconds" label={t('access.maxTtl')}>
            <InputNumber min={60} style={{ width: '100%' }} />
          </Form.Item>
        </Form>
      </Modal>
    </Space>
  );
}

function parseCsv(value?: string): string[] {
  const items = (value || '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
  return items.length > 0 ? items : ['*'];
}

function renderListTags(values: string[]) {
  return (values || []).map((value) => <Tag key={value}>{value}</Tag>);
}

function showClientKey(
  modal: ReturnType<typeof App.useApp>['modal'],
  t: (key: string) => string,
  response: SandboxAccessClientSecretResponse,
) {
  modal.success({
    title: t('access.keyTitle'),
    width: 720,
    content: (
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        <Typography.Text>{t('access.keyHint')}</Typography.Text>
        <Typography.Text strong>{response.client.client_id}</Typography.Text>
        <Typography.Paragraph copyable={{ icon: <CopyOutlined /> }} code>
          {response.client_key}
        </Typography.Paragraph>
      </Space>
    ),
  });
}

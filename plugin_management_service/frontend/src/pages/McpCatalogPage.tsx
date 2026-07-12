// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Button,
  Form,
  Input,
  Modal,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag, RuntimeKindTag, VisibilityTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import { mcpDisplayName, runtimeKindLabel } from '../i18n/labels';
import type { CurrentUser, McpRecord, RuntimeKind } from '../types';
import { jsonText, optionalText, parseJsonArray, parseJsonObject } from './formUtils';

interface McpCatalogPageProps {
  user: CurrentUser;
}

const adminRuntimeKinds: RuntimeKind[] = [
  'http',
  'stdio_cloud',
  'local_connector_stdio',
  'local_connector_http',
  'local_connector_builtin_proxy',
];
const userRuntimeKinds: RuntimeKind[] = ['local_connector_stdio', 'local_connector_http'];

export function McpCatalogPage({ user }: McpCatalogPageProps) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<McpRecord | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const isAdmin = user.role === 'super_admin';
  const runtimeKinds = isAdmin ? adminRuntimeKinds : userRuntimeKinds;
  const runtimeKind = Form.useWatch('runtime_kind', form) as RuntimeKind | undefined;
  const editingSystemManaged = editing ? isSystemManagedMcp(editing) : false;

  const mcpsQuery = useQuery({
    queryKey: ['mcps', isAdmin],
    queryFn: () => api.listMcps({ include_system: isAdmin, limit: 500 }),
  });

  const saveMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) => {
      const payload = editingSystemManaged
        ? { enabled: Boolean(values.enabled) }
        : buildMcpPayload(values, isAdmin);
      return editing ? api.updateMcp(editing.id, payload) : api.createMcp(payload);
    },
    onSuccess: () => {
      message.success(t('mcp.saved'));
      setModalOpen(false);
      setEditing(null);
      queryClient.invalidateQueries({ queryKey: ['mcps'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteMcp,
    onSuccess: () => {
      message.success(t('mcp.deleted'));
      queryClient.invalidateQueries({ queryKey: ['mcps'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const checkMutation = useMutation({
    mutationFn: api.checkMcp,
    onSuccess: (record) => {
      message.success(t('mcp.checkDone', { status: t(`status.${record.status}`) }));
      queryClient.invalidateQueries({ queryKey: ['mcps'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const columns = useMemo<ColumnsType<McpRecord>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'display_name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{mcpDisplayName(record, t)}</Typography.Text>
            <Typography.Text type="secondary">{record.name}</Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.visibility'),
        dataIndex: 'visibility',
        width: 120,
        render: (value) => <VisibilityTag value={value} />,
      },
      {
        title: t('table.runtime'),
        dataIndex: ['runtime', 'kind'],
        width: 210,
        render: (value, record) => (
          <Space direction="vertical" size={0}>
            <RuntimeKindTag value={value} />
            <Typography.Text type="secondary" className="table-secondary-nowrap" ellipsis>
              {record.runtime.builtin_kind || record.runtime.server_name || record.runtime.url || record.runtime.command}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('table.owner'),
        dataIndex: 'owner_user_id',
        width: 180,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('table.status'),
        dataIndex: 'enabled',
        width: 100,
        render: (enabled) => <EnabledTag enabled={enabled} />,
      },
      {
        title: t('table.updated'),
        dataIndex: 'updated_at',
        width: 170,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 230,
        render: (_, record) => (
          <Space>
            <Button
              icon={<ReloadOutlined />}
              size="small"
              loading={checkMutation.isPending}
              onClick={() => checkMutation.mutate(record.id)}
            >
              {t('common.check')}
            </Button>
            <Button icon={<EditOutlined />} size="small" onClick={() => openEdit(record)}>
              {t(isSystemManagedMcp(record) ? 'common.configure' : 'common.edit')}
            </Button>
            {!isSystemManagedMcp(record) ? (
              <Popconfirm
                title={t('mcp.deleteConfirm')}
                onConfirm={() => deleteMutation.mutate(record.id)}
              >
                <Button danger icon={<DeleteOutlined />} size="small" />
              </Popconfirm>
            ) : null}
          </Space>
        ),
      },
    ],
    [checkMutation, deleteMutation, t],
  );

  function openCreate() {
    setEditing(null);
    form.resetFields();
    form.setFieldsValue({
      visibility: 'private',
      enabled: true,
      runtime_kind: isAdmin ? 'http' : 'local_connector_stdio',
      args_json: '[]',
      env_json: '{}',
      headers_json: '{}',
      local_connector_json: '',
    });
    setModalOpen(true);
  }

  function openEdit(record: McpRecord) {
    setEditing(record);
    form.resetFields();
    if (isSystemManagedMcp(record)) {
      form.setFieldsValue({ enabled: record.enabled });
      setModalOpen(true);
      return;
    }
    form.setFieldsValue({
      ...record,
      runtime_kind: record.runtime.kind,
      command: record.runtime.command,
      cwd: record.runtime.cwd,
      url: record.runtime.url,
      args_json: jsonText(record.runtime.args || []),
      env_json: jsonText(record.runtime.env || {}),
      headers_json: jsonText(record.runtime.headers || {}),
      local_connector_json: jsonText(record.runtime.local_connector),
    });
    setModalOpen(true);
  }

  function closeModal() {
    setModalOpen(false);
    setEditing(null);
    form.resetFields();
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('mcp.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('mcp.description')}</Typography.Text>
        </Space>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
          {t('mcp.add')}
        </Button>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={mcpsQuery.data?.items || []}
        loading={mcpsQuery.isLoading}
        tableLayout="fixed"
        scroll={{ x: 1180 }}
        pagination={{ pageSize: 12 }}
      />
      <Modal
        title={t(
          editingSystemManaged
            ? 'mcp.systemConfigTitle'
            : editing
              ? 'mcp.editTitle'
              : 'mcp.addTitle',
        )}
        open={modalOpen}
        onCancel={closeModal}
        onOk={() => form.submit()}
        confirmLoading={saveMutation.isPending}
        width={editingSystemManaged ? 480 : 760}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={(values) => saveMutation.mutate(values)}>
          {editingSystemManaged && editing ? (
            <>
              <Form.Item label={t('table.name')}>
                <Space direction="vertical" size={0}>
                  <Typography.Text strong>{mcpDisplayName(editing, t)}</Typography.Text>
                  <Typography.Text type="secondary">{editing.name}</Typography.Text>
                </Space>
              </Form.Item>
              <Form.Item name="enabled" label={t('field.enabled')} valuePropName="checked">
                <Switch />
              </Form.Item>
            </>
          ) : (
            <>
              <div className="form-grid">
                <Form.Item name="name" label={t('field.internalName')} rules={[{ required: true }]}>
                  <Input />
                </Form.Item>
                <Form.Item name="display_name" label={t('field.displayName')}>
                  <Input />
                </Form.Item>
                <Form.Item name="visibility" label={t('field.visibility')}>
                  <Select
                    options={[
                      { value: 'private', label: t('visibility.private') },
                      ...(isAdmin
                        ? [
                            { value: 'public', label: t('visibility.public') },
                            { value: 'system_private', label: t('visibility.system_private') },
                          ]
                        : []),
                    ]}
                  />
                </Form.Item>
                <Form.Item name="enabled" label={t('field.enabled')} valuePropName="checked">
                  <Switch />
                </Form.Item>
              </div>
              <Form.Item name="description" label={t('field.description')}>
                <Input.TextArea rows={2} />
              </Form.Item>
              <Form.Item name="runtime_kind" label={t('field.runtimeKind')} rules={[{ required: true }]}>
                <Select
                  options={runtimeKinds.map((value) => ({ value, label: runtimeKindLabel(value, t) }))}
                />
              </Form.Item>
              {runtimeUsesCommand(runtimeKind) ? (
                <div className="form-grid two">
                  <Form.Item name="command" label={t('field.command')} rules={[{ required: true }]}>
                    <Input />
                  </Form.Item>
                  <Form.Item name="cwd" label={t('field.cwd')}>
                    <Input />
                  </Form.Item>
                  <Form.Item name="args_json" label={t('field.argsJson')}>
                    <Input.TextArea rows={4} />
                  </Form.Item>
                  <Form.Item name="env_json" label={t('field.envJson')}>
                    <Input.TextArea rows={4} />
                  </Form.Item>
                </div>
              ) : null}
              {runtimeUsesHttp(runtimeKind) ? (
                <div className="form-grid two">
                  <Form.Item name="url" label={t('field.url')} rules={[{ required: true }]}>
                    <Input />
                  </Form.Item>
                  <Form.Item name="headers_json" label={t('field.headersJson')}>
                    <Input.TextArea rows={4} />
                  </Form.Item>
                </div>
              ) : null}
              {runtimeUsesLocalConnector(runtimeKind) ? (
                <Form.Item
                  name="local_connector_json"
                  label={t('field.localConnectorJson')}
                  rules={[{ required: true }]}
                >
                  <Input.TextArea rows={4} />
                </Form.Item>
              ) : null}
            </>
          )}
        </Form>
      </Modal>
    </div>
  );
}

function buildMcpPayload(values: Record<string, unknown>, isAdmin: boolean) {
  const runtimeKind = values.runtime_kind as RuntimeKind;
  const runtime: Record<string, unknown> = { kind: runtimeKind };

  if (runtimeUsesCommand(runtimeKind)) {
    runtime.command = optionalText(values.command);
    runtime.cwd = optionalText(values.cwd);
    runtime.args = parseJsonArray(values.args_json, []);
    runtime.env = parseJsonObject(values.env_json, {});
  }
  if (runtimeUsesHttp(runtimeKind)) {
    runtime.url = optionalText(values.url);
    runtime.headers = parseJsonObject(values.headers_json, {});
  }
  if (runtimeUsesLocalConnector(runtimeKind)) {
    runtime.local_connector = parseJsonObject(values.local_connector_json, {});
  }

  const payload: Record<string, unknown> = {
    name: optionalText(values.name),
    display_name: optionalText(values.display_name),
    description: optionalText(values.description),
    visibility: values.visibility || 'private',
    enabled: Boolean(values.enabled),
    runtime,
  };
  if (!isAdmin && payload.visibility !== 'private') {
    payload.visibility = 'private';
  }
  return payload;
}

function isSystemManagedMcp(record: McpRecord): boolean {
  return record.source_kind === 'system_seed' || record.runtime.kind === 'builtin';
}

function runtimeUsesCommand(kind: RuntimeKind | undefined): boolean {
  return kind === 'stdio_cloud' || kind === 'local_connector_stdio';
}

function runtimeUsesHttp(kind: RuntimeKind | undefined): boolean {
  return kind === 'http' || kind === 'local_connector_http';
}

function runtimeUsesLocalConnector(kind: RuntimeKind | undefined): boolean {
  return (
    kind === 'local_connector_stdio' ||
    kind === 'local_connector_http' ||
    kind === 'local_connector_builtin_proxy'
  );
}

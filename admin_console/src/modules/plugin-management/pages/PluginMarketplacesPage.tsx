// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EditOutlined, PlusOutlined, SyncOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Form, Input, Modal, Select, Space, Switch, Table, Tag, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginMarketplaceRecord } from '../pluginTypes';
import type { CurrentUser } from '../types';
import { optionalText, parseJsonArray } from './formUtils';

export function PluginMarketplacesPage({ user }: { user: CurrentUser }) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const [editingMarketplace, setEditingMarketplace] = useState<PluginMarketplaceRecord | null>(null);
  const isAdmin = user.role === 'super_admin';
  const effectiveOwnerId = user.owner_user_id?.trim() || user.user_id;
  const marketplacesQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-marketplaces'],
    queryFn: api.listPluginMarketplaces,
  });
  const createMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) =>
      api.createPluginMarketplace({
        id: optionalText(values.id),
        name: values.name,
        source_kind: values.source_kind,
        catalog_url: optionalText(values.catalog_url),
        enabled: values.enabled !== false,
        trust_level: values.trust_level,
        trusted_signing_keys: parseJsonArray(values.trusted_signing_keys_json),
      }),
    onSuccess: () => {
      message.success(t('pluginMarketplace.created'));
      setModalOpen(false);
      setEditingMarketplace(null);
      form.resetFields();
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-marketplaces'] });
    },
    onError: (error) => message.error((error as Error).message),
  });
  const updateMutation = useMutation({
    mutationFn: ({ marketplaceId, values }: { marketplaceId: string; values: Record<string, unknown> }) =>
      api.updateAdminPluginMarketplace(marketplaceId, {
        name: values.name,
        catalog_url: optionalText(values.catalog_url),
        enabled: values.enabled !== false,
        trust_level: values.trust_level,
        trusted_signing_keys: parseJsonArray(values.trusted_signing_keys_json),
      }),
    onSuccess: () => {
      message.success(t('pluginMarketplace.updated'));
      setModalOpen(false);
      setEditingMarketplace(null);
      form.resetFields();
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-marketplaces'] });
    },
    onError: (error) => message.error((error as Error).message),
  });
  const syncMutation = useMutation({
    mutationFn: (marketplaceId: string) => api.syncPluginMarketplace(marketplaceId),
    onSuccess: (result) => {
      message.success(t('pluginMarketplace.synced', { revision: result.revision }));
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-marketplaces'] });
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'admin-plugins'] });
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-releases'] });
    },
    onError: (error) => message.error((error as Error).message),
  });
  const columns = useMemo<ColumnsType<PluginMarketplaceRecord>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{record.name}</Typography.Text>
            <CompactId value={record.id} />
          </Space>
        ),
      },
      {
        title: t('pluginMarketplace.visibility'),
        dataIndex: 'visibility',
        width: 120,
        render: (value, record) => (
          <Space direction="vertical" size={0}>
            <Tag color={value === 'private' ? 'gold' : 'cyan'}>
              {t(`pluginMarketplace.visibility.${value}`)}
            </Tag>
            {record.owner_user_id ? <CompactId value={record.owner_user_id} /> : null}
          </Space>
        ),
      },
      {
        title: t('pluginMarketplace.source'),
        dataIndex: 'source_kind',
        width: 170,
        render: (value) => <Tag color="blue">{t(`pluginMarketplace.source.${value}`)}</Tag>,
      },
      {
        title: t('pluginMarketplace.trust'),
        dataIndex: 'trust_level',
        width: 120,
        render: (value) => (
          <Tag color={value === 'trusted' ? 'green' : 'red'}>
            {t(`pluginMarketplace.trust.${value}`)}
          </Tag>
        ),
      },
      {
        title: t('table.status'),
        dataIndex: 'enabled',
        width: 100,
        render: (enabled) => <EnabledTag enabled={enabled} />,
      },
      {
        title: t('pluginMarketplace.keys'),
        dataIndex: 'trusted_signing_keys',
        width: 100,
        render: (keys) => keys?.length || 0,
      },
      {
        title: t('pluginMarketplace.revision'),
        dataIndex: 'last_catalog_revision',
        width: 170,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('pluginMarketplace.syncedAt'),
        dataIndex: 'last_synced_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 190,
        fixed: 'right',
        render: (_, record) => {
          const writable =
            isAdmin ||
            (record.visibility === 'private' && record.owner_user_id === effectiveOwnerId);
          const syncable =
            writable &&
            record.enabled &&
            record.trust_level === 'trusted' &&
            Boolean(record.catalog_url);
          return (
            <Space size="small">
              {isAdmin ? (
                <Button
                  size="small"
                  icon={<EditOutlined />}
                  onClick={() => {
                    setEditingMarketplace(record);
                    form.setFieldsValue({
                      id: record.id,
                      name: record.name,
                      source_kind: record.source_kind,
                      catalog_url: record.catalog_url || '',
                      enabled: record.enabled,
                      trust_level: record.trust_level,
                      trusted_signing_keys_json: JSON.stringify(record.trusted_signing_keys || [], null, 2),
                    });
                    setModalOpen(true);
                  }}
                >
                  {t('pluginMarketplace.edit')}
                </Button>
              ) : null}
              <Button
                size="small"
                icon={<SyncOutlined />}
                disabled={!syncable}
                loading={syncMutation.isPending && syncMutation.variables === record.id}
                onClick={() => syncMutation.mutate(record.id)}
              >
                {t('pluginMarketplace.sync')}
              </Button>
            </Space>
          );
        },
      },
    ],
    [effectiveOwnerId, isAdmin, syncMutation, t],
  );

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginMarketplace.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginMarketplace.description')}</Typography.Text>
        </Space>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditingMarketplace(null);
            form.setFieldsValue({
              source_kind: 'admin_registry',
              trust_level: 'trusted',
              enabled: true,
              trusted_signing_keys_json: '[]',
            });
            setModalOpen(true);
          }}
        >
          {t('pluginMarketplace.add')}
        </Button>
      </div>
      {!isAdmin ? (
        <Alert type="info" showIcon message={t('pluginMarketplace.personalNotice')} />
      ) : null}
      <Table
        rowKey="id"
        columns={columns}
        dataSource={marketplacesQuery.data?.items || []}
        loading={marketplacesQuery.isLoading}
        scroll={{ x: 1340 }}
        pagination={false}
      />
      <Modal
        title={t(editingMarketplace ? 'pluginMarketplace.editTitle' : 'pluginMarketplace.addTitle')}
        open={modalOpen}
        onCancel={() => {
          setModalOpen(false);
          setEditingMarketplace(null);
          form.resetFields();
        }}
        onOk={() => form.submit()}
        confirmLoading={createMutation.isPending || updateMutation.isPending}
        width={760}
        destroyOnClose
      >
        <Form
          form={form}
          layout="vertical"
          onFinish={(values) => {
            if (editingMarketplace) {
              updateMutation.mutate({ marketplaceId: editingMarketplace.id, values });
            } else {
              createMutation.mutate(values);
            }
          }}
        >
          <div className="form-grid">
            <Form.Item name="name" label={t('table.name')} rules={[{ required: true }]}>
              <Input placeholder="team-plugins" />
            </Form.Item>
            {isAdmin ? (
              <Form.Item name="id" label={t('pluginMarketplace.id')}>
                <Input disabled={Boolean(editingMarketplace)} placeholder={t('pluginMarketplace.idHint')} />
              </Form.Item>
            ) : null}
            {isAdmin ? (
              <Form.Item name="source_kind" label={t('pluginMarketplace.source')} rules={[{ required: true }]}>
                <Select
                  disabled={Boolean(editingMarketplace)}
                  options={['official_registry', 'admin_registry'].map((value) => ({
                    value,
                    label: t(`pluginMarketplace.source.${value}`),
                  }))}
                />
              </Form.Item>
            ) : null}
            {isAdmin ? (
              <Form.Item name="trust_level" label={t('pluginMarketplace.trust')} rules={[{ required: true }]}>
                <Select
                  options={['trusted', 'untrusted'].map((value) => ({
                    value,
                    label: t(`pluginMarketplace.trust.${value}`),
                  }))}
                />
              </Form.Item>
            ) : null}
          </div>
          <Form.Item
            name="catalog_url"
            label={t('pluginMarketplace.catalogUrl')}
            rules={isAdmin ? undefined : [{ required: true }]}
          >
            <Input placeholder="https://plugins.example.com/catalog.json" />
          </Form.Item>
          <Form.Item name="enabled" label={t('field.enabled')} valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="trusted_signing_keys_json" label={t('pluginMarketplace.keysJson')}>
            <Input.TextArea rows={9} className="code-input" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}

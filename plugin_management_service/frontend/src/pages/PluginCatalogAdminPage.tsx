// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PlusOutlined, RocketOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Form, Input, Modal, Select, Space, Switch, Table, Tag, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginCatalogListItem, PluginRuntimeTarget } from '../pluginTypes';
import type { CurrentUser } from '../types';
import { optionalText, parseJsonArray } from './formUtils';

interface PluginCatalogAdminPageProps {
  user: CurrentUser;
  onOpenReleases: (pluginId: string) => void;
}

const RUNTIME_TARGET_ORDER: PluginRuntimeTarget[] = ['local_connector'];
const RUNTIME_TARGET_COLORS: Record<PluginRuntimeTarget, string> = {
  local_connector: 'purple',
};

function renderRuntimeTargets(
  targets: PluginRuntimeTarget[] | undefined,
  latestReleaseId: string,
  t: (key: string, values?: Record<string, string | number>) => string,
) {
  if (!latestReleaseId) {
    return <Tag>{t('pluginCatalog.runtime.unpublished')}</Tag>;
  }
  const targetSet = new Set(targets || []);
  const orderedTargets = RUNTIME_TARGET_ORDER.filter((target) => targetSet.has(target));
  if (orderedTargets.length === 0) {
    return <Tag>{t('pluginCatalog.runtime.unknown')}</Tag>;
  }
  return (
    <Space size={[4, 4]} wrap>
      {orderedTargets.map((target) => (
        <Tag key={target} color={RUNTIME_TARGET_COLORS[target]}>
          {t(`pluginCatalog.runtime.${target}`)}
        </Tag>
      ))}
    </Space>
  );
}

export function PluginCatalogAdminPage({ user, onOpenReleases }: PluginCatalogAdminPageProps) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const isAdmin = user.role === 'super_admin';
  const pluginsQuery = useQuery({
    queryKey: ['admin-plugins'],
    queryFn: () => api.listAdminPlugins({ limit: 500 }),
    enabled: isAdmin,
  });
  const marketplacesQuery = useQuery({
    queryKey: ['plugin-marketplaces'],
    queryFn: api.listPluginMarketplaces,
    enabled: isAdmin,
  });
  const createMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) => api.createPluginCatalogEntry({
      marketplace_id: values.marketplace_id,
      name: values.name,
      display_name: values.display_name,
      description: values.description,
      publisher: {
        id: values.publisher_id,
        name: values.publisher_name,
        website: optionalText(values.publisher_website),
        verified: values.publisher_verified === true,
      },
      interface: {
        displayName: values.display_name,
        shortDescription: values.short_description,
        longDescription: values.long_description,
        developerName: values.developer_name,
        category: values.category,
        capabilities: parseJsonArray(values.capabilities_json),
        websiteURL: optionalText(values.website_url),
        privacyPolicyURL: optionalText(values.privacy_policy_url),
        termsOfServiceURL: optionalText(values.terms_url),
        defaultPrompt: parseJsonArray(values.default_prompt_json),
        brandColor: optionalText(values.brand_color),
        screenshots: [],
      },
      keywords: parseJsonArray(values.keywords_json),
      visibility: values.visibility,
      featured: values.featured === true,
      enabled: values.enabled !== false,
      license: {
        license_id: values.license_id,
        license_url: optionalText(values.license_url),
        redistributable: values.redistributable === true,
        reviewed_at: optionalText(values.reviewed_at),
      },
    }),
    onSuccess: () => {
      message.success(t('pluginCatalog.created'));
      setModalOpen(false);
      queryClient.invalidateQueries({ queryKey: ['admin-plugins'] });
    },
    onError: (error) => message.error((error as Error).message),
  });
  const columns = useMemo<ColumnsType<PluginCatalogListItem>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'display_name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Space>
              <Typography.Text strong>{record.display_name}</Typography.Text>
              {record.featured ? <Tag color="gold">{t('pluginCatalog.featured')}</Tag> : null}
            </Space>
            <Typography.Text type="secondary">{record.name}</Typography.Text>
            <Typography.Text type="secondary" ellipsis={{ tooltip: record.description }}>
              {record.description}
            </Typography.Text>
          </Space>
        ),
      },
      { title: t('pluginCatalog.marketplace'), dataIndex: 'marketplace_id', width: 160 },
      {
        title: t('pluginCatalog.publisher'),
        dataIndex: ['publisher', 'name'],
        width: 160,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text>{record.publisher.name}</Typography.Text>
            <CompactId value={record.publisher.id} />
          </Space>
        ),
      },
      { title: t('pluginCatalog.category'), dataIndex: ['interface', 'category'], width: 150 },
      {
        title: t('pluginCatalog.runtimeTargets'),
        dataIndex: 'runtime_targets',
        width: 150,
        render: (_, record) => renderRuntimeTargets(record.runtime_targets, record.latest_release_id, t),
      },
      {
        title: t('pluginCatalog.license'),
        dataIndex: ['license', 'license_id'],
        width: 190,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <CompactId value={record.license.license_id} />
            <Typography.Text type={record.license.redistributable ? 'success' : 'warning'}>
              {t(record.license.redistributable ? 'pluginCatalog.redistributable' : 'pluginCatalog.notRedistributable')}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('pluginCatalog.latestRelease'),
        dataIndex: 'latest_release_id',
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
        width: 130,
        render: (_, record) => (
          <Button icon={<RocketOutlined />} onClick={() => onOpenReleases(record.id)}>
            {t('pluginCatalog.releases')}
          </Button>
        ),
      },
    ],
    [onOpenReleases, t],
  );

  if (!isAdmin) {
    return <Alert type="error" showIcon message={t('admin.only')} />;
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginCatalog.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginCatalog.description')}</Typography.Text>
        </Space>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            form.setFieldsValue({
              marketplace_id: marketplacesQuery.data?.items[0]?.id,
              visibility: 'public',
              publisher_verified: false,
              featured: false,
              enabled: true,
              redistributable: false,
              keywords_json: '[]',
              capabilities_json: '["Skills"]',
              default_prompt_json: '[]',
            });
            setModalOpen(true);
          }}
        >
          {t('pluginCatalog.add')}
        </Button>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={pluginsQuery.data?.items || []}
        loading={pluginsQuery.isLoading}
        scroll={{ x: 1450 }}
        pagination={{ pageSize: 12 }}
      />
      <Modal
        title={t('pluginCatalog.addTitle')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={createMutation.isPending}
        width={900}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={(values) => createMutation.mutate(values)}>
          <div className="form-grid">
            <Form.Item name="marketplace_id" label={t('pluginCatalog.marketplace')} rules={[{ required: true }]}>
              <Select options={(marketplacesQuery.data?.items || []).map((item) => ({ value: item.id, label: item.name }))} />
            </Form.Item>
            <Form.Item name="name" label={t('pluginCatalog.internalName')} rules={[{ required: true }]}>
              <Input placeholder="documents" />
            </Form.Item>
            <Form.Item name="display_name" label={t('field.displayName')} rules={[{ required: true }]}>
              <Input />
            </Form.Item>
            <Form.Item name="category" label={t('pluginCatalog.category')} rules={[{ required: true }]}>
              <Input />
            </Form.Item>
          </div>
          <Form.Item name="description" label={t('field.description')} rules={[{ required: true }]}>
            <Input.TextArea rows={2} />
          </Form.Item>
          <div className="form-grid">
            <Form.Item name="publisher_id" label={t('pluginCatalog.publisherId')} rules={[{ required: true }]}><Input /></Form.Item>
            <Form.Item name="publisher_name" label={t('pluginCatalog.publisherName')} rules={[{ required: true }]}><Input /></Form.Item>
            <Form.Item name="publisher_website" label={t('pluginCatalog.publisherWebsite')}><Input /></Form.Item>
            <Form.Item name="developer_name" label={t('pluginCatalog.developerName')} rules={[{ required: true }]}><Input /></Form.Item>
          </div>
          <Form.Item name="short_description" label={t('pluginCatalog.shortDescription')} rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="long_description" label={t('pluginCatalog.longDescription')} rules={[{ required: true }]}><Input.TextArea rows={3} /></Form.Item>
          <div className="form-grid">
            <Form.Item name="visibility" label={t('table.visibility')}><Select options={['public', 'private'].map((value) => ({ value, label: t(`visibility.${value}`) }))} /></Form.Item>
            <Form.Item name="brand_color" label={t('pluginCatalog.brandColor')}><Input placeholder="#1ABCFE" /></Form.Item>
            <Form.Item name="license_id" label={t('pluginCatalog.licenseId')} rules={[{ required: true }]}><Input /></Form.Item>
            <Form.Item name="license_url" label={t('pluginCatalog.licenseUrl')}><Input /></Form.Item>
          </div>
          <div className="form-grid">
            <Form.Item name="publisher_verified" label={t('pluginCatalog.publisherVerified')} valuePropName="checked"><Switch /></Form.Item>
            <Form.Item name="featured" label={t('pluginCatalog.featured')} valuePropName="checked"><Switch /></Form.Item>
            <Form.Item name="enabled" label={t('field.enabled')} valuePropName="checked"><Switch /></Form.Item>
            <Form.Item name="redistributable" label={t('pluginCatalog.redistributable')} valuePropName="checked"><Switch /></Form.Item>
          </div>
          <Form.Item name="reviewed_at" label={t('pluginCatalog.reviewedAt')}><Input placeholder="2026-07-22T00:00:00Z" /></Form.Item>
          <div className="form-grid">
            <Form.Item name="keywords_json" label={t('pluginCatalog.keywordsJson')}><Input.TextArea rows={4} className="code-input" /></Form.Item>
            <Form.Item name="capabilities_json" label={t('pluginCatalog.capabilitiesJson')}><Input.TextArea rows={4} className="code-input" /></Form.Item>
          </div>
          <Form.Item name="default_prompt_json" label={t('pluginCatalog.defaultPromptJson')}><Input.TextArea rows={3} className="code-input" /></Form.Item>
          <div className="form-grid">
            <Form.Item name="website_url" label={t('pluginCatalog.websiteUrl')}><Input /></Form.Item>
            <Form.Item name="privacy_policy_url" label={t('pluginCatalog.privacyUrl')}><Input /></Form.Item>
            <Form.Item name="terms_url" label={t('pluginCatalog.termsUrl')}><Input /></Form.Item>
          </div>
        </Form>
      </Modal>
    </div>
  );
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { InboxOutlined } from '@ant-design/icons';
import { useMutation } from '@tanstack/react-query';
import {
  Alert,
  AutoComplete,
  Descriptions,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Switch,
  Tag,
  Typography,
  Upload,
  message,
} from 'antd';
import type { FormInstance } from 'antd';
import type { UploadFile } from 'antd/es/upload/interface';
import { useEffect, useMemo, useState } from 'react';

import { api } from '../../api/client';
import { useI18n } from '../../i18n/I18nProvider';
import type {
  PluginMarketplaceRecord,
  PluginPackageAnalysis,
  PluginPublisherRecord,
  PublishUploadedPluginResponse,
} from '../../pluginTypes';

interface Props {
  open: boolean;
  marketplaces: PluginMarketplaceRecord[];
  publishers: PluginPublisherRecord[];
  onClose: () => void;
  onPublished: (result: PublishUploadedPluginResponse) => void;
}

export function PluginPublishWizard({ open, marketplaces, publishers, onClose, onPublished }: Props) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const [packageFiles, setPackageFiles] = useState<UploadFile[]>([]);
  const [manifestFiles, setManifestFiles] = useState<UploadFile[]>([]);
  const [analysis, setAnalysis] = useState<PluginPackageAnalysis | null>(null);
  const selectedMarketplace = Form.useWatch('marketplace_id', form);
  const selectedPublisherId = Form.useWatch('publisher_id', form);
  const eligiblePublishers = useMemo(
    () => publishers.filter((publisher) =>
      publisher.status === 'approved' && publisher.marketplace_id === selectedMarketplace),
    [publishers, selectedMarketplace],
  );
  const matchedPublisher = useMemo(
    () => eligiblePublishers.find((publisher) => publisher.publisher_id === selectedPublisherId),
    [eligiblePublishers, selectedPublisherId],
  );

  useEffect(() => {
    if (!open) return;
    const marketplaceId = marketplaces.find((item) =>
      item.enabled && item.trust_level === 'trusted' && item.source_kind === 'admin_registry')?.id;
    form.setFieldsValue({
      marketplace_id: marketplaceId,
      visibility: 'public',
      release_channel: 'stable',
      redistributable: false,
      featured: false,
    });
  }, [form, marketplaces, open]);

  const analyzeMutation = useMutation({
    mutationFn: async () => {
      const packageFile = packageFiles[0]?.originFileObj;
      if (!packageFile) throw new Error(t('pluginPublish.packageRequired'));
      const payload = new FormData();
      payload.append('package', packageFile);
      const manifestFile = manifestFiles[0]?.originFileObj;
      if (manifestFile) payload.append('manifest', manifestFile);
      return api.analyzePluginPackage(payload);
    },
    onSuccess: (result) => {
      setAnalysis(result);
      const suggestedPublisherId = suggestPublisherId(result);
      const existingPublisher = eligiblePublishers.find(
        (publisher) => publisher.publisher_id === suggestedPublisherId,
      );
      form.setFieldsValue({
        license_id: result.manifest.license || 'NOASSERTION',
        publisher_id: suggestedPublisherId,
        publisher_name: existingPublisher?.name || result.manifest.author.name,
        publisher_website: existingPublisher?.website || result.manifest.author.url || '',
      });
      message.success(t('pluginPublish.analyzed'));
    },
    onError: (error) => message.error((error as Error).message),
  });

  const publishMutation = useMutation({
    mutationFn: async (values: Record<string, unknown>) => {
      if (!analysis) throw new Error(t('pluginPublish.analyzeFirst'));
      return api.publishUploadedPlugin({
        artifact_sha256: analysis.artifact_sha256,
        marketplace_id: values.marketplace_id,
        publisher_id: values.publisher_id,
        publisher_name: textOrNull(values.publisher_name),
        publisher_website: textOrNull(values.publisher_website),
        license_id: values.license_id,
        license_url: textOrNull(values.license_url),
        redistributable: values.redistributable === true,
        visibility: values.visibility,
        featured: values.featured === true,
        release_channel: values.release_channel,
      });
    },
    onSuccess: (result) => {
      message.success(t('pluginPublish.published'));
      onPublished(result);
      resetAndClose();
    },
    onError: (error) => message.error((error as Error).message),
  });

  const resetAndClose = () => {
    form.resetFields();
    setPackageFiles([]);
    setManifestFiles([]);
    setAnalysis(null);
    onClose();
  };

  return (
    <Modal
      title={t('pluginPublish.title')}
      open={open}
      onCancel={resetAndClose}
      onOk={() => analysis ? form.submit() : analyzeMutation.mutate()}
      okText={analysis ? t('pluginPublish.publish') : t('pluginPublish.analyze')}
      confirmLoading={analyzeMutation.isPending || publishMutation.isPending}
      width={920}
      destroyOnClose
    >
      <Alert
        type="info"
        showIcon
        message={t('pluginPublish.helpTitle')}
        description={t('pluginPublish.helpDescription')}
        style={{ marginBottom: 16 }}
      />
      <Form form={form} layout="vertical" onFinish={(values) => publishMutation.mutate(values)}>
        <div className="form-grid">
          <Form.Item name="marketplace_id" label={t('pluginCatalog.marketplace')} rules={[{ required: true }]}>
            <Select
              options={marketplaces
                .filter((item) => item.enabled && item.trust_level === 'trusted' && item.source_kind === 'admin_registry')
                .map((item) => ({ value: item.id, label: `${item.name} · ${item.id}` }))}
              onChange={() => setAnalysis(null)}
            />
          </Form.Item>
        </div>
        <div className="form-grid">
          <Form.Item label={t('pluginPublish.package')} required>
            <Upload.Dragger
              accept=".tgz,application/gzip,application/octet-stream"
              maxCount={1}
              beforeUpload={() => false}
              fileList={packageFiles}
              onChange={({ fileList }) => { setPackageFiles(fileList.slice(-1)); setAnalysis(null); }}
            >
              <p className="ant-upload-drag-icon"><InboxOutlined /></p>
              <p>{t('pluginPublish.packageDrop')}</p>
            </Upload.Dragger>
          </Form.Item>
          <Form.Item label={t('pluginPublish.manifest')} extra={t('pluginPublish.manifestExtra')}>
            <Upload.Dragger
              accept=".json,application/json"
              maxCount={1}
              beforeUpload={() => false}
              fileList={manifestFiles}
              onChange={({ fileList }) => { setManifestFiles(fileList.slice(-1)); setAnalysis(null); }}
            >
              <p className="ant-upload-drag-icon"><InboxOutlined /></p>
              <p>{t('pluginPublish.manifestDrop')}</p>
            </Upload.Dragger>
          </Form.Item>
        </div>

        {analysis ? (
          <>
            <Typography.Title level={5}>{t('pluginPublish.preview')}</Typography.Title>
            <Descriptions bordered size="small" column={2} style={{ marginBottom: 16 }}>
              <Descriptions.Item label={t('pluginCatalog.internalName')}>{analysis.manifest.name}</Descriptions.Item>
              <Descriptions.Item label={t('pluginRelease.version')}>{analysis.manifest.version}</Descriptions.Item>
              <Descriptions.Item label={t('pluginRelease.npmPackageName')}>{analysis.package_name}</Descriptions.Item>
              <Descriptions.Item label={t('pluginRelease.components')}>
                <Space size={[4, 4]} wrap>
                  {analysis.components.map((component) => (
                    <Tag key={component.component_key}>{component.kind}: {component.component_key}</Tag>
                  ))}
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label={t('pluginPublish.packageBins')} span={2}>
                <Space size={[4, 4]} wrap>
                  {analysis.package_bins.map((bin) => <Tag key={bin}>{bin}</Tag>)}
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label={t('pluginPublish.permissions')} span={2}>
                <Space size={[4, 4]} wrap>
                  {(analysis.manifest.permissions || []).map((permission) => (
                    <Tag key={`${permission.permission}:${permission.components.join(',')}`} color={permission.required ? 'orange' : 'default'}>
                      {permission.permission}{permission.required ? ' · required' : ''}
                    </Tag>
                  ))}
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label={t('pluginRelease.artifactHash')} span={2}>
                <Typography.Text code copyable>{analysis.artifact_sha256}</Typography.Text>
              </Descriptions.Item>
              <Descriptions.Item label={t('pluginRelease.npmPackageIntegrity')} span={2}>
                <Typography.Text code copyable ellipsis>{analysis.npm_integrity}</Typography.Text>
              </Descriptions.Item>
            </Descriptions>
            <Typography.Title level={5}>{t('pluginPublish.publisherSection')}</Typography.Title>
            <Alert
              type={matchedPublisher ? 'success' : 'info'}
              showIcon
              message={matchedPublisher ? t('pluginPublish.publisherReuse') : t('pluginPublish.publisherCreate')}
              description={t('pluginPublish.publisherHelp')}
              style={{ marginBottom: 16 }}
            />
            <div className="form-grid">
              <Form.Item
                name="publisher_id"
                label={t('pluginPublisher.id')}
                rules={[
                  { required: true },
                  { pattern: /^[a-z0-9]+(?:-[a-z0-9]+)*$/, message: t('pluginPublish.publisherIdInvalid') },
                ]}
              >
                <AutoComplete
                  options={eligiblePublishers.map((publisher) => ({
                    value: publisher.publisher_id,
                    label: `${publisher.name} · ${publisher.publisher_id}`,
                  }))}
                  placeholder={t('pluginPublish.publisherPlaceholder')}
                  onSelect={(value) => applyExistingPublisher(form, eligiblePublishers, value)}
                />
              </Form.Item>
              <Form.Item name="publisher_name" label={t('pluginPublish.publisherName')} rules={[{ required: true }]}>
                <Input />
              </Form.Item>
              <Form.Item name="publisher_website" label={t('pluginPublish.publisherWebsite')}>
                <Input placeholder="https://..." />
              </Form.Item>
            </div>
            <div className="form-grid">
              <Form.Item name="release_channel" label={t('pluginRelease.channel')} rules={[{ required: true }]}>
                <Select options={['stable', 'beta', 'canary'].map((value) => ({ value, label: value }))} />
              </Form.Item>
              <Form.Item name="visibility" label={t('table.visibility')} rules={[{ required: true }]}>
                <Select options={['public', 'private'].map((value) => ({ value, label: t(`visibility.${value}`) }))} />
              </Form.Item>
              <Form.Item name="license_id" label={t('pluginCatalog.licenseId')} rules={[{ required: true }]}>
                <Input />
              </Form.Item>
              <Form.Item name="license_url" label={t('pluginCatalog.licenseUrl')}>
                <Input placeholder="https://..." />
              </Form.Item>
            </div>
            <div className="form-grid">
              <Form.Item name="redistributable" label={t('pluginCatalog.redistributable')} valuePropName="checked">
                <Switch />
              </Form.Item>
              <Form.Item name="featured" label={t('pluginCatalog.featured')} valuePropName="checked">
                <Switch />
              </Form.Item>
            </div>
          </>
        ) : null}
      </Form>
    </Modal>
  );
}

function textOrNull(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function suggestPublisherId(analysis: PluginPackageAnalysis): string {
  const packageScope = analysis.package_name.match(/^@([^/]+)\//)?.[1];
  return slugifyPublisherId(packageScope || analysis.manifest.author.name)
    || slugifyPublisherId(analysis.manifest.name)
    || 'publisher';
}

function slugifyPublisherId(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-{2,}/g, '-')
    .slice(0, 64)
    .replace(/-+$/g, '');
}

function applyExistingPublisher(
  form: FormInstance,
  publishers: PluginPublisherRecord[],
  publisherId: string,
) {
  const publisher = publishers.find((item) => item.publisher_id === publisherId);
  if (!publisher) return;
  form.setFieldsValue({
    publisher_name: publisher.name,
    publisher_website: publisher.website || '',
  });
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { InboxOutlined } from '@ant-design/icons';
import { useMutation } from '@tanstack/react-query';
import {
  Alert,
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
  const eligiblePublishers = useMemo(
    () => publishers.filter((publisher) =>
      publisher.status === 'approved' && publisher.marketplace_id === selectedMarketplace),
    [publishers, selectedMarketplace],
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

  useEffect(() => {
    if (eligiblePublishers.length > 0
      && !eligiblePublishers.some((publisher) => publisher.publisher_id === form.getFieldValue('publisher_id'))) {
      form.setFieldValue('publisher_id', eligiblePublishers[0].publisher_id);
    }
  }, [eligiblePublishers, form]);

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
      form.setFieldsValue({
        license_id: result.manifest.license || 'NOASSERTION',
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
          <Form.Item name="publisher_id" label={t('pluginCatalog.publisher')} rules={[{ required: true }]}>
            <Select
              options={eligiblePublishers.map((publisher) => ({
                value: publisher.publisher_id,
                label: `${publisher.name} · ${publisher.publisher_id}`,
              }))}
              placeholder={t('pluginPublish.publisherPlaceholder')}
            />
          </Form.Item>
        </div>
        {eligiblePublishers.length === 0 ? (
          <Alert type="warning" showIcon message={t('pluginPublish.noPublisher')} style={{ marginBottom: 16 }} />
        ) : null}
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

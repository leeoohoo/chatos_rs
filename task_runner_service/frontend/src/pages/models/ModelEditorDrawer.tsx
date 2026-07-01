// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';
import {
  Button,
  Drawer,
  Form,
  Input,
  InputNumber,
  Select,
  Space,
  Switch,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ModelCatalogResponse, ModelConfigRecord } from '../../types';
import {
  type ModelFormValues,
  SUPPORTED_PROVIDER_OPTIONS,
} from './modelPageUtils';

type ModelEditorDrawerProps = {
  t: TranslateFn;
  open: boolean;
  editingModel: ModelConfigRecord | null;
  form: FormInstance<ModelFormValues>;
  saving: boolean;
  modelOptions: Array<{ label: string; value: string }>;
  thinkingLevelOptions: Array<{ label: string; value: string }>;
  modelCatalog: ModelCatalogResponse | null;
  watchedApiKey?: string;
  catalogLoading: boolean;
  onClose: () => void;
  onSubmit: (values: ModelFormValues) => void;
  onValuesChange: (changedValues: Partial<ModelFormValues>) => void;
  onFetchCatalog: () => void;
};

export function ModelEditorDrawer({
  t,
  open,
  editingModel,
  form,
  saving,
  modelOptions,
  thinkingLevelOptions,
  modelCatalog,
  watchedApiKey,
  catalogLoading,
  onClose,
  onSubmit,
  onValuesChange,
  onFetchCatalog,
}: ModelEditorDrawerProps) {
  return (
    <Drawer
      title={editingModel ? t('models.drawer.edit') : t('models.drawer.create')}
      open={open}
      width={560}
      destroyOnClose
      onClose={onClose}
      extra={
        <Space>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button type="primary" loading={saving} onClick={() => form.submit()}>
            {t('common.save')}
          </Button>
        </Space>
      }
    >
      <Form<ModelFormValues>
        layout="vertical"
        form={form}
        onFinish={onSubmit}
        onValuesChange={onValuesChange}
      >
        <Form.Item name="name" label={t('models.column.name')} rules={[{ required: true }]}>
          <Input />
        </Form.Item>
        <Form.Item
          name="usage_scenario"
          label={t('models.column.usageScenario')}
          extra={t('models.form.usageScenarioHint')}
        >
          <Input.TextArea
            rows={3}
            placeholder={t('models.form.usageScenarioPlaceholder')}
          />
        </Form.Item>
        <Space size="middle" style={{ width: '100%' }} align="start">
          <Form.Item
            name="provider"
            label="Provider"
            style={{ flex: 1 }}
            rules={[{ required: true }]}
          >
            <Select options={SUPPORTED_PROVIDER_OPTIONS} />
          </Form.Item>
        </Space>
        <Form.Item name="base_url" label="Base URL" rules={[{ required: true }]}>
          <Input />
        </Form.Item>
        <Form.Item name="api_key" label="API Key">
          <Input.Password />
        </Form.Item>
        <Form.Item label="Model" required>
          <Space.Compact style={{ width: '100%' }}>
            <Form.Item
              name="model"
              noStyle
              rules={[{ required: true, message: t('models.form.modelRequired') }]}
            >
              <Select
                showSearch
                allowClear
                placeholder={t('models.form.modelPlaceholder')}
                options={modelOptions}
                optionFilterProp="label"
                notFoundContent={
                  catalogLoading
                    ? t('models.form.loadingModels')
                    : t('models.form.noModels')
                }
              />
            </Form.Item>
            <Button
              loading={catalogLoading}
              onClick={onFetchCatalog}
              disabled={!watchedApiKey?.trim()}
            >
              {t('models.form.fetchModels')}
            </Button>
          </Space.Compact>
          <Typography.Text type="secondary" style={{ display: 'block', marginTop: 8 }}>
            {modelCatalog
              ? modelCatalog.source === 'live'
                ? t('models.form.catalogLoaded', {
                    baseUrl: modelCatalog.base_url,
                    count: modelCatalog.models.length,
                  })
                : modelCatalog.error || t('models.form.catalogEmpty')
              : t('models.form.catalogHint')}
          </Typography.Text>
        </Form.Item>
        <Space size="middle" style={{ width: '100%' }} align="start">
          <Form.Item name="temperature" label="Temperature" style={{ width: 160 }}>
            <InputNumber min={0} max={2} step={0.1} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="max_output_tokens" label="Max Output Tokens" style={{ width: 180 }}>
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="thinking_level" label="Thinking Level" style={{ flex: 1 }}>
            <Select
              allowClear
              placeholder={t('models.form.thinkingPlaceholder')}
              options={thinkingLevelOptions}
            />
          </Form.Item>
        </Space>
        <Form.Item name="instructions" label="Instructions">
          <Input.TextArea rows={4} />
        </Form.Item>
        <Form.Item name="request_cwd" label="Request CWD">
          <Input />
        </Form.Item>
        <Space size="middle" style={{ width: '100%' }} align="start">
          <Form.Item
            name="request_body_limit_bytes"
            label="Request Body Limit"
            style={{ flex: 1 }}
          >
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item
            name="supports_responses"
            label="Supports Responses"
            valuePropName="checked"
            style={{ marginBottom: 0 }}
          >
            <Switch />
          </Form.Item>
          <Form.Item
            name="include_prompt_cache_retention"
            label="Prompt Cache Retention"
            valuePropName="checked"
            style={{ marginBottom: 0 }}
          >
            <Switch />
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
  );
}

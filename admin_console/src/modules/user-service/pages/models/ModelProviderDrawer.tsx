// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Button, Drawer, Form, Input, Select, Space, Switch } from 'antd';
import type { FormInstance } from 'antd';

import type { UserModelProviderRecord } from '../../types';
import {
  defaultPromptVendor,
  PROMPT_VENDOR_OPTIONS,
  PROVIDER_OPTIONS,
  type ProviderFormValues,
} from './modelPageUtils';

type UserOption = {
  label: string;
  value: string;
};

type ModelProviderDrawerProps = {
  open: boolean;
  editingProvider: UserModelProviderRecord | null;
  isSuperAdmin: boolean;
  userOptions: UserOption[];
  form: FormInstance<ProviderFormValues>;
  saveLoading: boolean;
  onClose: () => void;
  onSubmit: (values: ProviderFormValues) => void;
};

export function ModelProviderDrawer({
  open,
  editingProvider,
  isSuperAdmin,
  userOptions,
  form,
  saveLoading,
  onClose,
  onSubmit,
}: ModelProviderDrawerProps) {
  return (
    <Drawer
      title={editingProvider ? `Edit ${editingProvider.name}` : 'New Provider'}
      open={open}
      width={520}
      onClose={onClose}
      destroyOnClose
      extra={
        <Space>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="primary" loading={saveLoading} onClick={() => form.submit()}>
            Save
          </Button>
        </Space>
      }
    >
      <Form<ProviderFormValues>
        form={form}
        layout="vertical"
        requiredMark={false}
        initialValues={{
          provider: 'gpt',
          prompt_vendor: 'gpt',
          enabled: true,
          supports_images: false,
          supports_reasoning: false,
          supports_responses: true,
          clear_api_key: false,
        }}
        onFinish={onSubmit}
      >
        {isSuperAdmin ? (
          <Form.Item
            name="owner_user_id"
            label="Owner User"
            rules={[{ required: true, message: 'Please choose an owner user' }]}
          >
            <Select options={userOptions} />
          </Form.Item>
        ) : null}
        <Form.Item
          name="name"
          label="Name"
          rules={[{ required: true, message: 'Please enter a name' }]}
        >
          <Input />
        </Form.Item>
        <Form.Item name="provider" label="Provider" rules={[{ required: true }]}>
          <Select
            options={PROVIDER_OPTIONS}
            onChange={(provider) => form.setFieldValue('prompt_vendor', defaultPromptVendor(provider))}
          />
        </Form.Item>
        <Form.Item
          name="prompt_vendor"
          label="Prompt Type"
          tooltip="Selects the optimized system prompt used by cloud agents. Known providers are filled automatically."
          rules={[{ required: true, message: 'Please choose a prompt type' }]}
        >
          <Select options={PROMPT_VENDOR_OPTIONS} />
        </Form.Item>
        <Form.Item name="base_url" label="Base URL">
          <Input placeholder="https://api.openai.com/v1" />
        </Form.Item>
        <Form.Item
          name="api_key"
          label={editingProvider ? 'New API Key' : 'API Key'}
          rules={editingProvider ? undefined : [{ required: true, message: 'Please enter an API key' }]}
        >
          <Input.Password placeholder={editingProvider ? 'Leave empty to keep existing key' : ''} />
        </Form.Item>
        {editingProvider?.has_api_key ? (
          <Form.Item name="clear_api_key" valuePropName="checked">
            <Switch checkedChildren="Clear Saved Key" unCheckedChildren="Keep Saved Key" />
          </Form.Item>
        ) : null}
        <Form.Item name="enabled" label="Enabled" valuePropName="checked">
          <Switch />
        </Form.Item>
        <Form.Item name="supports_images" label="Supports Images" valuePropName="checked">
          <Switch />
        </Form.Item>
        <Form.Item name="supports_reasoning" label="Supports Reasoning" valuePropName="checked">
          <Switch />
        </Form.Item>
        <Form.Item name="supports_responses" label="Supports Responses API" valuePropName="checked">
          <Switch />
        </Form.Item>
      </Form>
    </Drawer>
  );
}

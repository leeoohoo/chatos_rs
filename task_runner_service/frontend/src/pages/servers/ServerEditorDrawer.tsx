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
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  RemoteServerAuthType,
  RemoteServerRecord,
} from '../../types';
import {
  HOST_KEY_POLICY_OPTIONS,
  type RemoteServerFormValues,
} from './serverPageUtils';

type ServerEditorDrawerProps = {
  t: TranslateFn;
  open: boolean;
  editingServer: RemoteServerRecord | null;
  form: FormInstance<RemoteServerFormValues>;
  authType: RemoteServerAuthType;
  authTypeOptions: Array<{ label: string; value: string }>;
  saving: boolean;
  testingDraft: boolean;
  onClose: () => void;
  onDraftTest: () => void;
  onSubmit: (values: RemoteServerFormValues) => void;
};

export function ServerEditorDrawer({
  t,
  open,
  editingServer,
  form,
  authType,
  authTypeOptions,
  saving,
  testingDraft,
  onClose,
  onDraftTest,
  onSubmit,
}: ServerEditorDrawerProps) {
  return (
    <Drawer
      title={editingServer ? t('servers.drawer.edit') : t('servers.drawer.create')}
      open={open}
      width={560}
      destroyOnClose
      onClose={onClose}
      extra={
        <Space>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button loading={testingDraft} onClick={onDraftTest}>
            {t('servers.testDraft')}
          </Button>
          <Button
            type="primary"
            loading={saving}
            onClick={() => form.submit()}
          >
            {t('common.save')}
          </Button>
        </Space>
      }
    >
      <Form<RemoteServerFormValues> layout="vertical" form={form} onFinish={onSubmit}>
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
          <Form.Item
            name="auth_type"
            label={t('servers.form.authType')}
            style={{ width: 220 }}
            rules={[{ required: true }]}
          >
            <Select options={authTypeOptions} />
          </Form.Item>
        </Space>

        {authType === 'password' ? (
          <Form.Item
            name="password"
            label="Password"
            rules={[{ required: true, message: t('servers.form.passwordRequired') }]}
          >
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
          <Form.Item
            name="host_key_policy"
            label="Host Key Policy"
            style={{ flex: 1 }}
            rules={[{ required: true }]}
          >
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
  );
}

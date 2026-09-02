// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Form, Input, Select, Switch } from 'antd';

import { useI18n } from '../../i18n/I18nProvider';

export function CatalogIdentityFields({ isAdmin }: { isAdmin: boolean }) {
  const { t } = useI18n();
  return (
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
    </>
  );
}

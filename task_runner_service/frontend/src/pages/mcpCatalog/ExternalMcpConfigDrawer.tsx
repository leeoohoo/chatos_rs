import type { FormInstance } from 'antd';
import {
  Button,
  Drawer,
  Form,
  Input,
  Select,
  Space,
  Switch,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ExternalMcpConfigRecord, ExternalMcpTransport } from '../../types';
import type { ExternalMcpConfigFormValues } from './mcpCatalogPageUtils';

const { TextArea } = Input;

type ExternalMcpConfigDrawerProps = {
  t: TranslateFn;
  open: boolean;
  editingConfig: ExternalMcpConfigRecord | null;
  form: FormInstance<ExternalMcpConfigFormValues>;
  transport: ExternalMcpTransport;
  saving: boolean;
  onClose: () => void;
  onSubmit: (values: ExternalMcpConfigFormValues) => void;
};

export function ExternalMcpConfigDrawer({
  t,
  open,
  editingConfig,
  form,
  transport,
  saving,
  onClose,
  onSubmit,
}: ExternalMcpConfigDrawerProps) {
  return (
    <Drawer
      title={
        editingConfig
          ? t('mcpCatalog.externalConfigEditTitle')
          : t('mcpCatalog.externalConfigCreateTitle')
      }
      open={open}
      onClose={onClose}
      width={640}
      destroyOnClose
      extra={
        <Space>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button type="primary" loading={saving} onClick={() => form.submit()}>
            {t('common.save')}
          </Button>
        </Space>
      }
    >
      <Form<ExternalMcpConfigFormValues>
        layout="vertical"
        form={form}
        onFinish={onSubmit}
      >
        <Form.Item
          name="name"
          label={t('common.name')}
          rules={[{ required: true, message: t('mcpCatalog.externalConfigNameRequired') }]}
        >
          <Input placeholder="filesystem / jira / internal-search" />
        </Form.Item>

        <Space align="start" style={{ width: '100%' }}>
          <Form.Item
            name="transport"
            label={t('mcpCatalog.externalConfigTransport')}
            rules={[{ required: true }]}
          >
            <Select
              style={{ width: 160 }}
              options={[
                { label: 'stdio', value: 'stdio' },
                { label: 'http', value: 'http' },
              ]}
            />
          </Form.Item>
          <Form.Item name="enabled" label={t('common.status')} valuePropName="checked">
            <Switch checkedChildren={t('common.enabled')} unCheckedChildren={t('common.disabled')} />
          </Form.Item>
        </Space>

        {transport === 'http' ? (
          <>
            <Form.Item
              name="url"
              label="URL"
              rules={[{ required: true, message: t('mcpCatalog.externalConfigUrlRequired') }]}
            >
              <Input placeholder="http://127.0.0.1:3001/mcp" />
            </Form.Item>
            <Form.Item name="headersText" label="Headers JSON">
              <TextArea rows={5} placeholder='{"Authorization": "Bearer ..."}' />
            </Form.Item>
          </>
        ) : (
          <>
            <Form.Item
              name="command"
              label={t('mcpCatalog.externalConfigCommand')}
              rules={[{ required: true, message: t('mcpCatalog.externalConfigCommandRequired') }]}
            >
              <Input placeholder="npx / node / python" />
            </Form.Item>
            <Form.Item name="argsText" label={t('mcpCatalog.externalConfigArgs')}>
              <TextArea
                rows={4}
                placeholder={'-y\n@modelcontextprotocol/server-filesystem\n/Users/me/project'}
              />
            </Form.Item>
            <Form.Item name="cwd" label="cwd">
              <Input placeholder="/Users/me/project" />
            </Form.Item>
            <Form.Item name="envText" label="Env JSON">
              <TextArea rows={5} placeholder='{"TOKEN": "..."}' />
            </Form.Item>
          </>
        )}
      </Form>
    </Drawer>
  );
}

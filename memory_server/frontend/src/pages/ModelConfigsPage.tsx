import { DeleteOutlined, EditOutlined, PlusOutlined } from '@ant-design/icons';
import { useEffect, useMemo, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { AiModelConfig } from '../types';

interface ModelConfigsPageProps {
  userId: string;
}

interface ModelFormValues {
  name: string;
  provider: 'gpt' | 'deepseek' | 'kimik2';
  model: string;
  base_url?: string;
  api_key?: string;
  thinking_level?: 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh';
  temperature?: number;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
}

const THINKING_LEVEL_OPTIONS: Array<{
  label: string;
  value: NonNullable<ModelFormValues['thinking_level']>;
}> = [
  { label: 'none', value: 'none' },
  { label: 'minimal', value: 'minimal' },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
];

const DEFAULT_FORM: ModelFormValues = {
  name: '',
  provider: 'gpt',
  model: '',
  base_url: '',
  api_key: '',
  thinking_level: undefined,
  temperature: undefined,
  enabled: true,
  supports_images: false,
  supports_reasoning: false,
  supports_responses: false,
};

export function ModelConfigsPage({ userId }: ModelConfigsPageProps) {
  const { t } = useI18n();
  const [items, setItems] = useState<AiModelConfig[]>([]);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<AiModelConfig | null>(null);

  const [form] = Form.useForm<ModelFormValues>();
  const provider = Form.useWatch('provider', form);

  const disabled = useMemo(() => !userId.trim(), [userId]);

  const load = async () => {
    if (disabled) {
      setItems([]);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const data = await api.listModelConfigs(userId);
      setItems(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userId]);

  const openCreate = () => {
    setEditing(null);
    form.setFieldsValue(DEFAULT_FORM);
    setModalOpen(true);
  };

  const openEdit = (item: AiModelConfig) => {
    setEditing(item);
    form.setFieldsValue({
      name: item.name,
      provider: (item.provider as 'gpt' | 'deepseek' | 'kimik2') || 'gpt',
      model: item.model,
      base_url: item.base_url || '',
      api_key: item.api_key || '',
      thinking_level: (item.thinking_level as ModelFormValues['thinking_level']) || undefined,
      temperature: item.temperature === null ? undefined : item.temperature || undefined,
      enabled: item.enabled === 1,
      supports_images: item.supports_images === 1,
      supports_reasoning: item.supports_reasoning === 1,
      supports_responses: item.supports_responses === 1,
    });
    setModalOpen(true);
  };

  const normalizeOptionalString = (value?: string) => {
    const trimmed = value?.trim();
    return trimmed ? trimmed : undefined;
  };

  const submit = async () => {
    if (disabled) {
      setError(t('sessions.needUserId'));
      return;
    }

    setError(null);
    setMessage(null);

    try {
      const values = await form.validateFields();
      const providerValue = values.provider || 'gpt';

      const payload = {
        user_id: userId,
        name: values.name.trim(),
        provider: providerValue,
        model: values.model.trim(),
        base_url: normalizeOptionalString(values.base_url),
        api_key: normalizeOptionalString(values.api_key),
        supports_images: Boolean(values.supports_images),
        supports_reasoning: Boolean(values.supports_reasoning),
        supports_responses: Boolean(values.supports_responses),
        temperature: typeof values.temperature === 'number' ? values.temperature : undefined,
        thinking_level:
          providerValue === 'gpt' ? normalizeOptionalString(values.thinking_level) : undefined,
        enabled: Boolean(values.enabled),
      };

      setSubmitting(true);
      if (editing) {
        await api.updateModelConfig(editing.id, payload);
        setMessage(t('models.updateSuccess'));
      } else {
        await api.createModelConfig(payload);
        setMessage(t('models.createSuccess'));
      }

      setModalOpen(false);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (id: string) => {
    setError(null);
    setMessage(null);
    try {
      await api.deleteModelConfig(id);
      setMessage(t('models.deleteSuccess'));
      await load();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const handleTest = async (id: string) => {
    setError(null);
    setMessage(null);
    try {
      const result = await api.testModelConfig(id);
      if (result.ok) {
        setMessage(`${t('models.testOk')}: ${result.output || ''}`);
      } else {
        setError(`${t('models.testFailed')}: ${result.error || ''}`);
      }
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const columns: ColumnsType<AiModelConfig> = [
    { title: t('models.name'), dataIndex: 'name', key: 'name', width: 180 },
    { title: t('models.provider'), dataIndex: 'provider', key: 'provider', width: 120 },
    { title: t('models.model'), dataIndex: 'model', key: 'model', width: 180 },
    {
      title: t('models.baseUrl'),
      dataIndex: 'base_url',
      key: 'base_url',
      ellipsis: true,
      render: (value?: string | null) => value || '-',
    },
    {
      title: t('common.enabled'),
      dataIndex: 'enabled',
      key: 'enabled',
      width: 110,
      render: (value: number) =>
        value === 1 ? <Tag color="green">{t('common.enabled')}</Tag> : <Tag>{t('common.disabled')}</Tag>,
    },
    {
      title: t('models.capabilities'),
      key: 'capabilities',
      width: 260,
      render: (_, item) => (
        <Space size={4} wrap>
          {item.supports_images === 1 && <Tag color="blue">{t('models.supportImages')}</Tag>}
          {item.supports_reasoning === 1 && (
            <Tag color="geekblue">{t('models.supportReasoning')}</Tag>
          )}
          {item.supports_responses === 1 && (
            <Tag color="purple">{t('models.supportResponses')}</Tag>
          )}
          {item.supports_images !== 1 &&
            item.supports_reasoning !== 1 &&
            item.supports_responses !== 1 &&
            '-'}
        </Space>
      ),
    },
    {
      title: t('models.temperature'),
      dataIndex: 'temperature',
      key: 'temperature',
      width: 120,
      render: (value?: number | null) => (typeof value === 'number' ? value : '-'),
    },
    {
      title: t('models.thinking'),
      dataIndex: 'thinking_level',
      key: 'thinking_level',
      width: 120,
      render: (value?: string | null) => value || '-',
    },
    {
      title: t('models.updated'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 190,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    {
      title: t('common.action'),
      key: 'action',
      width: 220,
      fixed: 'right',
      render: (_, item) => (
        <Space>
          <Button size="small" onClick={() => handleTest(item.id)}>
            {t('common.test')}
          </Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(item)}>
            {t('common.edit')}
          </Button>
          <Popconfirm
            title={t('models.deleteConfirm')}
            onConfirm={() => handleDelete(item.id)}
            okText={t('common.confirm')}
            cancelText={t('common.cancel')}
          >
            <Button size="small" danger icon={<DeleteOutlined />}>
              {t('common.delete')}
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <Card
      title={t('models.title')}
      extra={
        <Space>
          <Button onClick={load} loading={loading}>
            {t('common.refresh')}
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate} disabled={disabled}>
            {t('models.add')}
          </Button>
        </Space>
      }
    >
      {!userId.trim() && (
        <Alert type="warning" showIcon message={t('sessions.needUserId')} style={{ marginBottom: 12 }} />
      )}
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      {message && <Alert type="success" showIcon message={message} style={{ marginBottom: 12 }} />}

      <Typography.Paragraph type="secondary" style={{ marginBottom: 12 }}>
        {t('models.required')}
      </Typography.Paragraph>

      <Table
        rowKey="id"
        loading={loading}
        columns={columns}
        dataSource={items}
        pagination={{ pageSize: 12 }}
        scroll={{ x: 1450 }}
        size="small"
      />

      <Modal
        title={editing ? t('models.edit') : t('models.add')}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={submit}
        confirmLoading={submitting}
        okText={t('common.save')}
        cancelText={t('common.cancel')}
        destroyOnClose
      >
        <Form form={form} layout="vertical" initialValues={DEFAULT_FORM}>
          <Form.Item
            label={t('models.name')}
            name="name"
            rules={[{ required: true, message: t('models.required') }]}
          >
            <Input />
          </Form.Item>

          <Form.Item
            label={t('models.provider')}
            name="provider"
            rules={[{ required: true, message: t('models.required') }]}
          >
            <Select
              options={[
                { label: 'gpt', value: 'gpt' },
                { label: 'deepseek', value: 'deepseek' },
                { label: 'kimik2', value: 'kimik2' },
              ]}
            />
          </Form.Item>

          <Form.Item
            label={t('models.model')}
            name="model"
            rules={[{ required: true, message: t('models.required') }]}
          >
            <Input />
          </Form.Item>

          <Form.Item label={t('models.baseUrl')} name="base_url">
            <Input />
          </Form.Item>

          <Form.Item label={t('models.apiKey')} name="api_key">
            <Input.Password autoComplete="off" />
          </Form.Item>

          <Form.Item label={t('models.thinking')} name="thinking_level">
            <Select
              allowClear
              disabled={provider !== 'gpt'}
              options={THINKING_LEVEL_OPTIONS}
            />
          </Form.Item>

          <Form.Item label={t('models.temperature')} name="temperature">
            <InputNumber min={0} max={2} step={0.1} style={{ width: '100%' }} />
          </Form.Item>

          <Form.Item label={t('common.enabled')} name="enabled" valuePropName="checked">
            <Switch />
          </Form.Item>

          <Form.Item
            label={t('models.supportImages')}
            name="supports_images"
            valuePropName="checked"
          >
            <Switch />
          </Form.Item>

          <Form.Item
            label={t('models.supportReasoning')}
            name="supports_reasoning"
            valuePropName="checked"
          >
            <Switch />
          </Form.Item>

          <Form.Item
            label={t('models.supportResponses')}
            name="supports_responses"
            valuePropName="checked"
          >
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </Card>
  );
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect } from 'react';
import { ReloadOutlined, SaveOutlined } from '@ant-design/icons';
import { App, Button, Descriptions, Form, InputNumber, Progress, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { sandboxesApi } from '../api/sandboxes';
import { useI18n } from '../i18n';

interface PoolConfigFormValues {
  max_active: number;
  max_pending: number;
}

export function PoolPage() {
  const { t } = useI18n();
  const { message } = App.useApp();
  const [form] = Form.useForm<PoolConfigFormValues>();
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: ['pool-status'],
    queryFn: sandboxesApi.poolStatus,
  });
  const data = query.data;
  const percent = data && data.max_active > 0 ? Math.round((data.active / data.max_active) * 100) : 0;
  const updateMutation = useMutation({
    mutationFn: sandboxesApi.updatePoolConfig,
    onSuccess: (updated) => {
      queryClient.setQueryData(['pool-status'], updated);
      void queryClient.invalidateQueries({ queryKey: ['system-config'] });
      message.success(t('pool.updateSuccess'));
    },
  });

  useEffect(() => {
    if (!data) {
      return;
    }
    form.setFieldsValue({
      max_active: data.max_active,
      max_pending: data.max_pending,
    });
  }, [data, form]);

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('pool.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pool.subtitle')}</Typography.Text>
        </div>
        <Button icon={<ReloadOutlined />} onClick={() => void query.refetch()}>
          {t('common.refresh')}
        </Button>
      </div>

      <div className="surface">
        <Progress percent={percent} status={percent >= 100 ? 'exception' : 'active'} />
        <Descriptions bordered column={2} style={{ marginTop: 18 }}>
          <Descriptions.Item label={t('common.backend')}>{data?.backend ?? '-'}</Descriptions.Item>
          <Descriptions.Item label={t('pool.active')}>
            {data ? `${data.active} / ${data.max_active}` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.pending')}>
            {data ? `${data.pending} / ${data.max_pending}` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.leaseTtl')}>
            {data ? `${data.lease_ttl_seconds}s` : '-'}
          </Descriptions.Item>
          <Descriptions.Item label={t('pool.cleanupInterval')}>
            {data ? `${data.cleanup_interval_seconds}s` : '-'}
          </Descriptions.Item>
        </Descriptions>
      </div>

      <div className="surface">
        <Typography.Title level={5}>{t('pool.configTitle')}</Typography.Title>
        <Form
          form={form}
          layout="inline"
          onFinish={(values) => updateMutation.mutate(values)}
          disabled={!data}
          style={{ rowGap: 12 }}
        >
          <Form.Item
            name="max_active"
            label={t('settings.poolActiveLimit')}
            rules={[{ required: true, type: 'number', min: 1 }]}
          >
            <InputNumber min={1} precision={0} />
          </Form.Item>
          <Form.Item
            name="max_pending"
            label={t('settings.poolPendingLimit')}
            rules={[{ required: true, type: 'number', min: 0 }]}
          >
            <InputNumber min={0} precision={0} />
          </Form.Item>
          <Form.Item>
            <Button
              type="primary"
              htmlType="submit"
              icon={<SaveOutlined />}
              loading={updateMutation.isPending}
            >
              {t('common.save')}
            </Button>
          </Form.Item>
        </Form>
      </div>
    </Space>
  );
}

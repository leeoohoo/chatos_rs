// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PlusOutlined } from '@ant-design/icons';
import { App, Button, Form, Input, InputNumber, Select, Space, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';

import { sandboxesApi } from '../api/sandboxes';
import { useI18n } from '../i18n';
import type { CreateSandboxLeasePayload } from '../types';

export function CreateSandboxPage() {
  const { t } = useI18n();
  const [form] = Form.useForm<CreateSandboxLeasePayload & { cpu: number; memory_mb: number; disk_mb: number; max_processes: number; network_mode: string; ttl_seconds: number }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message } = App.useApp();
  const imagesQuery = useQuery({
    queryKey: ['sandbox-images'],
    queryFn: sandboxesApi.images,
  });

  const mutation = useMutation({
    mutationFn: (values: CreateSandboxLeasePayload) => sandboxesApi.create(values),
    onSuccess: async (response) => {
      message.success(t('create.success'));
      await queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
      await queryClient.invalidateQueries({ queryKey: ['pool-status'] });
      navigate(`/sandboxes/${response.sandbox_id}`);
    },
    onError: (error) => message.error(error instanceof Error ? error.message : t('create.failure')),
  });

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('create.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('create.subtitle')}</Typography.Text>
        </div>
      </div>

      <div className="surface form-surface">
        <Form
          form={form}
          layout="vertical"
          initialValues={{
            tenant_id: 'tenant-dev',
            user_id: 'user-dev',
            project_id: 'project-dev',
            run_id: `run-${Date.now()}`,
            workspace_root: '/tmp/chatos-sandbox-demo',
            image_id: imagesQuery.data?.default_image_id ?? 'default',
            tools: ['filesystem', 'terminal'],
            ttl_seconds: 3600,
            cpu: 2,
            memory_mb: 4096,
            disk_mb: 10240,
            max_processes: 128,
            network_mode: 'bridge',
          }}
          onFinish={(values) => {
            mutation.mutate({
              tenant_id: values.tenant_id,
              user_id: values.user_id,
              project_id: values.project_id,
              run_id: values.run_id,
              workspace_root: values.workspace_root,
              image_id: values.image_id,
              tools: values.tools,
              ttl_seconds: values.ttl_seconds,
              resource_limits: {
                cpu: values.cpu,
                memory_mb: values.memory_mb,
                disk_mb: values.disk_mb,
                max_processes: values.max_processes,
              },
              network: { mode: values.network_mode },
            });
          }}
        >
          <Form.Item label={t('create.tenantId')} name="tenant_id" rules={[{ required: true, message: t('form.required') }]}>
            <Input />
          </Form.Item>
          <Form.Item label={t('create.userId')} name="user_id" rules={[{ required: true, message: t('form.required') }]}>
            <Input />
          </Form.Item>
          <Form.Item label={t('create.projectId')} name="project_id" rules={[{ required: true, message: t('form.required') }]}>
            <Input />
          </Form.Item>
          <Form.Item label={t('create.runId')} name="run_id" rules={[{ required: true, message: t('form.required') }]}>
            <Input />
          </Form.Item>
          <Form.Item label={t('create.workspaceRoot')} name="workspace_root" rules={[{ required: true, message: t('form.required') }]}>
            <Input />
          </Form.Item>
          <Form.Item label={t('common.image')} name="image_id">
            <Select
              loading={imagesQuery.isLoading}
              options={(imagesQuery.data?.images ?? [
                {
                  id: 'default',
                  name: t('image.default'),
                  image_ref: '',
                  initialized: true,
                  buildable: false,
                },
              ]).map((image) => ({
                label: `${image.name}${image.image_ref ? ` · ${image.image_ref}` : ''}${
                  image.buildable && !image.initialized ? ` · ${t('image.missing')}` : ''
                }`,
                value: image.id,
                disabled: image.buildable && !image.initialized,
              }))}
            />
          </Form.Item>
          <Form.Item label={t('create.tools')} name="tools">
            <Select
              mode="multiple"
              options={[
                { label: t('tool.filesystem'), value: 'filesystem' },
                { label: t('tool.terminal'), value: 'terminal' },
              ]}
            />
          </Form.Item>
          <Form.Item label={t('create.ttlSeconds')} name="ttl_seconds">
            <InputNumber min={60} max={86400} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item label={t('create.cpu')} name="cpu">
            <InputNumber min={0.1} max={32} step={0.5} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item label={t('create.memoryMb')} name="memory_mb">
            <InputNumber min={128} max={262144} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item label={t('create.diskMb')} name="disk_mb">
            <InputNumber min={128} max={1048576} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item label={t('create.maxProcesses')} name="max_processes">
            <InputNumber min={1} max={4096} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item label={t('create.network')} name="network_mode">
            <Select
              options={[
                { label: 'none', value: 'none' },
                { label: 'bridge', value: 'bridge' },
              ]}
            />
          </Form.Item>
          <Button type="primary" htmlType="submit" icon={<PlusOutlined />} loading={mutation.isPending}>
            {t('create.submit')}
          </Button>
        </Form>
      </div>
    </Space>
  );
}

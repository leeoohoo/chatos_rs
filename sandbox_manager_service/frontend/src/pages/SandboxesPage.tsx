import { ReloadOutlined } from '@ant-design/icons';
import { Button, Form, Input, Select, Space, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import dayjs from 'dayjs';
import { Link } from 'react-router-dom';

import { sandboxesApi, type SandboxListFilters } from '../api/sandboxes';
import { SandboxActions } from '../components/SandboxActions';
import { StatusTag } from '../components/StatusTag';
import { useI18n } from '../i18n';
import type { SandboxLeaseRecord, SandboxStatus } from '../types';

const statusOptions: SandboxStatus[] = [
  'pending',
  'leasing',
  'starting',
  'ready',
  'running',
  'releasing',
  'destroying',
  'destroyed',
  'failed',
  'expired',
];

export function SandboxesPage() {
  const { t } = useI18n();
  const [form] = Form.useForm<SandboxListFilters>();
  const filters = Form.useWatch([], form) ?? {};
  const query = useQuery({
    queryKey: ['sandboxes', filters],
    queryFn: () => sandboxesApi.list(filters),
  });

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('sandboxes.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('sandboxes.subtitle')}</Typography.Text>
        </div>
        <Button icon={<ReloadOutlined />} onClick={() => void query.refetch()}>
          {t('common.refresh')}
        </Button>
      </div>

      <div className="surface">
        <Form form={form} layout="inline" className="filter-bar">
          <Form.Item name="status" label={t('common.status')}>
            <Select
              allowClear
              style={{ width: 150 }}
              options={statusOptions.map((status) => ({
                label: t(`status.${status}`),
                value: status,
              }))}
            />
          </Form.Item>
          <Form.Item name="tenant_id" label={t('common.tenant')}>
            <Input allowClear style={{ width: 180 }} />
          </Form.Item>
          <Form.Item name="project_id" label={t('common.project')}>
            <Input allowClear style={{ width: 180 }} />
          </Form.Item>
          <Form.Item name="run_id" label={t('common.run')}>
            <Input allowClear style={{ width: 180 }} />
          </Form.Item>
        </Form>

        <Table<SandboxLeaseRecord>
          rowKey="sandbox_id"
          loading={query.isLoading}
          dataSource={query.data ?? []}
          pagination={{ pageSize: 12 }}
          scroll={{ x: 1100 }}
          columns={[
            {
              title: t('common.status'),
              dataIndex: 'status',
              width: 120,
              render: (status) => <StatusTag status={status} />,
            },
            {
              title: t('common.sandbox'),
              dataIndex: 'sandbox_id',
              width: 260,
              render: (id) => <Link to={`/sandboxes/${id}`}>{id}</Link>,
            },
            { title: t('common.tenant'), dataIndex: 'tenant_id', width: 150 },
            { title: t('common.user'), dataIndex: 'user_id', width: 150 },
            { title: t('common.project'), dataIndex: 'project_id', width: 160 },
            { title: t('common.run'), dataIndex: 'run_id', width: 160 },
            { title: t('common.backend'), dataIndex: 'backend', width: 100 },
            {
              title: t('common.expiresAt'),
              dataIndex: 'expires_at',
              width: 160,
              render: (value) => dayjs(value).format('MM-DD HH:mm:ss'),
            },
            {
              title: t('common.actions'),
              fixed: 'right',
              width: 180,
              render: (_, record) => <SandboxActions sandbox={record} />,
            },
          ]}
        />
      </div>
    </Space>
  );
}

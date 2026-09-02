// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EyeOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { Alert, Button, Form, Input, Modal, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginAuditLogRecord } from '../pluginTypes';
import type { CurrentUser } from '../types';
import { optionalText } from './formUtils';

type AuditFilters = {
  event?: string;
  plugin_id?: string;
  owner_user_id?: string;
  device_id?: string;
};

export function PluginAuditPage({ user }: { user: CurrentUser }) {
  const { t } = useI18n();
  const [form] = Form.useForm<AuditFilters>();
  const [filters, setFilters] = useState<AuditFilters>({});
  const [detail, setDetail] = useState<PluginAuditLogRecord | null>(null);
  const isAdmin = user.role === 'super_admin';
  const auditQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-audit', filters],
    queryFn: () => api.listPluginAudit({ ...filters, limit: 100 }),
    enabled: isAdmin,
  });
  const columns = useMemo<ColumnsType<PluginAuditLogRecord>>(
    () => [
      {
        title: t('pluginAudit.event'),
        dataIndex: 'event',
        width: 190,
        render: (value) => <Tag color="blue">{value}</Tag>,
      },
      {
        title: t('pluginAudit.outcome'),
        dataIndex: 'outcome',
        width: 110,
        render: (value) => <Tag color={value === 'success' ? 'green' : 'red'}>{value}</Tag>,
      },
      {
        title: t('pluginAudit.pluginRelease'),
        key: 'plugin',
        width: 230,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <CompactId value={record.plugin_id} />
            <CompactId value={record.release_id} />
          </Space>
        ),
      },
      {
        title: t('pluginAudit.actor'),
        dataIndex: 'owner_user_id',
        width: 170,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('pluginAudit.deviceComponent'),
        key: 'device',
        width: 220,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <CompactId value={record.device_id} />
            <CompactId value={record.component_key} />
          </Space>
        ),
      },
      {
        title: t('pluginAudit.createdAt'),
        dataIndex: 'created_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 110,
        fixed: 'right',
        render: (_, record) => (
          <Button size="small" icon={<EyeOutlined />} onClick={() => setDetail(record)}>
            {t('pluginAudit.details')}
          </Button>
        ),
      },
    ],
    [t],
  );

  if (!isAdmin) {
    return <Alert type="error" showIcon message={t('admin.only')} />;
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginAudit.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginAudit.description')}</Typography.Text>
        </Space>
        <Button icon={<ReloadOutlined />} onClick={() => auditQuery.refetch()}>
          {t('pluginAudit.refresh')}
        </Button>
      </div>
      <Form
        form={form}
        layout="inline"
        onFinish={(values) => {
          setFilters({
            event: optionalText(values.event),
            plugin_id: optionalText(values.plugin_id),
            owner_user_id: optionalText(values.owner_user_id),
            device_id: optionalText(values.device_id),
          });
        }}
      >
        <Form.Item name="event">
          <Input allowClear placeholder={t('pluginAudit.event')} />
        </Form.Item>
        <Form.Item name="plugin_id">
          <Input allowClear placeholder={t('pluginAudit.pluginId')} />
        </Form.Item>
        <Form.Item name="owner_user_id">
          <Input allowClear placeholder={t('pluginAudit.ownerUserId')} />
        </Form.Item>
        <Form.Item name="device_id">
          <Input allowClear placeholder={t('pluginAudit.deviceId')} />
        </Form.Item>
        <Form.Item>
          <Space>
            <Button type="primary" htmlType="submit" icon={<SearchOutlined />}>
              {t('pluginAudit.filter')}
            </Button>
            <Button
              onClick={() => {
                form.resetFields();
                setFilters({});
              }}
            >
              {t('pluginAudit.clear')}
            </Button>
          </Space>
        </Form.Item>
      </Form>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={auditQuery.data?.items || []}
        loading={auditQuery.isLoading || auditQuery.isFetching}
        scroll={{ x: 1210 }}
        pagination={{
          pageSize: 100,
          total: auditQuery.data?.total || 0,
          showSizeChanger: false,
        }}
      />
      <Modal
        title={t('pluginAudit.details')}
        open={Boolean(detail)}
        onCancel={() => setDetail(null)}
        footer={null}
        width={820}
        destroyOnClose
      >
        <Input.TextArea
          className="code-input"
          rows={18}
          readOnly
          value={detail ? JSON.stringify(detail, null, 2) : ''}
        />
      </Modal>
    </div>
  );
}

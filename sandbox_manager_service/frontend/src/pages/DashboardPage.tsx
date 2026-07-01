// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ReloadOutlined } from '@ant-design/icons';
import { Button, Col, Row, Space, Statistic, Table, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';
import dayjs from 'dayjs';
import { Link } from 'react-router-dom';

import { sandboxesApi } from '../api/sandboxes';
import { StatusTag } from '../components/StatusTag';
import { useI18n } from '../i18n';
import type { SandboxLeaseRecord } from '../types';

export function DashboardPage() {
  const { t } = useI18n();
  const sandboxesQuery = useQuery({
    queryKey: ['sandboxes', 'dashboard'],
    queryFn: () => sandboxesApi.list(),
  });
  const poolQuery = useQuery({
    queryKey: ['pool-status'],
    queryFn: sandboxesApi.poolStatus,
  });

  const sandboxes = sandboxesQuery.data ?? [];
  const active = sandboxes.filter((item) => !['destroyed', 'failed', 'expired'].includes(item.status));
  const failed = sandboxes.filter((item) => ['failed', 'expired'].includes(item.status));

  return (
    <Space direction="vertical" size={18} style={{ width: '100%' }}>
      <div className="page-heading">
        <div>
          <Typography.Title level={3}>{t('dashboard.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('dashboard.subtitle')}</Typography.Text>
        </div>
        <Button
          icon={<ReloadOutlined />}
          onClick={() => {
            void sandboxesQuery.refetch();
            void poolQuery.refetch();
          }}
        >
          {t('common.refresh')}
        </Button>
      </div>

      <Row gutter={12}>
        <Col xs={24} md={6}>
          <div className="metric-panel">
            <Statistic title={t('dashboard.active')} value={active.length} />
          </div>
        </Col>
        <Col xs={24} md={6}>
          <div className="metric-panel">
            <Statistic title={t('dashboard.failedExpired')} value={failed.length} />
          </div>
        </Col>
        <Col xs={24} md={6}>
          <div className="metric-panel">
            <Statistic title={t('dashboard.poolActive')} value={poolQuery.data?.active ?? 0} />
          </div>
        </Col>
        <Col xs={24} md={6}>
          <div className="metric-panel">
            <Statistic title={t('dashboard.poolCapacity')} value={poolQuery.data?.max_active ?? 0} />
          </div>
        </Col>
      </Row>

      <div className="surface">
        <Typography.Title level={4}>{t('dashboard.recentSandboxes')}</Typography.Title>
        <Table<SandboxLeaseRecord>
          size="middle"
          rowKey="sandbox_id"
          loading={sandboxesQuery.isLoading}
          dataSource={sandboxes.slice(0, 8)}
          pagination={false}
          columns={[
            {
              title: t('common.status'),
              dataIndex: 'status',
              render: (status) => <StatusTag status={status} />,
              width: 120,
            },
            {
              title: t('common.sandbox'),
              dataIndex: 'sandbox_id',
              render: (id) => <Link to={`/sandboxes/${id}`}>{id}</Link>,
            },
            { title: t('common.project'), dataIndex: 'project_id' },
            { title: t('common.run'), dataIndex: 'run_id' },
            { title: t('common.backend'), dataIndex: 'backend', width: 110 },
            {
              title: t('common.createdAt'),
              dataIndex: 'created_at',
              render: (value) => dayjs(value).format('MM-DD HH:mm:ss'),
              width: 150,
            },
          ]}
        />
      </div>
    </Space>
  );
}

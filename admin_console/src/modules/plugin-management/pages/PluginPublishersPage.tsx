// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { CheckOutlined, PlusOutlined, StopOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Form, Input, Modal, Select, Space, Table, Tag, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginMarketplaceRecord, PluginPublisherRecord } from '../pluginTypes';
import type { CurrentUser } from '../types';
import { optionalText } from './formUtils';

type ReviewDecision = 'approve' | 'reject' | 'suspend';

export function PluginPublishersPage({ user }: { user: CurrentUser }) {
  const { t } = useI18n();
  const isAdmin = user.role === 'super_admin';
  const queryClient = useQueryClient();
  const [applicationForm] = Form.useForm();
  const [reviewForm] = Form.useForm();
  const [applicationOpen, setApplicationOpen] = useState(false);
  const [reviewTarget, setReviewTarget] = useState<PluginPublisherRecord | null>(null);
  const [reviewDecision, setReviewDecision] = useState<ReviewDecision>('approve');
  const publishersQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-publishers', isAdmin ? 'admin' : 'self'],
    queryFn: () => isAdmin ? api.listAdminPluginPublishers() : api.listPluginPublishers(),
  });
  const marketplacesQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-marketplaces'],
    queryFn: api.listPluginMarketplaces,
  });
  const eligibleMarketplaces = useMemo(
    () => (marketplacesQuery.data?.items || []).filter(isEligibleOnboardingMarketplace),
    [marketplacesQuery.data?.items],
  );
  const submitMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) =>
      api.submitPluginPublisher({
        publisher_id: values.publisher_id,
        marketplace_id: values.marketplace_id,
        name: values.name,
        website: optionalText(values.website),
      }),
    onSuccess: () => {
      message.success(t('pluginPublisher.submitted'));
      setApplicationOpen(false);
      applicationForm.resetFields();
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-publishers'] });
    },
    onError: (error) => message.error((error as Error).message),
  });
  const reviewMutation = useMutation({
    mutationFn: ({ recordId, values }: { recordId: string; values: Record<string, unknown> }) =>
      api.reviewAdminPluginPublisher(recordId, {
        decision: reviewDecision,
        review_note: optionalText(values.review_note),
      }),
    onSuccess: () => {
      message.success(t('pluginPublisher.reviewed'));
      setReviewTarget(null);
      reviewForm.resetFields();
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-publishers'] });
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-marketplaces'] });
    },
    onError: (error) => message.error((error as Error).message),
  });

  const openApplication = (record?: PluginPublisherRecord) => {
    applicationForm.setFieldsValue({
      publisher_id: record?.publisher_id || '',
      marketplace_id: record?.marketplace_id || eligibleMarketplaces[0]?.id,
      name: record?.name || '',
      website: record?.website || '',
    });
    setApplicationOpen(true);
  };
  const openReview = (record: PluginPublisherRecord, decision: ReviewDecision) => {
    setReviewTarget(record);
    setReviewDecision(decision);
    reviewForm.setFieldsValue({ review_note: '' });
  };

  const columns = useMemo<ColumnsType<PluginPublisherRecord>>(
    () => [
      {
        title: t('pluginPublisher.identity'),
        key: 'identity',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text strong>{record.name}</Typography.Text>
            <CompactId value={record.publisher_id} />
            {record.website ? (
              <Typography.Link href={record.website} target="_blank" rel="noreferrer">
                {record.website}
              </Typography.Link>
            ) : null}
          </Space>
        ),
      },
      {
        title: t('pluginPublisher.marketplace'),
        dataIndex: 'marketplace_id',
        width: 180,
        render: (value) => <CompactId value={value} />,
      },
      ...(isAdmin
        ? [{
            title: t('pluginPublisher.owner'),
            dataIndex: 'owner_user_id',
            width: 170,
            render: (value: string) => <CompactId value={value} />,
          }]
        : []),
      {
        title: t('table.status'),
        dataIndex: 'status',
        width: 120,
        render: (status) => (
          <Tag color={publisherStatusColor(status)}>{t(`pluginPublisher.status.${status}`)}</Tag>
        ),
      },
      {
        title: t('pluginPublisher.keys'),
        dataIndex: 'signing_keys',
        width: 90,
        render: (keys) => keys?.length || 0,
      },
      {
        title: t('pluginPublisher.submittedAt'),
        dataIndex: 'submitted_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('pluginPublisher.review'),
        key: 'review',
        width: 220,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <DateTimeCell value={record.reviewed_at} />
            {record.review_note ? (
              <Typography.Text type="secondary" ellipsis={{ tooltip: record.review_note }}>
                {record.review_note}
              </Typography.Text>
            ) : null}
          </Space>
        ),
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 230,
        fixed: 'right',
        render: (_, record) => {
          if (!isAdmin) {
            return record.status === 'rejected' ? (
              <Button size="small" onClick={() => openApplication(record)}>
                {t('pluginPublisher.resubmit')}
              </Button>
            ) : null;
          }
          if (record.status === 'pending') {
            return (
              <Space size="small">
                <Button
                  size="small"
                  type="primary"
                  icon={<CheckOutlined />}
                  onClick={() => openReview(record, 'approve')}
                >
                  {t('pluginPublisher.approve')}
                </Button>
                <Button size="small" danger onClick={() => openReview(record, 'reject')}>
                  {t('pluginPublisher.reject')}
                </Button>
              </Space>
            );
          }
          if (record.status === 'approved') {
            return (
              <Button
                size="small"
                danger
                icon={<StopOutlined />}
                onClick={() => openReview(record, 'suspend')}
              >
                {t('pluginPublisher.suspend')}
              </Button>
            );
          }
          if (record.status === 'suspended') {
            return (
              <Button
                size="small"
                type="primary"
                icon={<CheckOutlined />}
                onClick={() => openReview(record, 'approve')}
              >
                {t('pluginPublisher.restore')}
              </Button>
            );
          }
          return null;
        },
      },
    ],
    [eligibleMarketplaces, isAdmin, t],
  );

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginPublisher.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginPublisher.description')}</Typography.Text>
        </Space>
        {!isAdmin ? (
          <Button
            type="primary"
            icon={<PlusOutlined />}
            disabled={eligibleMarketplaces.length === 0}
            onClick={() => openApplication()}
          >
            {t('pluginPublisher.apply')}
          </Button>
        ) : null}
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={publishersQuery.data?.items || []}
        loading={publishersQuery.isLoading}
        scroll={{ x: 1380 }}
        pagination={false}
      />

      <Modal
        title={t('pluginPublisher.applyTitle')}
        open={applicationOpen}
        onCancel={() => {
          setApplicationOpen(false);
          applicationForm.resetFields();
        }}
        onOk={() => applicationForm.submit()}
        confirmLoading={submitMutation.isPending}
        width={760}
        destroyOnClose
      >
        <Form form={applicationForm} layout="vertical" onFinish={(values) => submitMutation.mutate(values)}>
          <div className="form-grid">
            <Form.Item name="publisher_id" label={t('pluginPublisher.id')} rules={[{ required: true }]}>
              <Input placeholder="acme-tools" />
            </Form.Item>
            <Form.Item name="marketplace_id" label={t('pluginPublisher.marketplace')} rules={[{ required: true }]}>
              <Select
                options={eligibleMarketplaces.map((marketplace) => ({
                  value: marketplace.id,
                  label: `${marketplace.name} | ${marketplace.id}`,
                }))}
              />
            </Form.Item>
            <Form.Item name="name" label={t('table.name')} rules={[{ required: true }]}>
              <Input placeholder="Acme Tools" />
            </Form.Item>
            <Form.Item name="website" label={t('pluginPublisher.website')}>
              <Input placeholder="https://example.com" />
            </Form.Item>
          </div>
          <Alert type="info" showIcon message={t('pluginPublisher.managedSigningNotice')} />
        </Form>
      </Modal>

      <Modal
        title={t(`pluginPublisher.reviewTitle.${reviewDecision}`)}
        open={Boolean(reviewTarget)}
        onCancel={() => {
          setReviewTarget(null);
          reviewForm.resetFields();
        }}
        onOk={() => reviewForm.submit()}
        confirmLoading={reviewMutation.isPending}
        destroyOnClose
      >
        <Form
          form={reviewForm}
          layout="vertical"
          onFinish={(values) => {
            if (reviewTarget) {
              reviewMutation.mutate({ recordId: reviewTarget.id, values });
            }
          }}
        >
          <Typography.Paragraph>
            {reviewTarget?.name} · {reviewTarget?.publisher_id}
          </Typography.Paragraph>
          <Form.Item
            name="review_note"
            label={t('pluginPublisher.reviewNote')}
            rules={reviewDecision === 'approve' ? undefined : [{ required: true }]}
          >
            <Input.TextArea rows={5} maxLength={2000} showCount />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}

function isEligibleOnboardingMarketplace(marketplace: PluginMarketplaceRecord): boolean {
  return marketplace.enabled
    && marketplace.source_kind === 'admin_registry'
    && marketplace.trust_level === 'trusted'
    && marketplace.visibility === 'public'
    && !marketplace.owner_user_id;
}

function publisherStatusColor(status: PluginPublisherRecord['status']): string {
  if (status === 'approved') return 'green';
  if (status === 'pending') return 'blue';
  if (status === 'suspended') return 'orange';
  return 'red';
}

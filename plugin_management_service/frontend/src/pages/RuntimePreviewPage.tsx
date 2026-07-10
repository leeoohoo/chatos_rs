// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PlayCircleOutlined } from '@ant-design/icons';
import { useMutation, useQuery } from '@tanstack/react-query';
import { Button, Form, Input, Select, Space, Table, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { RuntimeKindTag, StatusTag, VisibilityTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import {
  agentDisplayName,
  bindingScopeLabel,
  contentKindLabel,
  mcpDisplayName,
  resourceKindLabel,
} from '../i18n/labels';
import type { CurrentUser, LocalConnectorRequirement, ResolvedMcp, ResolvedSkill } from '../types';
import { optionalText } from './formUtils';

interface RuntimePreviewPageProps {
  user: CurrentUser;
}

export function RuntimePreviewPage({ user }: RuntimePreviewPageProps) {
  const { t } = useI18n();
  const [form] = Form.useForm();
  const agentsQuery = useQuery({
    queryKey: ['system-agents', 'runtime-preview'],
    queryFn: api.listSystemAgents,
  });

  const resolveMutation = useMutation({
    mutationFn: (values: Record<string, unknown>) =>
      api.resolveAgentCapabilities({
        agent_key: values.agent_key as string,
        owner_user_id: optionalText(values.owner_user_id),
        include_unavailable: true,
      }),
    onError: (error) => message.error((error as Error).message),
  });

  const mcpColumns: ColumnsType<ResolvedMcp> = [
    {
      title: t('resource.mcp'),
      dataIndex: ['resource', 'display_name'],
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{mcpDisplayName(record.resource, t)}</Typography.Text>
          <Typography.Text type="secondary">{record.resource.id}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('table.visibility'),
      dataIndex: ['resource', 'visibility'],
      width: 120,
      render: (value) => <VisibilityTag value={value} />,
    },
    {
      title: t('table.runtime'),
      dataIndex: ['resource', 'runtime', 'kind'],
      width: 210,
      render: (value, record) => (
        <Space direction="vertical" size={0}>
          <RuntimeKindTag value={value} />
          <Typography.Text type="secondary">{record.resource.runtime.builtin_kind}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('agent.binding'),
      dataIndex: ['binding', 'binding_scope'],
      width: 160,
      render: (value) => bindingScopeLabel(value, t),
    },
    {
      title: t('table.status'),
      dataIndex: 'status',
      width: 120,
      render: (value) => <StatusTag status={value} />,
    },
    {
      title: t('table.reason'),
      dataIndex: 'reason',
      width: 260,
    },
  ];

  const skillColumns: ColumnsType<ResolvedSkill> = [
    {
      title: t('resource.skill'),
      dataIndex: ['resource', 'display_name'],
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{record.resource.display_name}</Typography.Text>
          <Typography.Text type="secondary">{record.resource.id}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('table.visibility'),
      dataIndex: ['resource', 'visibility'],
      width: 120,
      render: (value) => <VisibilityTag value={value} />,
    },
    {
      title: t('table.content'),
      dataIndex: ['resource', 'content', 'kind'],
      width: 190,
      render: (value) => contentKindLabel(value, t),
    },
    {
      title: t('agent.binding'),
      dataIndex: ['binding', 'binding_scope'],
      width: 160,
      render: (value) => bindingScopeLabel(value, t),
    },
    {
      title: t('table.status'),
      dataIndex: 'status',
      width: 120,
      render: (value) => <StatusTag status={value} />,
    },
    {
      title: t('table.reason'),
      dataIndex: 'reason',
      width: 260,
    },
  ];

  const localColumns: ColumnsType<LocalConnectorRequirement> = [
    {
      title: t('table.resourceType'),
      dataIndex: 'resource_kind',
      width: 120,
      render: (value) => resourceKindLabel(value, t),
    },
    { title: t('table.resourceId'), dataIndex: 'resource_id' },
    { title: t('runtime.device'), dataIndex: 'device_id', width: 180 },
    { title: t('runtime.workspace'), dataIndex: 'workspace_id', width: 180 },
    {
      title: t('field.required'),
      dataIndex: 'required',
      width: 100,
      render: (required) => t(required ? 'common.yes' : 'common.no'),
    },
    {
      title: t('table.status'),
      dataIndex: 'available',
      width: 120,
      render: (available) => <StatusTag status={available ? 'available' : 'unknown'} />,
    },
    { title: t('table.reason'), dataIndex: 'reason', width: 260 },
  ];

  const data = resolveMutation.data;

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('runtime.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('runtime.description')}</Typography.Text>
        </Space>
      </div>
      <div className="preview-panel">
        <Form
          form={form}
          layout="vertical"
          initialValues={{ owner_user_id: user.user_id }}
          onFinish={(values) => resolveMutation.mutate(values)}
        >
          <div className="form-grid">
            <Form.Item name="agent_key" label={t('agent.title')} rules={[{ required: true }]}>
              <Select
                showSearch
                optionFilterProp="label"
                options={(agentsQuery.data || []).map((agent) => ({
                  value: agent.agent_key,
                  label: `${agentDisplayName(agent, t)} (${agent.agent_key})`,
                }))}
              />
            </Form.Item>
            <Form.Item name="owner_user_id" label={t('field.ownerUserId')}>
              <Input disabled={user.role !== 'super_admin'} />
            </Form.Item>
          </div>
          <Button
            type="primary"
            icon={<PlayCircleOutlined />}
            htmlType="submit"
            loading={resolveMutation.isPending}
          >
            {t('runtime.resolve')}
          </Button>
        </Form>
      </div>
      {data ? (
        <Space direction="vertical" size={20} className="runtime-results">
          <Typography.Title level={4}>MCP</Typography.Title>
          <Table
            rowKey={(row) => row.binding.id}
            columns={mcpColumns}
            dataSource={data.mcps}
            tableLayout="fixed"
            scroll={{ x: 1050 }}
          />
          <Typography.Title level={4}>{t('resource.skill')}</Typography.Title>
          <Table
            rowKey={(row) => row.binding.id}
            columns={skillColumns}
            dataSource={data.skills}
            tableLayout="fixed"
            scroll={{ x: 980 }}
          />
          <Typography.Title level={4}>{t('runtime.localRequirements')}</Typography.Title>
          <Table
            rowKey={(row) => `${row.resource_kind}:${row.resource_id}`}
            columns={localColumns}
            dataSource={data.local_connector_requirements}
            tableLayout="fixed"
            scroll={{ x: 1050 }}
          />
        </Space>
      ) : null}
    </div>
  );
}

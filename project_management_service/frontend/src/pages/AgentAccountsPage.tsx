import { ReloadOutlined, TeamOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { Button, Empty, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api } from '../api/client';
import type { AgentAccountListItem } from '../types';

export function AgentAccountsPage() {
  const accountsQuery = useQuery({
    queryKey: ['agent-accounts'],
    queryFn: () => api.listAgentAccounts(),
  });

  const columns: ColumnsType<AgentAccountListItem> = [
    {
      title: 'Agent 账号',
      dataIndex: 'username',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>
            {record.display_name || record.username}
          </Typography.Text>
          <Typography.Text type="secondary">{record.username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '归属真实用户',
      dataIndex: 'owner_user_id',
      width: 240,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text>
            {record.owner_display_name || record.owner_username || record.owner_user_id}
          </Typography.Text>
          <Typography.Text type="secondary">{record.owner_username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      width: 100,
      render: (enabled: boolean) => (
        <Tag color={enabled ? 'success' : 'default'}>
          {enabled ? '启用' : '停用'}
        </Tag>
      ),
    },
    {
      title: '最近登录',
      dataIndex: 'last_login_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
  ];

  return (
    <div className="page">
      <div className="page-header">
        <Space direction="vertical" size={4}>
          <Space size={10}>
            <TeamOutlined style={{ color: '#1677ff' }} />
            <Typography.Title level={3} style={{ margin: 0 }}>
              Agent 账号
            </Typography.Title>
          </Space>
          <Typography.Text type="secondary">
            项目管理 MCP 复用 User Service 的 Agent 账号；程序会自动兑换 agent token 并注入真实用户 token。
          </Typography.Text>
        </Space>
        <Button
          icon={<ReloadOutlined />}
          onClick={() => accountsQuery.refetch()}
          loading={accountsQuery.isFetching}
        >
          刷新
        </Button>
      </div>

      <Table<AgentAccountListItem>
        rowKey="id"
        columns={columns}
        dataSource={accountsQuery.data || []}
        loading={accountsQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description="暂无 Agent 账号"
            />
          ),
        }}
      />
    </div>
  );
}

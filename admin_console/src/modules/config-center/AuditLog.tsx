import { Card, Table } from 'antd';
import { useQuery } from '@tanstack/react-query';
import dayjs from 'dayjs';

import { api } from './api';

export function AuditLog() {
  const query = useQuery({ queryKey: ['config-center', 'audit'], queryFn: api.audit });

  return (
    <Card>
      <Table
        rowKey="id"
        loading={query.isLoading}
        dataSource={query.data || []}
        columns={[
          { title: '时间', dataIndex: 'created_at', render: (value) => dayjs(value).format('YYYY-MM-DD HH:mm:ss') },
          { title: '环境', dataIndex: 'environment', render: (value) => value || '-' },
          { title: '动作', dataIndex: 'action' },
          { title: '操作者', dataIndex: 'actor_display_name' },
          { title: '变更 Key', dataIndex: 'changed_keys', render: (values: string[]) => values.join(', ') || '-' },
        ]}
      />
    </Card>
  );
}

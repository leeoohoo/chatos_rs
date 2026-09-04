import { Card, Table, Tag } from 'antd';
import { useQuery } from '@tanstack/react-query';
import dayjs from 'dayjs';

import { api } from './api';

export function Instances() {
  const query = useQuery({
    queryKey: ['config-center', 'instances'],
    queryFn: api.instances,
    refetchInterval: 10000,
  });

  return (
    <Card>
      <Table
        rowKey="id"
        loading={query.isLoading}
        dataSource={query.data || []}
        columns={[
          { title: '环境', dataIndex: 'environment' },
          { title: '服务', dataIndex: 'service_name' },
          { title: '实例', dataIndex: 'service_id' },
          { title: 'Revision', dataIndex: 'effective_revision' },
          { title: '状态', render: (_, item) => item.stale ? <Tag color="orange">stale</Tag> : <Tag color="green">active</Tag> },
          { title: '待重启 Key', dataIndex: 'pending_restart_keys', render: (values: string[]) => values.join(', ') || '-' },
          { title: '最后心跳', dataIndex: 'last_seen_at', render: (value) => dayjs(value).format('YYYY-MM-DD HH:mm:ss') },
        ]}
      />
    </Card>
  );
}

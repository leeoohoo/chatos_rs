import { App as AntdApp, Button, Card, Table, Tag } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import dayjs from 'dayjs';

import { api } from './api';
import type { ConfigRelease } from './types';

export function ReleaseHistory({ environment }: { environment: string }) {
  const { message, modal } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const releases = useQuery({
    queryKey: ['config-center', 'releases', environment],
    queryFn: () => api.releases(environment),
  });
  const rollback = useMutation({
    mutationFn: (release: ConfigRelease) => api.rollback(environment, release.id),
    onSuccess: async (release) => {
      message.success(`已回滚并生成 Revision ${release.revision}`);
      await queryClient.invalidateQueries();
    },
    onError: (error: Error) => message.error(error.message),
  });

  return (
    <Card>
      <Table
        rowKey="id"
        loading={releases.isLoading}
        dataSource={releases.data || []}
        columns={[
          { title: 'Revision', dataIndex: 'revision', render: (value) => <Tag color="blue">r{value}</Tag> },
          { title: '状态', dataIndex: 'status', render: (value) => <Tag color={value === 'published' ? 'green' : 'red'}>{value}</Tag> },
          { title: '说明', dataIndex: 'publish_message' },
          { title: '变更', dataIndex: 'changed_keys', render: (values: string[]) => values.length },
          { title: '发布时间', dataIndex: 'published_at', render: (value) => value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-' },
          {
            title: '操作',
            render: (_, release: ConfigRelease) => (
              <Button
                size="small"
                onClick={() => modal.confirm({
                  title: `回滚到 Revision ${release.revision}？`,
                  content: '回滚会创建一个新的发布版本，不会删除历史记录。',
                  onOk: () => rollback.mutateAsync(release),
                })}
              >
                回滚
              </Button>
            ),
          },
        ]}
      />
    </Card>
  );
}

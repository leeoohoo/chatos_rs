import { Button, Card, Table } from 'antd';

import type { UserItem } from '../../types';

interface UserSelectorCardProps {
  users: UserItem[];
  usersLoading: boolean;
  targetUserId: string;
  userIdTitle: string;
  roleTitle: string;
  actionTitle: string;
  title: string;
  viewConfigLabel: string;
  onSelectUser: (userId: string) => void;
}

export function UserSelectorCard({
  users,
  usersLoading,
  targetUserId,
  userIdTitle,
  roleTitle,
  actionTitle,
  title,
  viewConfigLabel,
  onSelectUser,
}: UserSelectorCardProps) {
  return (
    <Card size="small" title={title} style={{ marginBottom: 12 }}>
      <Table<UserItem>
        rowKey="username"
        loading={usersLoading}
        dataSource={users}
        pagination={false}
        size="small"
        columns={[
          {
            title: userIdTitle,
            dataIndex: 'username',
            key: 'username',
          },
          {
            title: roleTitle,
            dataIndex: 'role',
            key: 'role',
          },
          {
            title: actionTitle,
            key: 'action',
            width: 160,
            render: (_, record) => (
              <Button
                type={targetUserId === record.username ? 'primary' : 'default'}
                onClick={() => onSelectUser(record.username)}
              >
                {viewConfigLabel}
              </Button>
            ),
          },
        ]}
      />
    </Card>
  );
}

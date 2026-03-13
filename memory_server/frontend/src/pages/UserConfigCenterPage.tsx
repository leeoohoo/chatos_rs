import { ArrowLeftOutlined } from '@ant-design/icons';
import { Button, Card, Space, Tabs, Tag } from 'antd';

import { useI18n } from '../i18n';
import { JobConfigsPage } from './JobConfigsPage';
import { ModelConfigsPage } from './ModelConfigsPage';

interface UserConfigCenterPageProps {
  userId: string;
  isAdmin: boolean;
  currentUserId: string;
  onBack: () => void;
}

export function UserConfigCenterPage({
  userId,
  isAdmin,
  currentUserId,
  onBack,
}: UserConfigCenterPageProps) {
  const { t } = useI18n();

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <Card
        size="small"
        title={t('users.configCenterTitle')}
        extra={
          <Space>
            <Tag color="blue">
              {t('users.userId')}: {userId}
            </Tag>
            <Button icon={<ArrowLeftOutlined />} onClick={onBack}>
              {t('common.back')}
            </Button>
          </Space>
        }
      >
        {t('users.configCenterDesc')}
      </Card>

      <Tabs
        defaultActiveKey="models"
        items={[
          {
            key: 'models',
            label: t('users.modelTab'),
            children: (
              <ModelConfigsPage
                userId={userId}
                isAdmin={isAdmin}
                currentUserId={currentUserId}
                showUserSelector={false}
              />
            ),
          },
          {
            key: 'jobs',
            label: t('users.jobTab'),
            children: (
              <JobConfigsPage
                userId={userId}
                isAdmin={isAdmin}
                showUserSelector={false}
              />
            ),
          },
        ]}
      />
    </Space>
  );
}

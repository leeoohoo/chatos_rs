import { ArrowLeftOutlined } from '@ant-design/icons';
import { Button, Card, Space, Tabs, Tag } from 'antd';

import { useI18n } from '../i18n';
import { AgentsPage } from './AgentsPage';
import { ContactMemoriesPage } from './ContactMemoriesPage';
import { JobConfigsPage } from './JobConfigsPage';
import { ModelConfigsPage } from './ModelConfigsPage';
import { SkillsPage } from './SkillsPage';

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
          {
            key: 'skills',
            label: t('users.skillsTab'),
            children: (
              <SkillsPage
                filterUserId={userId}
                currentUserId={currentUserId}
                isAdmin={isAdmin}
              />
            ),
          },
          {
            key: 'agents',
            label: t('users.agentsTab'),
            children: (
              <AgentsPage
                filterUserId={userId}
                currentUserId={currentUserId}
                isAdmin={isAdmin}
              />
            ),
          },
          {
            key: 'project-memories',
            label: t('users.projectSummaryTab'),
            children: (
              <ContactMemoriesPage
                filterUserId={userId}
                currentUserId={currentUserId}
                isAdmin={isAdmin}
                mode="project"
              />
            ),
          },
          {
            key: 'agent-recalls',
            label: t('users.agentRecallTab'),
            children: (
              <ContactMemoriesPage
                filterUserId={userId}
                currentUserId={currentUserId}
                isAdmin={isAdmin}
                mode="recall"
              />
            ),
          },
        ]}
      />
    </Space>
  );
}

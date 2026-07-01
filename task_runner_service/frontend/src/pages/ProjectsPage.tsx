// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import {
  Button,
  Empty,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type { TaskProjectRecord } from '../types';

const PUBLIC_PROJECT_ID = '-1';

export function ProjectsPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const projectsQuery = useQuery({
    queryKey: ['task-projects'],
    queryFn: () => api.listProjects(),
  });

  const columns = useMemo<ColumnsType<TaskProjectRecord>>(
    () => [
      {
        title: t('projects.column.project'),
        dataIndex: 'name',
        width: 260,
        render: (_, project) => (
          <Space direction="vertical" size={2}>
            <Space size={[6, 6]} wrap>
              <Typography.Text strong>
                {project.id === PUBLIC_PROJECT_ID ? t('projects.public') : project.name}
              </Typography.Text>
              {project.id === PUBLIC_PROJECT_ID ? <Tag>{PUBLIC_PROJECT_ID}</Tag> : null}
            </Space>
            <Typography.Text type="secondary" copyable>
              {project.id}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('projects.column.status'),
        dataIndex: 'status',
        width: 120,
        render: (status: TaskProjectRecord['status']) => (
          <Tag color={status === 'active' ? 'success' : 'default'}>
            {t(`projects.status.${status}`)}
          </Tag>
        ),
      },
      {
        title: t('projects.column.rootPath'),
        dataIndex: 'root_path',
        width: 260,
        render: (value?: string | null) => value || '-',
      },
      {
        title: t('projects.column.gitUrl'),
        dataIndex: 'git_url',
        width: 260,
        render: (value?: string | null) => value || '-',
      },
      {
        title: t('projects.column.owner'),
        dataIndex: 'owner_display_name',
        width: 180,
        render: (_, project) =>
          project.owner_display_name || project.owner_username || project.owner_user_id || '-',
      },
      {
        title: t('common.updatedAt'),
        dataIndex: 'updated_at',
        width: 180,
        render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
      },
      {
        title: t('common.actions'),
        key: 'actions',
        width: 160,
        render: (_, project) => (
          <Button
            size="small"
            onClick={() => navigate(`/tasks?project_id=${encodeURIComponent(project.id)}`)}
          >
            {t('projects.viewTasks')}
          </Button>
        ),
      },
    ],
    [navigate, t],
  );

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space style={{ justifyContent: 'space-between', width: '100%' }}>
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            {t('projects.title')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('projects.subtitle')}
          </Typography.Text>
        </Space>
        <Button onClick={() => projectsQuery.refetch()}>{t('common.refresh')}</Button>
      </Space>

      <Table<TaskProjectRecord>
        rowKey="id"
        loading={projectsQuery.isLoading}
        columns={columns}
        dataSource={projectsQuery.data || []}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        scroll={{ x: 1420 }}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={t('projects.empty')}
            />
          ),
        }}
      />
    </Space>
  );
}

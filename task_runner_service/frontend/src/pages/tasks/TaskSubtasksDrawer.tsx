import { Button, Drawer, Empty, List, Space, Tag, Typography } from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRecord, TaskStatus } from '../../types';
import { statusColorMap } from './taskPageUtils';

type TaskSubtasksDrawerProps = {
  t: TranslateFn;
  open: boolean;
  parentTask: TaskRecord | null;
  tasks?: TaskRecord[];
  loading: boolean;
  taskStatusLabel: (status: TaskStatus) => string;
  onClose: () => void;
  onOpenDetail: (task: TaskRecord) => void;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
};

export function TaskSubtasksDrawer({
  t,
  open,
  parentTask,
  tasks,
  loading,
  taskStatusLabel,
  onClose,
  onOpenDetail,
  onOpenRunHistory,
}: TaskSubtasksDrawerProps) {
  return (
    <Drawer
      title={parentTask
        ? t('tasks.subtasks.titleWithName', { title: parentTask.title })
        : t('tasks.subtasks.title')}
      open={open}
      width={680}
      onClose={onClose}
    >
      {tasks?.length ? (
        <List
          bordered
          dataSource={tasks}
          renderItem={(task) => (
            <List.Item
              actions={[
                <Button key="detail" size="small" onClick={() => onOpenDetail(task)}>
                  {t('tasks.action.detail')}
                </Button>,
                <Button
                  key="history"
                  size="small"
                  onClick={() => onOpenRunHistory(task.id)}
                >
                  {t('tasks.action.history')}
                </Button>,
              ]}
            >
              <Space direction="vertical" size={4} style={{ width: '100%' }}>
                <Space wrap>
                  <Typography.Text strong>{task.title}</Typography.Text>
                  <Tag color={statusColorMap[task.status]}>
                    {taskStatusLabel(task.status)}
                  </Tag>
                  {task.prerequisite_task_ids.length ? (
                    <Tag>
                      {t('tasks.subtasks.prerequisiteCount', {
                        count: task.prerequisite_task_ids.length,
                      })}
                    </Tag>
                  ) : null}
                </Space>
                <Typography.Paragraph
                  type="secondary"
                  ellipsis={{ rows: 2 }}
                  style={{ marginBottom: 0 }}
                >
                  {task.description || task.objective || t('tasks.detail.noSummary')}
                </Typography.Paragraph>
                {task.result_summary ? (
                  <Typography.Paragraph
                    type="secondary"
                    ellipsis={{ rows: 2 }}
                    style={{ marginBottom: 0 }}
                  >
                    {task.result_summary}
                  </Typography.Paragraph>
                ) : null}
                <Typography.Text type="secondary">
                  {dayjs(task.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                </Typography.Text>
              </Space>
            </List.Item>
          )}
        />
      ) : loading ? null : (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={t('tasks.subtasks.empty')}
        />
      )}
    </Drawer>
  );
}

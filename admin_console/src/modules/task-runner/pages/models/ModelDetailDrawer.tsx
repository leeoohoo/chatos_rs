// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  List,
  Space,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ModelConfigRecord, TaskRecord, TaskRunRecord } from '../../types';

type ModelDetailDrawerProps = {
  t: TranslateFn;
  open: boolean;
  selectedModel: ModelConfigRecord | null;
  loading: boolean;
  taskCount: number;
  runCount: number;
  boundTasks?: TaskRecord[];
  boundTasksLoading: boolean;
  recentRuns?: TaskRunRecord[];
  recentRunsLoading: boolean;
  testing: boolean;
  onClose: () => void;
  onViewTasks: (modelId: string) => void;
  onViewRuns: (modelId: string) => void;
  onTest: (modelId: string) => void;
  onEdit: (model: ModelConfigRecord) => void;
  onOpenTask: (taskId: string) => void;
  onOpenRun: (runId: string) => void;
};

export function ModelDetailDrawer({
  t,
  open,
  selectedModel,
  loading,
  taskCount,
  runCount,
  boundTasks,
  boundTasksLoading,
  recentRuns,
  recentRunsLoading,
  testing,
  onClose,
  onViewTasks,
  onViewRuns,
  onTest,
  onEdit,
  onOpenTask,
  onOpenRun,
}: ModelDetailDrawerProps) {
  return (
    <Drawer
      title={selectedModel
        ? t('models.detail.titleWithName', { name: selectedModel.name })
        : t('models.detail.title')}
      open={open}
      width={760}
      onClose={onClose}
    >
      {selectedModel ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space wrap>
            <Button onClick={() => onViewTasks(selectedModel.id)}>
              {t('models.detail.viewTasks')}
            </Button>
            <Button onClick={() => onViewRuns(selectedModel.id)}>
              {t('models.detail.viewRuns')}
            </Button>
            <Button loading={testing} onClick={() => onTest(selectedModel.id)}>
              {t('models.detail.testConnection')}
            </Button>
            <Button onClick={() => onEdit(selectedModel)}>
              {t('models.detail.editConfig')}
            </Button>
          </Space>

          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('models.detail.modelId')}>
              {selectedModel.id}
            </Descriptions.Item>
            <Descriptions.Item label="Provider">{selectedModel.provider}</Descriptions.Item>
            <Descriptions.Item label="Model">{selectedModel.model}</Descriptions.Item>
            <Descriptions.Item label={t('models.column.usageScenario')}>
              {selectedModel.usage_scenario || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Base URL">{selectedModel.base_url}</Descriptions.Item>
            <Descriptions.Item label={t('common.status')}>
              <Tag color={selectedModel.enabled ? 'success' : 'default'}>
                {selectedModel.enabled ? t('common.enabled') : t('common.disabled')}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Supports Responses">
              {selectedModel.supports_responses ? t('common.yes') : t('common.no')}
            </Descriptions.Item>
            <Descriptions.Item label="Temperature">
              {selectedModel.temperature ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Max Output Tokens">
              {selectedModel.max_output_tokens ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Thinking Level">
              {selectedModel.thinking_level || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Request CWD">
              {selectedModel.request_cwd || '-'}
            </Descriptions.Item>
            <Descriptions.Item label="Prompt Cache Retention">
              {selectedModel.include_prompt_cache_retention
                ? t('common.enabled')
                : t('common.disabled')}
            </Descriptions.Item>
            <Descriptions.Item label="Request Body Limit">
              {selectedModel.request_body_limit_bytes ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('models.column.boundTasks')}>
              {taskCount}
            </Descriptions.Item>
            <Descriptions.Item label={t('models.column.runCount')}>
              {runCount}
            </Descriptions.Item>
            <Descriptions.Item label={t('models.detail.createdAt')}>
              {dayjs(selectedModel.created_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
            <Descriptions.Item label={t('common.updatedAt')}>
              {dayjs(selectedModel.updated_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
          </Descriptions>

          {selectedModel.instructions ? (
            <div>
              <Typography.Title level={5}>Instructions</Typography.Title>
              <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
                {selectedModel.instructions}
              </Typography.Paragraph>
            </div>
          ) : null}

          <div>
            <Typography.Title level={5}>{t('models.detail.boundTasks')}</Typography.Title>
            {boundTasks?.length ? (
              <List
                bordered
                dataSource={boundTasks}
                renderItem={(task) => (
                  <List.Item
                    actions={[
                      <Button key="task" size="small" onClick={() => onOpenTask(task.id)}>
                        {t('common.open')}
                      </Button>,
                    ]}
                  >
                    <Space direction="vertical" size={4} style={{ width: '100%' }}>
                      <Space wrap>
                        <Typography.Text strong>{task.title}</Typography.Text>
                        <Tag>{task.status}</Tag>
                      </Space>
                      <Typography.Paragraph
                        type="secondary"
                        ellipsis={{ rows: 2 }}
                        style={{ marginBottom: 0 }}
                      >
                        {task.objective}
                      </Typography.Paragraph>
                    </Space>
                  </List.Item>
                )}
              />
            ) : boundTasksLoading ? null : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('models.detail.noBoundTasks')}
              />
            )}
          </div>

          <div>
            <Typography.Title level={5}>{t('models.detail.recentRuns')}</Typography.Title>
            {recentRuns?.length ? (
              <List
                bordered
                dataSource={recentRuns}
                renderItem={(run) => (
                  <List.Item
                    actions={[
                      <Button key="run" size="small" onClick={() => onOpenRun(run.id)}>
                        {t('common.open')}
                      </Button>,
                    ]}
                  >
                    <Space direction="vertical" size={4} style={{ width: '100%' }}>
                      <Space wrap>
                        <Typography.Text code>{run.id.slice(0, 12)}</Typography.Text>
                        <Tag>{run.status}</Tag>
                        <Typography.Text type="secondary">
                          {dayjs(run.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                        </Typography.Text>
                      </Space>
                      {run.result_summary ? (
                        <Typography.Paragraph
                          type="secondary"
                          ellipsis={{ rows: 2 }}
                          style={{ marginBottom: 0 }}
                        >
                          {run.result_summary}
                        </Typography.Paragraph>
                      ) : (
                        <Typography.Text type="secondary">
                          {t('models.detail.noSummary')}
                        </Typography.Text>
                      )}
                    </Space>
                  </List.Item>
                )}
              />
            ) : recentRunsLoading ? null : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('models.detail.noRuns')}
              />
            )}
          </div>
        </Space>
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </Drawer>
  );
}

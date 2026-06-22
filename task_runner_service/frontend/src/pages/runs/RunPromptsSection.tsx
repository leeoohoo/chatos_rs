import {
  Button,
  Empty,
  List,
  Pagination,
  Space,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  UiPromptRecord,
  UiPromptStatus,
} from '../../types';
import { promptColorMap } from './runPageUtils';

type RunPromptsSectionProps = {
  t: TranslateFn;
  prompts: UiPromptRecord[];
  loading: boolean;
  page: number;
  pageSize: number;
  total: number;
  promptStatusLabel: (status: UiPromptStatus) => string;
  onOpenPrompt: (promptId: string) => void;
  onPageChange: (page: number, pageSize: number) => void;
};

export function RunPromptsSection({
  t,
  prompts,
  loading,
  page,
  pageSize,
  total,
  promptStatusLabel,
  onOpenPrompt,
  onPageChange,
}: RunPromptsSectionProps) {
  return (
    <div>
      <Typography.Title level={5}>{t('runs.prompts.title')}</Typography.Title>
      {prompts.length ? (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <List
            bordered
            dataSource={prompts}
            renderItem={(prompt) => (
              <List.Item
                actions={[
                  <Button
                    key="open-prompt"
                    size="small"
                    onClick={() => onOpenPrompt(prompt.id)}
                  >
                    {t('common.open')}
                  </Button>,
                ]}
              >
                <Space
                  direction="vertical"
                  size={2}
                  style={{ width: '100%', alignItems: 'flex-start' }}
                >
                  <Space wrap>
                    <Typography.Text strong>
                      {prompt.title || prompt.message || prompt.kind}
                    </Typography.Text>
                    <Tag color={promptColorMap[prompt.status]}>
                      {promptStatusLabel(prompt.status)}
                    </Tag>
                    <Typography.Text code>{prompt.id.slice(0, 12)}</Typography.Text>
                  </Space>
                  {prompt.message ? (
                    <Typography.Text>{prompt.message}</Typography.Text>
                  ) : null}
                  <Typography.Text type="secondary">
                    {dayjs(prompt.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                  </Typography.Text>
                </Space>
              </List.Item>
            )}
          />
          <Pagination
            size="small"
            current={page}
            pageSize={pageSize}
            total={total}
            showSizeChanger
            onChange={onPageChange}
          />
        </Space>
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </div>
  );
}

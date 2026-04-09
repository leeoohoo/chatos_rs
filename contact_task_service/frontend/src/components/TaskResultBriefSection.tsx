import { ReloadOutlined } from '@ant-design/icons';
import { Alert, Button, Card, Space, Spin, Tag, Typography } from 'antd';

import { formatTaskStatusColor } from '../appHelpers';
import type { TaskResultBrief } from '../types';

const { Text, Paragraph } = Typography;

type TaskResultBriefSectionProps = {
  resultBrief: TaskResultBrief | null | undefined;
  loading: boolean;
  error?: string | null;
  onRefresh: () => Promise<void> | void;
};

export function TaskResultBriefSection({
  resultBrief,
  loading,
  error,
  onRefresh,
}: TaskResultBriefSectionProps) {
  return (
    <>
      <Text strong>任务结果桥接摘要</Text>
      <Space>
        <Button
          size="small"
          icon={<ReloadOutlined />}
          loading={loading}
          onClick={() => { void onRefresh(); }}
        >
          刷新结果桥接
        </Button>
      </Space>
      {error && (
        <Alert
          type="error"
          showIcon
          message={error || '加载任务结果桥接失败'}
        />
      )}
      {loading && (
        <Space>
          <Spin size="small" />
          <Text type="secondary">任务结果桥接加载中...</Text>
        </Space>
      )}
      {!loading && !error && resultBrief === null && (
        <Text type="secondary">当前任务还没有生成结果桥接摘要，通常会在任务进入终态后出现。</Text>
      )}
      {resultBrief && (
        <Card size="small" bodyStyle={{ padding: 12 }} style={{ width: '100%' }}>
          <Space direction="vertical" size={6} style={{ width: '100%' }}>
            <Space wrap>
              <Tag color={formatTaskStatusColor(resultBrief.task_status)}>
                {resultBrief.task_status}
              </Tag>
              {resultBrief.result_format && (
                <Tag>{resultBrief.result_format}</Tag>
              )}
              {resultBrief.finished_at && (
                <Text type="secondary">
                  完成时间:
                  {' '}
                  {new Date(resultBrief.finished_at).toLocaleString()}
                </Text>
              )}
            </Space>
            <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
              {resultBrief.result_summary}
            </Paragraph>
            {resultBrief.result_message_id && (
              <Text type="secondary">
                结果消息 ID:
                {' '}
                {resultBrief.result_message_id}
              </Text>
            )}
            {resultBrief.source_session_id && (
              <Text type="secondary">
                来源会话:
                {' '}
                {resultBrief.source_session_id}
              </Text>
            )}
          </Space>
        </Card>
      )}
    </>
  );
}

import { ReloadOutlined } from '@ant-design/icons';
import { Alert, Button, Collapse, Pagination, Space, Spin, Tag, Typography } from 'antd';

import { EXECUTION_PAGE_SIZE } from '../appHelpers';
import type { TaskExecutionMessage } from '../types';
import { ExecutionMessageCard } from './ExecutionMessageCard';

const { Text } = Typography;

type TaskExecutionSectionProps = {
  expanded: boolean;
  executionMessages: TaskExecutionMessage[];
  loading: boolean;
  error?: string | null;
  pagedExecutionMessages: TaskExecutionMessage[];
  safeExecutionPage: number;
  onExpandedChange: (expanded: boolean) => void;
  onRefresh: () => Promise<void> | void;
  onPageChange: (page: number) => void;
};

export function TaskExecutionSection({
  expanded,
  executionMessages,
  loading,
  error,
  pagedExecutionMessages,
  safeExecutionPage,
  onExpandedChange,
  onRefresh,
  onPageChange,
}: TaskExecutionSectionProps) {
  return (
    <Collapse
      ghost
      activeKey={expanded ? ['execution'] : []}
      onChange={(keys) => {
        const nextKeys = Array.isArray(keys) ? keys : [keys];
        onExpandedChange(nextKeys.includes('execution'));
      }}
      items={[
        {
          key: 'execution',
          label: (
            <Space wrap>
              <Text strong>执行过程</Text>
              <Tag>{executionMessages.length} 条</Tag>
              {loading && <Tag color="processing">加载中</Tag>}
            </Space>
          ),
          children: (
            <Space direction="vertical" size={10} style={{ width: '100%' }}>
              <Space>
                <Button
                  size="small"
                  icon={<ReloadOutlined />}
                  loading={loading}
                  onClick={(event) => {
                    event.stopPropagation();
                    void onRefresh();
                  }}
                >
                  刷新执行记录
                </Button>
              </Space>
              {error && (
                <Alert
                  type="error"
                  showIcon
                  message={error || '加载执行过程失败'}
                />
              )}
              {loading && (
                <Space>
                  <Spin size="small" />
                  <Text type="secondary">执行记录加载中...</Text>
                </Space>
              )}
              {!loading && !error && executionMessages.length === 0 && (
                <Text type="secondary">暂无执行记录。</Text>
              )}
              {pagedExecutionMessages.map((message) => (
                <ExecutionMessageCard key={message.id} message={message} />
              ))}
              {executionMessages.length > EXECUTION_PAGE_SIZE && (
                <Pagination
                  align="end"
                  current={safeExecutionPage}
                  pageSize={EXECUTION_PAGE_SIZE}
                  total={executionMessages.length}
                  showSizeChanger={false}
                  onChange={(page) => {
                    onPageChange(page);
                  }}
                />
              )}
            </Space>
          ),
        },
      ]}
    />
  );
}

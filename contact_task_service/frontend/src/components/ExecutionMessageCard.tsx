import { Card, Collapse, Space, Tag, Typography } from 'antd';

import {
  extractToolEntries,
  getToolEntryLabel,
  stringifyPretty,
  truncateText,
} from '../appHelpers';
import type { TaskExecutionMessage } from '../types';

const { Text, Paragraph } = Typography;

type ExecutionMessageCardProps = {
  message: TaskExecutionMessage;
};

export function ExecutionMessageCard({ message }: ExecutionMessageCardProps) {
  const toolEntries = extractToolEntries(message.tool_calls);
  const hasToolDetails =
    message.role === 'tool'
    || toolEntries.length > 0
    || Boolean(message.tool_call_id);

  return (
    <Card
      key={message.id}
      size="small"
      bodyStyle={{ padding: 12 }}
      style={{ width: '100%' }}
    >
      <Space direction="vertical" size={6} style={{ width: '100%' }}>
        <Space wrap>
          <Tag color={message.role === 'assistant' ? 'blue' : message.role === 'tool' ? 'purple' : 'default'}>
            {message.role}
          </Tag>
          {message.message_source && <Tag>{message.message_source}</Tag>}
          {message.summary_status && <Tag>{message.summary_status}</Tag>}
          <Text type="secondary">{new Date(message.created_at).toLocaleString()}</Text>
        </Space>
        {hasToolDetails ? (
          <>
            <Text type="secondary">
              {truncateText(message.content, 160)}
            </Text>
            <Collapse
              ghost
              items={[
                {
                  key: 'tool-details',
                  label: (
                    <Space wrap>
                      <Text>工具调用详情</Text>
                      {toolEntries.length > 0 && <Tag>{toolEntries.length} 个</Tag>}
                    </Space>
                  ),
                  children: (
                    <Space direction="vertical" size={8} style={{ width: '100%' }}>
                      <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                        {message.content || '-'}
                      </Paragraph>
                      {message.tool_call_id && (
                        <Text type="secondary">
                          tool_call_id:
                          {' '}
                          {message.tool_call_id}
                        </Text>
                      )}
                      {toolEntries.length > 0 && (
                        <Space direction="vertical" size={8} style={{ width: '100%' }}>
                          {toolEntries.map((entry, index) => (
                            <Card
                              key={`${message.id}-tool-${index}`}
                              size="small"
                              bodyStyle={{ padding: 10 }}
                            >
                              <Space direction="vertical" size={6} style={{ width: '100%' }}>
                                <Text strong>{getToolEntryLabel(entry, index)}</Text>
                                <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                                  {stringifyPretty(entry)}
                                </pre>
                              </Space>
                            </Card>
                          ))}
                        </Space>
                      )}
                      {message.metadata && (
                        <Card size="small" bodyStyle={{ padding: 10 }}>
                          <Space direction="vertical" size={4} style={{ width: '100%' }}>
                            <Text strong>metadata</Text>
                            <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                              {stringifyPretty(message.metadata)}
                            </pre>
                          </Space>
                        </Card>
                      )}
                      {message.reasoning && (
                        <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                          {message.reasoning}
                        </Paragraph>
                      )}
                    </Space>
                  ),
                },
              ]}
            />
          </>
        ) : (
          <>
            <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
              {message.content || '-'}
            </Paragraph>
            {message.reasoning && (
              <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
                {message.reasoning}
              </Paragraph>
            )}
            {message.metadata && (
              <Collapse
                ghost
                items={[
                  {
                    key: 'message-metadata',
                    label: '附加信息',
                    children: (
                      <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                        {stringifyPretty(message.metadata)}
                      </pre>
                    ),
                  },
                ]}
              />
            )}
          </>
        )}
      </Space>
    </Card>
  );
}

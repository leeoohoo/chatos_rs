import { Space, Tag, Typography } from 'antd';

import { toLocal } from '../../utils';
import type { PolicySummaryProps } from './types';

const { Paragraph, Text } = Typography;

export function PolicySummary(props: PolicySummaryProps) {
  const { meta, updatedAt } = props;

  return (
    <>
      <Space>
        <Tag color={meta.tagColor}>{meta.tabLabel}</Tag>
        <Text strong>{meta.title}</Text>
        <Text type="secondary">更新时间 {toLocal(updatedAt)}</Text>
      </Space>
      <div
        style={{
          marginBottom: 20,
          padding: 16,
          borderRadius: 8,
          background: '#f7faf9',
          border: '1px solid #d9ebe8',
        }}
      >
        <Space direction="vertical" size={8} style={{ width: '100%' }}>
          <Paragraph style={{ marginBottom: 0 }}>{meta.description}</Paragraph>
          <Text>
            <Text strong>输入：</Text>
            {meta.inputText}
          </Text>
          <Text>
            <Text strong>输出：</Text>
            {meta.outputText}
          </Text>
          <Text>
            <Text strong>作用：</Text>
            {meta.purposeText}
          </Text>
          {meta.sharedPolicyHint ? (
            <Text type="secondary">
              <Text strong>说明：</Text>
              {meta.sharedPolicyHint}
            </Text>
          ) : null}
        </Space>
      </div>
    </>
  );
}

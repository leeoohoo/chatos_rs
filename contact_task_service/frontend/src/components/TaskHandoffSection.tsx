import { Card, Space, Tag, Typography } from 'antd';

import { formatHandoffKind } from '../appHelpers';
import type { TaskHandoffPayload } from '../types';

const { Text, Paragraph } = Typography;

type TaskHandoffSectionProps = {
  taskId: string;
  payload: TaskHandoffPayload;
};

export function TaskHandoffSection({ taskId, payload }: TaskHandoffSectionProps) {
  return (
    <>
      <Text strong>任务交接</Text>
      <Card size="small" bodyStyle={{ padding: 12 }} style={{ width: '100%' }}>
        <Space direction="vertical" size={6} style={{ width: '100%' }}>
          <Space wrap>
            <Tag color="magenta">{formatHandoffKind(payload.handoff_kind)}</Tag>
            {payload.generated_at && (
              <Text type="secondary">
                生成时间:
                {' '}
                {new Date(payload.generated_at).toLocaleString()}
              </Text>
            )}
          </Space>
          <Paragraph style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
            {payload.summary}
          </Paragraph>
          {payload.result_summary && (
            <Paragraph type="secondary" style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>
              {payload.result_summary}
            </Paragraph>
          )}
          {payload.key_changes.length > 0 && (
            <>
              <Text type="secondary">关键变化</Text>
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                {payload.key_changes.map((item, index) => (
                  <Text key={`${taskId}-handoff-change-${index}`}>{`${index + 1}. ${item}`}</Text>
                ))}
              </Space>
            </>
          )}
          {payload.verification_suggestions.length > 0 && (
            <>
              <Text type="secondary">验证建议</Text>
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                {payload.verification_suggestions.map((item, index) => (
                  <Text key={`${taskId}-handoff-verify-${index}`}>{`${index + 1}. ${item}`}</Text>
                ))}
              </Space>
            </>
          )}
          {payload.open_risks.length > 0 && (
            <>
              <Text type="secondary">遗留风险</Text>
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                {payload.open_risks.map((item, index) => (
                  <Text key={`${taskId}-handoff-risk-${index}`} type="danger">{`${index + 1}. ${item}`}</Text>
                ))}
              </Space>
            </>
          )}
          {payload.artifact_refs.length > 0 && (
            <>
              <Text type="secondary">关联引用</Text>
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                {payload.artifact_refs.map((item, index) => (
                  <Text key={`${taskId}-handoff-artifact-${index}`}>{item}</Text>
                ))}
              </Space>
            </>
          )}
        </Space>
      </Card>
    </>
  );
}

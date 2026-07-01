// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Collapse,
  Empty,
  Space,
  Tag,
  Typography,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import {
  CodeParagraph,
  describeStructuredValueSummary,
} from './payloadView';
import type {
  ToolCallView,
  ToolResultView,
} from './runEventUtils';

export function RunToolCallsSection({
  t,
  toolCalls,
}: {
  t: TranslateFn;
  toolCalls: ToolCallView[];
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('runs.tools.plan')}</Typography.Title>
      {toolCalls.length ? (
        <Collapse
          ghost
          items={toolCalls.map((toolCall, index) => ({
            key: `${toolCall.callId || toolCall.name}-${index}`,
            label: (
              <Space wrap>
                <Tag color="processing">{toolCall.name}</Tag>
                <Typography.Text code>
                  {toolCall.callId || 'no-call-id'}
                </Typography.Text>
                {toolCall.arguments ? (
                  <Typography.Text type="secondary">
                    {describeStructuredValueSummary(toolCall.arguments, t('runs.viewPayload'))}
                  </Typography.Text>
                ) : null}
              </Space>
            ),
            children: toolCall.arguments ? (
              <CodeParagraph value={toolCall.arguments} />
            ) : (
              <Typography.Text type="secondary">{t('runs.tools.noArguments')}</Typography.Text>
            ),
          }))}
        />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('runs.tools.noCalls')} />
      )}
    </div>
  );
}

export function RunToolResultsSection({
  t,
  toolResults,
}: {
  t: TranslateFn;
  toolResults: ToolResultView[];
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('runs.tools.results')}</Typography.Title>
      {toolResults.length ? (
        <Collapse
          ghost
          items={toolResults.map((result, index) => ({
            key: `${result.toolCallId || result.name}-${index}`,
            label: (
              <Space wrap>
                <Tag color={result.success ? 'success' : 'error'}>
                  {result.success ? t('common.success') : t('common.failed')}
                </Tag>
                <Typography.Text strong>{result.name}</Typography.Text>
                <Typography.Text code>
                  {result.toolCallId || 'no-call-id'}
                </Typography.Text>
              </Space>
            ),
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%' }}>
                <Typography.Text>{result.content || '-'}</Typography.Text>
                {result.result !== undefined ? (
                  <CodeParagraph value={result.result} />
                ) : null}
              </Space>
            ),
          }))}
        />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('runs.tools.noResults')} />
      )}
    </div>
  );
}

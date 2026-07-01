// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Collapse, Empty, Typography } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';

export function JsonBlock({
  title,
  value,
  t,
  collapsible = false,
  defaultOpen = true,
}: {
  title: string;
  value: unknown;
  t: TranslateFn;
  collapsible?: boolean;
  defaultOpen?: boolean;
}) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      {!value ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : collapsible ? (
        <Collapse
          ghost
          size="small"
          defaultActiveKey={defaultOpen ? ['content'] : []}
          items={[
            {
              key: 'content',
              label: (
                <Typography.Text type="secondary">
                  {describeStructuredValueSummary(
                    value,
                    t('runs.viewNamedPayload', { title }),
                  )}
                </Typography.Text>
              ),
              children: <CodeParagraph value={value} />,
            },
          ]}
        />
      ) : (
        <CodeParagraph value={value} />
      )}
    </div>
  );
}

export function CodeParagraph({ value }: { value: unknown }) {
  return (
    <Typography.Paragraph
      style={{
        background: '#fafafa',
        padding: 12,
        borderRadius: 6,
        marginBottom: 0,
        whiteSpace: 'pre-wrap',
        fontFamily: 'monospace',
      }}
    >
      {JSON.stringify(value, null, 2)}
    </Typography.Paragraph>
  );
}

export function CollapsiblePayload({
  value,
  t,
  defaultOpen = false,
}: {
  value: unknown;
  t: TranslateFn;
  defaultOpen?: boolean;
}) {
  return (
    <Collapse
      ghost
      size="small"
      defaultActiveKey={defaultOpen ? ['payload'] : []}
      items={[
        {
          key: 'payload',
          label: (
            <Typography.Text type="secondary">
              {describeStructuredValueSummary(value, t('runs.viewPayload'))}
            </Typography.Text>
          ),
          children: <CodeParagraph value={value} />,
        },
      ]}
    />
  );
}

export function describeStructuredValueSummary(
  value: unknown,
  labelPrefix: string,
): string {
  if (Array.isArray(value)) {
    return `${labelPrefix} (${value.length} items)`;
  }
  if (value && typeof value === 'object') {
    return `${labelPrefix} (${Object.keys(value as Record<string, unknown>).length} keys)`;
  }
  if (typeof value === 'string') {
    return `${labelPrefix} (${value.length} chars)`;
  }
  return labelPrefix;
}

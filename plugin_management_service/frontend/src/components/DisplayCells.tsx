// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import dayjs from 'dayjs';
import { Typography } from 'antd';

export function CompactId({ value }: { value?: string | null }) {
  if (!value) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  return (
    <Typography.Text className="compact-id" ellipsis={{ tooltip: value }} copyable={{ text: value }}>
      {value}
    </Typography.Text>
  );
}

export function DateTimeCell({ value }: { value?: string | null }) {
  if (!value) {
    return <Typography.Text type="secondary">-</Typography.Text>;
  }
  const parsed = dayjs(value);
  return (
    <Typography.Text className="nowrap-cell" title={value}>
      {parsed.isValid() ? parsed.format('YYYY-MM-DD HH:mm:ss') : value}
    </Typography.Text>
  );
}

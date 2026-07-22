// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Empty,
  Space,
  Timeline,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRunEventRecord } from '../../types';
import {
  RunEventPayload,
  describeRunEventType,
} from './runEventUtils';
import { formatUserVisibleRunText } from './runPageUtils';

export function RunEventsTimeline({
  t,
  events,
  loading,
}: {
  t: TranslateFn;
  events: TaskRunEventRecord[];
  loading: boolean;
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('runs.events.title')}</Typography.Title>
      {events.length ? (
        <Timeline
          items={events.map((event) => ({
            color:
              event.event_type.includes('failed')
                ? 'red'
                : event.event_type.includes('cancel')
                  ? 'gray'
                  : event.event_type.includes('completed')
                    ? 'green'
                    : 'blue',
            children: (
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                <Typography.Text strong>{describeRunEventType(event, t)}</Typography.Text>
                <Typography.Text type="secondary">
                  {dayjs(event.created_at).format('YYYY-MM-DD HH:mm:ss')}
                </Typography.Text>
                {event.message ? (
                  <Typography.Text>
                    {event.event_type.includes('failed')
                      || event.event_type.includes('error')
                      || event.event_type.includes('expired')
                      ? formatUserVisibleRunText(event.message, t)
                      : event.message}
                  </Typography.Text>
                ) : null}
                <RunEventPayload event={event} t={t} />
              </Space>
            ),
          }))}
        />
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </div>
  );
}

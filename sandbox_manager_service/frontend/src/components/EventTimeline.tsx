import { Empty, Timeline, Typography } from 'antd';
import dayjs from 'dayjs';

import { useI18n } from '../i18n';
import type { SandboxEventRecord } from '../types';

export function EventTimeline({ events }: { events: SandboxEventRecord[] }) {
  const { t, translateDynamic } = useI18n();

  if (!events.length) {
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('events.empty')} />;
  }

  return (
    <Timeline
      items={events.map((event) => ({
        children: (
          <div>
            <Typography.Text strong>
              {translateDynamic('event.type', event.event_type)}
            </Typography.Text>
            {event.message ? (
              <Typography.Text type="secondary">
                {' '}
                · {translateDynamic('event.message', event.message)}
              </Typography.Text>
            ) : null}
            <div className="muted-line">{dayjs(event.created_at).format('YYYY-MM-DD HH:mm:ss')}</div>
          </div>
        ),
      }))}
    />
  );
}

// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Collapse,
  Empty,
  Space,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRunEventRecord } from '../../types';
import { CollapsiblePayload } from './payloadView';

export function RunModelRequestsSection({
  t,
  modelRequests,
}: {
  t: TranslateFn;
  modelRequests: TaskRunEventRecord[];
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('runs.modelRequests.title')}</Typography.Title>
      {modelRequests.length ? (
        <Collapse
          ghost
          items={modelRequests.map((event, index) => ({
            key: `${event.id}-${index}`,
            label: (
              <Space wrap>
                <Typography.Text strong>
                  {t('runs.modelRequests.request', { index: index + 1 })}
                </Typography.Text>
                <Typography.Text type="secondary">
                  {dayjs(event.created_at).format('YYYY-MM-DD HH:mm:ss')}
                </Typography.Text>
              </Space>
            ),
            children: event.payload ? (
              <CollapsiblePayload value={event.payload} t={t} />
            ) : (
              <Typography.Text type="secondary">{t('runs.modelRequests.noPayload')}</Typography.Text>
            ),
          }))}
        />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('runs.modelRequests.empty')} />
      )}
    </div>
  );
}

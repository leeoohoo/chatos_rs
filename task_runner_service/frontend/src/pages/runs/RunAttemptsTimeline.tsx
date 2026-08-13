// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Empty, Space, Tag, Timeline, Typography } from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRunAttemptRecord, TaskRunAttemptStatus } from '../../types';

const attemptColor: Record<TaskRunAttemptStatus, string> = {
  running: 'blue',
  succeeded: 'green',
  failed: 'red',
  cancelled: 'default',
  blocked: 'orange',
  interrupted: 'gold',
};

export function RunAttemptsTimeline({
  t,
  attempts,
}: {
  t: TranslateFn;
  attempts: TaskRunAttemptRecord[];
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('runs.attempts.title')}</Typography.Title>
      {attempts.length ? (
        <Timeline
          items={attempts.map((attempt) => ({
            color: attemptColor[attempt.status],
            children: (
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                <Space wrap>
                  <Typography.Text strong>
                    {t('runs.attempts.sequence', { sequence: attempt.sequence })}
                  </Typography.Text>
                  <Tag color={attemptColor[attempt.status]}>
                    {t(`runs.attempts.status.${attempt.status}`)}
                  </Tag>
                  <Typography.Text type="secondary">
                    {t('runs.attempts.duration', {
                      duration: formatDuration(
                        attempt.started_at,
                        attempt.finished_at || undefined,
                      ),
                    })}
                  </Typography.Text>
                </Space>
                <Typography.Text type="secondary">
                  {dayjs(attempt.started_at).format('YYYY-MM-DD HH:mm:ss')}
                  {' - '}
                  {attempt.finished_at
                    ? dayjs(attempt.finished_at).format('YYYY-MM-DD HH:mm:ss')
                    : t('runs.attempts.status.running')}
                </Typography.Text>
                {attempt.recovery_reason ? (
                  <Typography.Text>
                    {t('runs.attempts.recoveryReason')}:{' '}
                    {t(`runs.attempts.reason.${attempt.recovery_reason}`)}
                  </Typography.Text>
                ) : null}
                {attempt.model_response_id ? (
                  <Typography.Text>
                    {t('runs.attempts.modelResponse')}: {attempt.model_response_id}
                  </Typography.Text>
                ) : null}
              </Space>
            ),
          }))}
        />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </div>
  );
}

function formatDuration(startedAt: string, finishedAt?: string): string {
  const seconds = Math.max(0, dayjs(finishedAt).diff(dayjs(startedAt), 'second'));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = seconds % 60;
  return [hours, minutes, remainingSeconds]
    .map((value) => String(value).padStart(2, '0'))
    .join(':');
}

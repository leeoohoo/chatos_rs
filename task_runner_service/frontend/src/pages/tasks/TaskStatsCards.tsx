// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Statistic } from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskStatsResponse } from '../../types';

type TaskStatsCardsProps = {
  t: TranslateFn;
  stats?: TaskStatsResponse;
  loading: boolean;
};

export function TaskStatsCards({ t, stats, loading }: TaskStatsCardsProps) {
  const items = [
    { title: t('tasks.stats.total'), value: stats?.total || 0 },
    { title: t('tasks.stats.scheduled'), value: stats?.scheduled || 0 },
    { title: t('tasks.stats.followUp'), value: stats?.follow_up || 0 },
    { title: t('tasks.stats.ready'), value: stats?.ready || 0 },
    { title: t('tasks.stats.queued'), value: stats?.queued || 0 },
    { title: t('tasks.stats.running'), value: stats?.running || 0 },
    { title: t('tasks.stats.succeeded'), value: stats?.succeeded || 0 },
    { title: t('tasks.stats.failed'), value: stats?.failed || 0 },
    { title: t('tasks.stats.blocked'), value: stats?.blocked || 0 },
  ];

  return (
    <div
      style={{
        display: 'grid',
        gap: 12,
        gridTemplateColumns: 'repeat(auto-fit, minmax(132px, 1fr))',
        width: '100%',
      }}
    >
      {items.map((item) => (
        <div
          key={item.title}
          style={{
            background: '#fff',
            border: '1px solid #f0f0f0',
            borderRadius: 8,
            padding: 16,
          }}
        >
          <Statistic title={item.title} value={item.value} loading={loading} />
        </div>
      ))}
    </div>
  );
}
